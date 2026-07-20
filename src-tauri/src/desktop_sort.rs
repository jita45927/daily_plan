use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

#[derive(Deserialize)]
pub enum ConflictStrategy {
    Overwrite,
    Rename,
    Skip,
}

// ==================== 桌面分析功能 ====================

/// 桌面项分类
/// - virtual: Windows 虚拟对象（如我的电脑、回收站、网络等，没有文件系统路径）
/// - programShortcut: 程序快捷方式（.lnk 指向 .exe）
/// - otherShortcut: 其他快捷方式（.lnk 指向非 exe，或 .url 文件）
/// - image: 图片文件
/// - regular: 普通文件和文件夹
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopItem {
    pub name: String,
    pub category: String,
    pub path: Option<String>,
    pub target: Option<String>,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopAnalysis {
    pub desktop_path: String,
    pub public_desktop_path: Option<String>,
    pub items: Vec<DesktopItem>,
    pub virtual_count: usize,
    pub program_shortcut_count: usize,
    pub other_shortcut_count: usize,
    pub image_count: usize,
    pub regular_count: usize,
    pub errors: Vec<String>,
}

/// 桌面分析窗口的状态管理器
pub struct DesktopAnalyzeManager {
    current_analysis: Mutex<Option<DesktopAnalysis>>,
}

impl DesktopAnalyzeManager {
    pub fn new() -> Self {
        Self {
            current_analysis: Mutex::new(None),
        }
    }

    pub fn set(&self, analysis: DesktopAnalysis) {
        *self.current_analysis.lock().unwrap() = Some(analysis);
    }

    pub fn get(&self) -> Option<DesktopAnalysis> {
        self.current_analysis.lock().unwrap().clone()
    }
}

impl Default for DesktopAnalyzeManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 获取公共桌面路径 C:\Users\Public\Desktop
fn get_public_desktop_path() -> Option<String> {
    #[cfg(windows)]
    {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;
        use windows_sys::Win32::UI::Shell::SHGetFolderPathW;
        use windows_sys::Win32::Foundation::MAX_PATH;

        const CSIDL_COMMON_DESKTOPDIRECTORY: i32 = 0x0019;
        let mut buffer: [u16; MAX_PATH as usize] = [0; MAX_PATH as usize];
        let result = unsafe {
            SHGetFolderPathW(0, CSIDL_COMMON_DESKTOPDIRECTORY, 0, 0, buffer.as_mut_ptr())
        };
        if result != 0 {
            return None;
        }
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(MAX_PATH as usize);
        let os_string = OsString::from_wide(&buffer[0..len]);
        Some(PathBuf::from(os_string).to_string_lossy().to_string())
    }
    #[cfg(not(windows))]
    None
}

/// 解析 .lnk 快捷方式的目标路径
fn read_lnk_target(lnk_path: &str) -> Option<String> {
    #[cfg(windows)]
    {
        use windows::core::{Interface, PCWSTR};
        use windows::Win32::Storage::FileSystem::WIN32_FIND_DATAW;
        use windows::Win32::System::Com::{CoCreateInstance, IPersistFile, STGM_READ, CLSCTX_INPROC_SERVER};
        use windows::Win32::UI::Shell::{IShellLinkW, ShellLink, SLGP_SHORTPATH};

        unsafe {
            let shell_link: IShellLinkW = match CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) {
                Ok(s) => s,
                Err(e) => {
                    println!("[桌面分析] CoCreateInstance 失败: {} ({})", lnk_path, e);
                    return None;
                }
            };
            let persist_file: IPersistFile = match shell_link.cast() {
                Ok(p) => p,
                Err(e) => {
                    println!("[桌面分析] cast IPersistFile 失败: {} ({})", lnk_path, e);
                    return None;
                }
            };

            let wide_path: Vec<u16> = lnk_path.encode_utf16().chain(std::iter::once(0)).collect();
            if let Err(e) = persist_file.Load(PCWSTR(wide_path.as_ptr()), STGM_READ) {
                println!("[桌面分析] Load 失败: {} ({})", lnk_path, e);
                return None;
            }

            let mut target_buf = [0u16; 260];
            let mut find_data: WIN32_FIND_DATAW = std::mem::zeroed();
            if let Err(e) = shell_link.GetPath(&mut target_buf, &mut find_data, SLGP_SHORTPATH.0 as u32) {
                println!("[桌面分析] GetPath 失败: {} ({})", lnk_path, e);
                return None;
            }

            let len = target_buf.iter().position(|&c| c == 0).unwrap_or(0);
            if len == 0 {
                return None;
            }
            Some(String::from_utf16_lossy(&target_buf[..len]))
        }
    }
    #[cfg(not(windows))]
    {
        let _ = lnk_path;
        None
    }
}

/// 检查桌面系统图标是否显示
/// 通过读取注册表 HideDesktopIcons 判断
/// - 值为 0x1 表示隐藏
/// - 值为 0x0 表示显示（用户明确启用）
/// - 值不存在：使用默认行为（回收站默认显示，其他默认不显示）
fn is_desktop_icon_visible(clsid: &str) -> bool {
    #[cfg(windows)]
    {
        let paths = [
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\HideDesktopIcons\NewStartPanel",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\HideDesktopIcons\ClassicStartPanel",
        ];

        for path in &paths {
            let output = std::process::Command::new("reg")
                .args(["query", path, "/v", clsid])
                .output();

            if let Ok(output) = output {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    // 值为 0x1 表示隐藏
                    if stdout.contains("0x1") {
                        return false;
                    }
                    // 值为 0x0 表示显示
                    if stdout.contains("0x0") {
                        return true;
                    }
                }
            }
        }

        // 值不存在，使用默认行为
        // 回收站默认显示，其他默认不显示
        clsid.eq_ignore_ascii_case("{645FF040-5081-101B-9F08-00AA002F954E}")
    }
    #[cfg(not(windows))]
    {
        let _ = clsid;
        false
    }
}

/// 获取桌面上显示的系统虚拟对象（此电脑、回收站等）
/// 通过注册表检测哪些系统图标在桌面上显示
fn get_desktop_virtual_icons() -> Vec<DesktopItem> {
    // 常见的桌面系统图标 CLSID 和显示名称
    const ICONS: &[(&str, &str)] = &[
        ("{20D04FE0-3AEA-1069-A2D8-08002B30309D}", "此电脑"),
        ("{645FF040-5081-101B-9F08-00AA002F954E}", "回收站"),
        ("{59031a47-3f72-44a7-89c5-5595fe6b30ee}", "用户的文件"),
        ("{F02C1A0D-BE21-4350-88B0-7367FC96EF3C}", "网络"),
        ("{5399E694-6CE5-4D6C-8FCE-1D8870FDCBA0}", "控制面板"),
    ];

    let mut items = Vec::new();

    for (clsid, name) in ICONS {
        if is_desktop_icon_visible(clsid) {
            println!("[桌面分析] 虚拟对象: {} ({})", name, clsid);
            items.push(DesktopItem {
                name: name.to_string(),
                category: "virtual".to_string(),
                path: None,
                target: None,
                is_dir: true,
            });
        }
    }

    items
}

/// 分析桌面文件，分类为虚拟对象、程序快捷方式、其他快捷方式、普通文件
pub fn analyze_desktop() -> Result<DesktopAnalysis, String> {
    let desktop_path = get_desktop_path()?;
    let public_desktop_path = get_public_desktop_path();

    println!("[桌面分析] 用户桌面: {}", desktop_path);
    if let Some(ref p) = public_desktop_path {
        println!("[桌面分析] 公共桌面: {}", p);
    }

    // 在单独的线程上执行分析，避免与 Tauri 主线程的 COM 模型冲突
    // read_lnk_target 需要 STA 模式的 COM，而 Tauri 主线程可能已经是 MTA 模式
    let handle = std::thread::spawn(move || {
        #[cfg(windows)]
        {
            use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
            let co_hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            // S_OK = 初始化成功，需要 CoUninitialize
            // S_FALSE = 已经初始化过，不需要 CoUninitialize
            // RPC_E_CHANGED_MODE = 线程模型冲突（不应该发生，因为是新线程）
            let need_uninit = co_hr == windows::Win32::Foundation::S_OK;
            println!("[桌面分析] COM 初始化 hr=0x{:x}, need_uninit={}", co_hr.0, need_uninit);

            let result = analyze_desktop_inner(&desktop_path, &public_desktop_path);

            if need_uninit {
                unsafe { CoUninitialize() };
            }
            result
        }
        #[cfg(not(windows))]
        {
            analyze_desktop_inner(&desktop_path, &public_desktop_path)
        }
    });

    handle.join().map_err(|e| format!("分析线程崩溃: {:?}", e))?
}

fn analyze_desktop_inner(
    desktop_path: &str,
    public_desktop_path: &Option<String>,
) -> Result<DesktopAnalysis, String> {
    let mut items = Vec::new();
    let mut errors = Vec::new();

    // 获取桌面上显示的系统虚拟对象（此电脑、回收站等）
    let virtual_items = get_desktop_virtual_icons();
    items.extend(virtual_items);

    // 枚举用户桌面文件夹
    enumerate_desktop_folder(desktop_path, &mut items, &mut errors);

    // 枚举公共桌面文件夹（C:\Users\Public\Desktop）
    if let Some(ref pub_path) = public_desktop_path {
        enumerate_desktop_folder(pub_path, &mut items, &mut errors);
    }

    println!("[桌面分析] 枚举完成，共 {} 项", items.len());

    let virtual_count = items.iter().filter(|i| i.category == "virtual").count();
    let program_shortcut_count = items.iter().filter(|i| i.category == "programShortcut").count();
    let other_shortcut_count = items.iter().filter(|i| i.category == "otherShortcut").count();
    let image_count = items.iter().filter(|i| i.category == "image").count();
    let regular_count = items.iter().filter(|i| i.category == "regular").count();

    Ok(DesktopAnalysis {
        desktop_path: desktop_path.to_string(),
        public_desktop_path: public_desktop_path.clone(),
        items,
        virtual_count,
        program_shortcut_count,
        other_shortcut_count,
        image_count,
        regular_count,
        errors,
    })
}

/// 枚举桌面文件夹中的项目并进行分类
/// 只枚举文件系统中实际存在的文件，不包含 Shell 虚拟命名空间对象
fn enumerate_desktop_folder(
    folder_path: &str,
    items: &mut Vec<DesktopItem>,
    errors: &mut Vec<String>,
) {
    let entries = match fs::read_dir(folder_path) {
        Ok(e) => e,
        Err(e) => {
            errors.push(format!("读取目录 {} 失败: {}", folder_path, e));
            return;
        }
    };

    for entry in entries {
        match entry {
            Ok(entry) => {
                let path = entry.path();
                let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

                // 跳过 desktop.ini（系统文件，控制文件夹外观）
                if name.eq_ignore_ascii_case("desktop.ini") {
                    continue;
                }

                let is_dir = path.is_dir();
                let ext = path.extension().map(|e| e.to_ascii_lowercase());
                let path_str = path.to_string_lossy().to_string();

                match ext.as_ref().and_then(|e| e.to_str()) {
                    Some("lnk") => {
                        let target = read_lnk_target(&path_str);
                        let is_program = target
                            .as_ref()
                            .map(|t| {
                                let lower = t.to_lowercase();
                                lower.ends_with(".exe") || lower.ends_with(".bat") || lower.ends_with(".cmd")
                            })
                            .unwrap_or(false);
                        let category = if is_program { "programShortcut" } else { "otherShortcut" };
                        println!("[桌面分析] {}: {} -> {:?}", category, name, target.as_deref().unwrap_or("(无法解析)"));
                        items.push(DesktopItem {
                            name,
                            category: category.to_string(),
                            path: Some(path_str),
                            target,
                            is_dir: false,
                        });
                    }
                    Some("url") => {
                        println!("[桌面分析] otherShortcut (url): {}", name);
                        items.push(DesktopItem {
                            name,
                            category: "otherShortcut".to_string(),
                            path: Some(path_str),
                            target: None,
                            is_dir: false,
                        });
                    }
                    _ => {
                        // 检测是否为图片文件
                        let is_image = ext.as_ref().and_then(|e| e.to_str()).map(|e| {
                            matches!(
                                e,
                                "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp"
                                    | "tiff" | "tif" | "svg" | "ico" | "heic" | "heif"
                            )
                        }).unwrap_or(false);

                        if is_image && !is_dir {
                            println!("[桌面分析] image: {}", name);
                            items.push(DesktopItem {
                                name,
                                category: "image".to_string(),
                                path: Some(path_str),
                                target: None,
                                is_dir: false,
                            });
                        } else {
                            println!("[桌面分析] regular: {}", name);
                            items.push(DesktopItem {
                                name,
                                category: "regular".to_string(),
                                path: Some(path_str),
                                target: None,
                                is_dir,
                            });
                        }
                    }
                }
            }
            Err(e) => errors.push(format!("读取目录项失败: {}", e)),
        }
    }
}

pub fn get_desktop_path() -> Result<String, String> {
    let path: PathBuf;
    
    #[cfg(windows)]
    {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;
        use windows_sys::Win32::UI::Shell::SHGetFolderPathW;
        use windows_sys::Win32::Foundation::MAX_PATH;
        
        const CSIDL_DESKTOP: i32 = 0x0000;
        
        let mut buffer: [u16; MAX_PATH as usize] = [0; MAX_PATH as usize];
        
        let result = unsafe {
            SHGetFolderPathW(
                0,
                CSIDL_DESKTOP,
                0,
                0,
                buffer.as_mut_ptr()
            )
        };
        
        if result != 0 {
            return Err("获取桌面路径失败".to_string());
        }
        
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(MAX_PATH as usize);
        let wide_str = &buffer[0..len];
        let os_string = OsString::from_wide(wide_str);
        
        path = PathBuf::from(os_string);
    }
    
    #[cfg(not(windows))]
    {
        path = dirs::desktop_dir().ok_or("无法获取桌面路径")?;
    }
    
    path.to_str().map(|s| s.to_string()).ok_or("桌面路径转换失败".to_string())
}

fn create_target_folders(desktop_path: &Path) -> Result<(PathBuf, PathBuf, PathBuf, PathBuf), String> {
    let program_shortcuts_folder = desktop_path.join("程序快捷方式");
    let other_shortcuts_folder = desktop_path.join("其他快捷方式");
    let images_folder = desktop_path.join("桌面图片文件");
    let other_files_folder = desktop_path.join("桌面整理文件");

    if !program_shortcuts_folder.exists() {
        fs::create_dir(&program_shortcuts_folder).map_err(|e| format!("创建程序快捷方式文件夹失败: {}", e))?;
    }

    if !other_shortcuts_folder.exists() {
        fs::create_dir(&other_shortcuts_folder).map_err(|e| format!("创建其他快捷方式文件夹失败: {}", e))?;
    }

    if !images_folder.exists() {
        fs::create_dir(&images_folder).map_err(|e| format!("创建桌面图片文件文件夹失败: {}", e))?;
    }

    if !other_files_folder.exists() {
        fs::create_dir(&other_files_folder).map_err(|e| format!("创建桌面整理文件文件夹失败: {}", e))?;
    }

    Ok((program_shortcuts_folder, other_shortcuts_folder, images_folder, other_files_folder))
}

fn generate_unique_path(target_folder: &Path, file_name: &str) -> PathBuf {
    let target_path = target_folder.join(file_name);
    if !target_path.exists() {
        return target_path;
    }
    
    let path = Path::new(file_name);
    let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
    let extension = path.extension().map(|e| e.to_string_lossy().to_string());
    
    let mut counter = 1;
    loop {
        let new_name = match &extension {
            Some(ext) => format!("{}({}).{}", stem, counter, ext),
            None => format!("{}({})", stem, counter),
        };
        let new_path = target_folder.join(&new_name);
        if !new_path.exists() {
            return new_path;
        }
        counter += 1;
    }
}

fn move_file(source: &Path, target: &Path, strategy: &ConflictStrategy) -> Result<(), String> {
    if target.exists() {
        match strategy {
            ConflictStrategy::Overwrite => {
                fs::remove_file(target).map_err(|e| format!("删除目标文件失败: {}", e))?;
            }
            ConflictStrategy::Rename => {
                let unique_target = generate_unique_path(target.parent().unwrap_or(target), target.file_name().unwrap().to_str().unwrap());
                return fs::rename(source, &unique_target).map_err(|e| format!("移动文件失败: {}", e));
            }
            ConflictStrategy::Skip => {
                return Ok(());
            }
        }
    }
    
    fs::rename(source, target).map_err(|e| format!("移动文件失败: {}", e))
}

fn move_dir(source: &Path, target: &Path, strategy: &ConflictStrategy) -> Result<(), String> {
    if target.exists() {
        match strategy {
            ConflictStrategy::Overwrite => {
                fs::remove_dir_all(target).map_err(|e| format!("删除目标文件夹失败: {}", e))?;
            }
            ConflictStrategy::Rename => {
                let unique_target = generate_unique_path(target.parent().unwrap_or(target), target.file_name().unwrap().to_str().unwrap());
                return fs::rename(source, &unique_target).map_err(|e| format!("移动文件夹失败: {}", e));
            }
            ConflictStrategy::Skip => {
                return Ok(());
            }
        }
    }
    
    fs::rename(source, target).map_err(|e| format!("移动文件夹失败: {}", e))
}

pub fn organize_desktop(strategy: ConflictStrategy) -> Result<(usize, usize, usize, usize, Vec<String>), String> {
    let desktop_path_str = get_desktop_path()?;
    let desktop_path = Path::new(&desktop_path_str);

    if !desktop_path.exists() {
        return Err("桌面路径不存在".to_string());
    }

    let (program_shortcuts_folder, other_shortcuts_folder, images_folder, other_files_folder) =
        create_target_folders(desktop_path)?;

    let entries = fs::read_dir(desktop_path).map_err(|e| format!("读取桌面目录失败: {}", e))?;

    let mut program_shortcut_count = 0;
    let mut other_shortcut_count = 0;
    let mut image_count = 0;
    let mut other_count = 0;
    let mut errors = Vec::new();

    // 跳过的目标文件夹名称
    let skip_folders: &[&str] = &[
        "程序快捷方式",
        "其他快捷方式",
        "桌面图片文件",
        "桌面整理文件",
    ];

    // 图片扩展名列表
    let image_exts: &[&str] = &[
        "jpg", "jpeg", "png", "gif", "bmp", "webp", "tiff", "tif", "svg", "ico", "heic", "heif",
    ];

    // 程序扩展名列表（快捷方式指向这些即为程序快捷方式）
    let program_exts: &[&str] = &["exe", "bat", "cmd"];

    for entry in entries {
        match entry {
            Ok(entry) => {
                let path = entry.path();
                let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

                // 跳过目标文件夹本身
                if skip_folders.contains(&file_name.as_str()) {
                    continue;
                }

                // 跳过 desktop.ini 系统文件
                if file_name.eq_ignore_ascii_case("desktop.ini") {
                    continue;
                }

                if path.is_file() {
                    let ext = path.extension().map(|e| {
                        e.to_ascii_lowercase()
                            .to_string_lossy()
                            .to_string()
                            .to_lowercase()
                    });

                    if let Some(ref ext_str) = ext {
                        // .lnk 快捷方式：解析目标区分为程序快捷方式和其他快捷方式
                        if ext_str == "lnk" {
                            let target = read_lnk_target(&path.to_string_lossy());
                            let is_program = target
                                .as_ref()
                                .map(|t| {
                                    let lower = t.to_lowercase();
                                    program_exts.iter().any(|e| lower.ends_with(&format!(".{}", e)))
                                })
                                .unwrap_or(false);

                            let target_folder = if is_program {
                                &program_shortcuts_folder
                            } else {
                                &other_shortcuts_folder
                            };
                            let target_path = target_folder.join(&file_name);
                            if let Err(e) = move_file(&path, &target_path, &strategy) {
                                errors.push(format!("{}: {}", file_name, e));
                            } else if is_program {
                                program_shortcut_count += 1;
                            } else {
                                other_shortcut_count += 1;
                            }
                            continue;
                        }

                        // .url 快捷方式：其他快捷方式
                        if ext_str == "url" {
                            let target_path = other_shortcuts_folder.join(&file_name);
                            if let Err(e) = move_file(&path, &target_path, &strategy) {
                                errors.push(format!("{}: {}", file_name, e));
                            } else {
                                other_shortcut_count += 1;
                            }
                            continue;
                        }

                        // 图片文件
                        if image_exts.contains(&ext_str.as_str()) {
                            let target_path = images_folder.join(&file_name);
                            if let Err(e) = move_file(&path, &target_path, &strategy) {
                                errors.push(format!("{}: {}", file_name, e));
                            } else {
                                image_count += 1;
                            }
                            continue;
                        }
                    }

                    // 其他文件 → 桌面整理文件
                    let target_path = other_files_folder.join(&file_name);
                    if let Err(e) = move_file(&path, &target_path, &strategy) {
                        errors.push(format!("{}: {}", file_name, e));
                    } else {
                        other_count += 1;
                    }
                } else if path.is_dir() {
                    // 文件夹 → 桌面整理文件
                    let target_path = other_files_folder.join(&file_name);
                    if let Err(e) = move_dir(&path, &target_path, &strategy) {
                        errors.push(format!("{}: {}", file_name, e));
                    } else {
                        other_count += 1;
                    }
                }
            }
            Err(e) => {
                errors.push(format!("读取目录项失败: {}", e));
            }
        }
    }

    Ok((
        program_shortcut_count,
        other_shortcut_count,
        image_count,
        other_count,
        errors,
    ))
}

// ==================== 桌面分析窗口 ====================

const ANALYZE_WIN_WIDTH: f64 = 720.0;
const ANALYZE_WIN_HEIGHT: f64 = 560.0;

/// 预创建桌面分析窗口（保留函数签名，实际不做预创建，改为动态创建）
/// 保留此函数是为了不修改 lib.rs 中的调用点
pub fn setup_desktop_analyze_window<R: Runtime>(_app: &tauri::AppHandle<R>) -> Result<(), String> {
    // 不再预创建窗口，改为每次调用 analyze_desktop_cmd 时动态创建
    // 预创建会导致 Vue 组件在应用启动时就 mounted，等用户打开窗口时数据已经过期
    // 动态创建确保组件挂载时分析结果已经准备好
    Ok(())
}

/// 执行桌面分析并存储结果（不创建窗口）
#[tauri::command]
pub fn analyze_desktop_cmd<R: Runtime>(window: tauri::Window<R>) -> Result<bool, String> {
    let app = window.app_handle();
    println!("[桌面分析] 开始分析桌面文件...");

    let analysis = analyze_desktop()?;

    println!("[桌面分析] 分析完成，共 {} 项 (虚拟:{}, 程序快捷方式:{}, 其他快捷方式:{}, 图片:{}, 普通:{})",
        analysis.items.len(),
        analysis.virtual_count,
        analysis.program_shortcut_count,
        analysis.other_shortcut_count,
        analysis.image_count,
        analysis.regular_count
    );

    let manager = app.state::<Arc<DesktopAnalyzeManager>>();
    manager.set(analysis);

    Ok(true)
}

/// 显示桌面分析结果窗口（分析已完成，结果已存储在 manager 中）
#[tauri::command]
pub fn show_analyze_window<R: Runtime>(window: tauri::Window<R>) -> Result<bool, String> {
    let app = window.app_handle();

    // 如果窗口已存在，先销毁（destroy 是同步的，避免 close 的异步问题）
    if let Some(existing_win) = app.get_webview_window("desktop_analyze") {
        let _ = existing_win.destroy();
    }

    // 计算窗口在主屏幕中央的位置
    let (x, y) = if let Ok(Some(monitor)) = window.primary_monitor() {
        let work_area = monitor.work_area();
        let px = (work_area.size.width as f64 - ANALYZE_WIN_WIDTH) / 2.0 + work_area.position.x as f64;
        let py = (work_area.size.height as f64 - ANALYZE_WIN_HEIGHT) / 2.0 + work_area.position.y as f64;
        (px, py)
    } else {
        (100.0, 100.0)
    };

    // 动态创建窗口（组件挂载时分析结果已经存储在 manager 中，直接拉取即可）
    // 使用 app 而不是 &window，与其他窗口保持一致
    let analyze_win = WebviewWindowBuilder::new(
        app,
        "desktop_analyze",
        WebviewUrl::App("/desktop-analyze.html".into()),
    )
    .title("桌面文件分析")
    .inner_size(ANALYZE_WIN_WIDTH, ANALYZE_WIN_HEIGHT)
    .decorations(true)
    .transparent(false)
    .always_on_top(true)
    .skip_taskbar(false)
    .resizable(true)
    .position(x, y)
    .on_navigation(|url| {
        println!("[桌面分析] 导航到: {}", url);
        true
    })
    .build()
    .map_err(|e| format!("创建桌面分析窗口失败: {:?}", e))?;

    // 自动打开 DevTools 以便调试
    let _ = analyze_win.open_devtools();

    println!("[桌面分析] 窗口已创建，DevTools 已打开");

    Ok(true)
}

/// 获取当前桌面分析结果
#[tauri::command]
pub fn get_desktop_analysis<R: Runtime>(window: tauri::Window<R>) -> Option<DesktopAnalysis> {
    let manager = window.app_handle().state::<Arc<DesktopAnalyzeManager>>();
    let result = manager.get();
    if let Some(ref r) = result {
        println!("[桌面分析] get_desktop_analysis 返回数据: {} 项", r.items.len());
    } else {
        println!("[桌面分析] get_desktop_analysis 返回 None");
    }
    result
}

/// 关闭桌面分析窗口（完全销毁，避免下次创建时标签冲突）
#[tauri::command]
pub fn close_desktop_analyze<R: Runtime>(window: tauri::Window<R>) {
    if let Some(win) = window.app_handle().get_webview_window("desktop_analyze") {
        let _ = win.destroy();
    }
}