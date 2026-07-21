use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, Runtime};

// ==================== 清理电脑功能 ====================
//
// 安全原则：
// 1. 只清理白名单目录（用户临时目录、系统临时目录、浏览器缓存、缩略图缓存、日志）
// 2. 删除前捕获异常，正在占用 / 权限不足的文件直接跳过
// 3. 仅删除 7 天前的临时文件，避免误删正在使用的临时安装包
// 4. 不删除用户文档、注册表、系统核心目录
// 5. 后台线程执行，不阻塞主窗口

/// 清理统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanStats {
    /// 已扫描的文件数
    pub scanned: usize,
    /// 已删除的文件数
    pub deleted: usize,
    /// 跳过的文件数（占用 / 权限 / 过滤）
    pub skipped: usize,
    /// 释放的字节数
    pub freed_bytes: u64,
    /// 当前清理的类别
    pub current_category: String,
    /// 当前正在处理的路径
    pub current_path: String,
    /// 是否正在运行
    pub is_running: bool,
    /// 错误详情（最多保留 30 条）
    pub error_details: Vec<String>,
    /// 各类别清理结果汇总
    pub categories: Vec<CategoryResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryResult {
    pub name: String,
    pub deleted: usize,
    pub skipped: usize,
    pub freed_bytes: u64,
}

impl Default for CleanStats {
    fn default() -> Self {
        Self {
            scanned: 0,
            deleted: 0,
            skipped: 0,
            freed_bytes: 0,
            current_category: String::new(),
            current_path: String::new(),
            is_running: false,
            error_details: Vec::new(),
            categories: Vec::new(),
        }
    }
}

/// 清理电脑管理器（保存运行状态和最近一次结果）
pub struct CleanComputerManager {
    is_running: AtomicBool,
    last_stats: Mutex<Option<CleanStats>>,
}

impl CleanComputerManager {
    pub fn new() -> Self {
        Self {
            is_running: AtomicBool::new(false),
            last_stats: Mutex::new(None),
        }
    }

    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    pub fn set_running(&self, running: bool) {
        self.is_running.store(running, Ordering::SeqCst);
    }

    pub fn set_stats(&self, stats: CleanStats) {
        *self.last_stats.lock().unwrap() = Some(stats);
    }

    pub fn get_stats(&self) -> Option<CleanStats> {
        self.last_stats.lock().unwrap().clone()
    }
}

impl Default for CleanComputerManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 7 天的秒数：仅删除 7 天前修改过的临时文件
const SEVEN_DAYS_SECS: u64 = 7 * 24 * 60 * 60;

/// 安全删除单个文件：捕获所有异常，占用 / 权限不足直接跳过
fn safe_delete_file(path: &Path, stats: &mut CleanStats, require_old: bool) {
    stats.scanned += 1;
    stats.current_path = path.to_string_lossy().to_string();

    let metadata = match fs::metadata(path) {
        Ok(meta) => meta,
        Err(_) => {
            stats.skipped += 1;
            return;
        }
    };

    // 仅对临时文件应用 7 天过滤（避免误删刚下载的安装包）
    if require_old {
        let is_old = if let Ok(modified) = metadata.modified() {
            if let Ok(elapsed) = SystemTime::now().duration_since(modified) {
                elapsed.as_secs() >= SEVEN_DAYS_SECS
            } else {
                false
            }
        } else {
            false
        };
        if !is_old {
            stats.skipped += 1;
            return;
        }
    }

    let size = metadata.len();

    match fs::remove_file(path) {
        Ok(()) => {
            stats.deleted += 1;
            stats.freed_bytes += size;
        }
        Err(e) => {
            stats.skipped += 1;
            if stats.error_details.len() < 30 {
                stats.error_details.push(format!(
                    "跳过 {}: {}",
                    path.display(),
                    e
                ));
            }
        }
    }
}

/// 清理目录内容（保留目录本身）
/// `require_old` 是否要求文件 7 天以上才删除
/// `max_depth` 最大递归深度，防止无限递归
fn clean_dir_contents(dir: &Path, stats: &mut CleanStats, require_old: bool, max_depth: u32) {
    if max_depth == 0 || !dir.exists() {
        return;
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // 递归清理子目录内容
            clean_dir_contents(&path, stats, require_old, max_depth.saturating_sub(1));
            // 尝试删除空目录（失败也无所谓，可能是被占用）
            let _ = fs::remove_dir(&path);
        } else {
            safe_delete_file(&path, stats, require_old);
        }
    }
}

/// 清理指定类别下的多个目录
fn clean_category<R: Runtime>(
    name: &str,
    paths: Vec<PathBuf>,
    require_old: bool,
    stats: &mut CleanStats,
    app_handle: &tauri::AppHandle<R>,
) {
    println!("[清理电脑] 开始清理: {}", name);
    stats.current_category = name.to_string();

    let cat_start_deleted = stats.deleted;
    let cat_start_skipped = stats.skipped;
    let cat_start_freed = stats.freed_bytes;

    for path in &paths {
        if !path.exists() {
            continue;
        }
        stats.current_path = path.to_string_lossy().to_string();
        // 进度事件（每个目录开始时发送）
        let _ = app_handle.emit("clean-computer-progress", stats.clone());
        clean_dir_contents(path, stats, require_old, 6);
    }

    let cat_result = CategoryResult {
        name: name.to_string(),
        deleted: stats.deleted - cat_start_deleted,
        skipped: stats.skipped - cat_start_skipped,
        freed_bytes: stats.freed_bytes - cat_start_freed,
    };
    println!(
        "[清理电脑] {} 完成: 删除={}, 跳过={}, 释放 {} 字节",
        name, cat_result.deleted, cat_result.skipped, cat_result.freed_bytes
    );
    stats.categories.push(cat_result);

    // 类别完成时发送进度
    let _ = app_handle.emit("clean-computer-progress", stats.clone());
}

/// 在后台线程执行清理（不阻塞主窗口）
pub fn run_clean_computer<R: Runtime>(app_handle: tauri::AppHandle<R>) {
    println!("[清理电脑] 后台清理线程启动");

    let mut stats = CleanStats::default();
    stats.is_running = true;
    stats.current_category = "初始化".to_string();

    // 获取 %LOCALAPPDATA% (C:\Users\<user>\AppData\Local)
    let local_app_data = dirs::cache_dir().unwrap_or_default();

    // 1. 用户临时目录 %TEMP% （7 天以上才删除）
    let user_temp = std::env::temp_dir();
    clean_category(
        "用户临时文件",
        vec![user_temp],
        true,
        &mut stats,
        &app_handle,
    );

    // 2. 系统临时目录 C:\Windows\Temp （7 天以上才删除，权限不足会自动跳过）
    #[cfg(windows)]
    {
        let sys_temp = PathBuf::from(r"C:\Windows\Temp");
        clean_category(
            "系统临时文件",
            vec![sys_temp],
            true,
            &mut stats,
            &app_handle,
        );
    }

    // 3. 浏览器缓存（缓存无所谓新旧，全部清理）
    #[cfg(windows)]
    {
        let mut browser_paths: Vec<PathBuf> = Vec::new();

        // Edge 缓存
        let edge_base = local_app_data.join("Microsoft").join("Edge").join("User Data");
        browser_paths.push(edge_base.join("Default").join("Cache"));
        browser_paths.push(edge_base.join("Default").join("Code Cache"));
        browser_paths.push(edge_base.join("Default").join("GPUCache"));

        // Chrome 缓存
        let chrome_base = local_app_data.join("Google").join("Chrome").join("User Data");
        browser_paths.push(chrome_base.join("Default").join("Cache"));
        browser_paths.push(chrome_base.join("Default").join("Code Cache"));
        browser_paths.push(chrome_base.join("Default").join("GPUCache"));

        // Firefox 缓存（位于 Profiles 子目录下，每个 profile 一个文件夹）
        let firefox_profiles = local_app_data
            .join("Mozilla")
            .join("Firefox")
            .join("Profiles");
        if firefox_profiles.exists() {
            if let Ok(entries) = fs::read_dir(&firefox_profiles) {
                for entry in entries.flatten() {
                    browser_paths.push(entry.path().join("cache2"));
                }
            }
        }

        clean_category(
            "浏览器缓存",
            browser_paths,
            false,
            &mut stats,
            &app_handle,
        );
    }

    // 4. Windows 更新缓存 C:\Windows\SoftwareDistribution\Download
    //    （不停止服务直接删，文件占用会自动跳过）
    #[cfg(windows)]
    {
        let update_cache = PathBuf::from(r"C:\Windows\SoftwareDistribution\Download");
        clean_category(
            "Windows 更新缓存",
            vec![update_cache],
            false,
            &mut stats,
            &app_handle,
        );
    }

    // 5. 缩略图缓存
    #[cfg(windows)]
    {
        let thumb_cache_dir = local_app_data
            .join("Microsoft")
            .join("Windows")
            .join("Explorer");
        if thumb_cache_dir.exists() {
            // 仅删除 thumbcache_*.db 和 iconcache_*.db
            let mut thumb_files: Vec<PathBuf> = Vec::new();
            if let Ok(entries) = fs::read_dir(&thumb_cache_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_lowercase();
                    if (name.starts_with("thumbcache_") || name.starts_with("iconcache_"))
                        && name.ends_with(".db")
                    {
                        thumb_files.push(entry.path());
                    }
                }
            }
            // 直接逐个删除（不递归）
            stats.current_category = "缩略图缓存".to_string();
            println!("[清理电脑] 开始清理: 缩略图缓存");
            let cat_start_deleted = stats.deleted;
            let cat_start_skipped = stats.skipped;
            let cat_start_freed = stats.freed_bytes;
            for f in &thumb_files {
                safe_delete_file(f, &mut stats, false);
            }
            stats.categories.push(CategoryResult {
                name: "缩略图缓存".to_string(),
                deleted: stats.deleted - cat_start_deleted,
                skipped: stats.skipped - cat_start_skipped,
                freed_bytes: stats.freed_bytes - cat_start_freed,
            });
            let _ = app_handle.emit("clean-computer-progress", stats.clone());
        }
    }

    // 6. 系统日志文件 C:\Windows\Logs （7 天以上）
    #[cfg(windows)]
    {
        let win_logs = PathBuf::from(r"C:\Windows\Logs");
        clean_category(
            "系统日志",
            vec![win_logs],
            true,
            &mut stats,
            &app_handle,
        );
    }

    // 完成
    stats.is_running = false;
    stats.current_category = "完成".to_string();
    stats.current_path = String::new();

    let manager = app_handle.state::<std::sync::Arc<CleanComputerManager>>();
    manager.set_running(false);
    manager.set_stats(stats.clone());

    let _ = app_handle.emit("clean-computer-done", stats.clone());
    println!(
        "[清理电脑] 全部完成: 扫描={}, 删除={}, 跳过={}, 释放 {:.2} MB",
        stats.scanned,
        stats.deleted,
        stats.skipped,
        stats.freed_bytes as f64 / (1024.0 * 1024.0)
    );
}

/// Tauri 命令：启动清理电脑（后台线程执行，立即返回）
#[tauri::command]
pub fn clean_computer_cmd<R: Runtime>(
    window: tauri::Window<R>,
) -> Result<bool, String> {
    let app_handle = window.app_handle().clone();
    let manager = app_handle.state::<std::sync::Arc<CleanComputerManager>>();

    if manager.is_running() {
        return Err("清理任务正在进行中，请等待完成".to_string());
    }

    manager.set_running(true);

    // 后台线程执行清理，不阻塞 Tauri 命令调用，主窗口其他功能可继续使用
    std::thread::spawn(move || {
        run_clean_computer(app_handle);
    });

    Ok(true)
}

/// Tauri 命令：获取清理状态
#[tauri::command]
pub fn get_clean_computer_status<R: Runtime>(
    window: tauri::Window<R>,
) -> Result<CleanStats, String> {
    let manager = window.app_handle().state::<std::sync::Arc<CleanComputerManager>>();
    if manager.is_running() {
        // 正在运行时返回默认 stats（前端通过事件接收实时进度）
        let mut stats = manager.get_stats().unwrap_or_default();
        stats.is_running = true;
        Ok(stats)
    } else {
        // 不在运行时返回最近一次结果
        let mut stats = manager.get_stats().unwrap_or_default();
        stats.is_running = false;
        Ok(stats)
    }
}
