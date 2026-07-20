use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};
use crate::window::WindowManager;

#[derive(Deserialize, Serialize)]
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

/// 跨盘符安全的文件移动：先尝试 rename（同盘符快），失败则 copy+remove（跨盘符）
fn move_file_safe(source: &Path, target: &Path) -> Result<(), String> {
    match fs::rename(source, target) {
        Ok(()) => Ok(()),
        Err(_) => {
            // rename 失败（通常是跨盘符），改用 copy + remove
            fs::copy(source, target)
                .map_err(|e| format!("复制文件失败(跨盘符): {}", e))?;
            fs::remove_file(source)
                .map_err(|e| format!("删除源文件失败(跨盘符): {}", e))?;
            Ok(())
        }
    }
}

/// 跨盘符安全的文件夹移动：先尝试 rename（同盘符快），失败则递归 copy+remove（跨盘符）
fn move_dir_safe(source: &Path, target: &Path) -> Result<(), String> {
    match fs::rename(source, target) {
        Ok(()) => Ok(()),
        Err(_) => {
            // rename 失败（通常是跨盘符），改用递归 copy + remove
            copy_dir_recursive(source, target)
                .map_err(|e| format!("复制文件夹失败(跨盘符): {}", e))?;
            fs::remove_dir_all(source)
                .map_err(|e| format!("删除源文件夹失败(跨盘符): {}", e))?;
            Ok(())
        }
    }
}

/// 递归复制文件夹
fn copy_dir_recursive(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target)
        .map_err(|e| format!("创建目标文件夹失败: {}", e))?;
    for entry in fs::read_dir(source).map_err(|e| format!("读取源文件夹失败: {}", e))? {
        let entry = entry.map_err(|e| format!("读取条目失败: {}", e))?;
        let path = entry.path();
        let file_name = entry.file_name();
        let dest = target.join(&file_name);
        if path.is_dir() {
            copy_dir_recursive(&path, &dest)?;
        } else {
            fs::copy(&path, &dest)
                .map_err(|e| format!("复制文件失败: {}", e))?;
        }
    }
    Ok(())
}

fn move_file(source: &Path, target: &Path, strategy: &ConflictStrategy) -> Result<(), String> {
    if target.exists() {
        match strategy {
            ConflictStrategy::Overwrite => {
                fs::remove_file(target).map_err(|e| format!("删除目标文件失败: {}", e))?;
            }
            ConflictStrategy::Rename => {
                let unique_target = generate_unique_path(target.parent().unwrap_or(target), target.file_name().unwrap().to_str().unwrap());
                return move_file_safe(source, &unique_target);
            }
            ConflictStrategy::Skip => {
                return Ok(());
            }
        }
    }

    move_file_safe(source, target)
}

fn move_dir(source: &Path, target: &Path, strategy: &ConflictStrategy) -> Result<(), String> {
    if target.exists() {
        match strategy {
            ConflictStrategy::Overwrite => {
                fs::remove_dir_all(target).map_err(|e| format!("删除目标文件夹失败: {}", e))?;
            }
            ConflictStrategy::Rename => {
                let unique_target = generate_unique_path(target.parent().unwrap_or(target), target.file_name().unwrap().to_str().unwrap());
                return move_dir_safe(source, &unique_target);
            }
            ConflictStrategy::Skip => {
                return Ok(());
            }
        }
    }

    move_dir_safe(source, target)
}

/// 冲突文件信息
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictFile {
    /// 源文件名
    pub file_name: String,
    /// 源文件路径
    pub source_path: String,
    /// 目标文件夹名
    pub target_folder: String,
    /// 目标文件路径
    pub target_path: String,
}

/// 整理前检查冲突：返回所有会产生同名冲突的文件列表
pub fn check_conflicts_before_organize() -> Result<Vec<ConflictFile>, String> {
    let desktop_path_str = get_desktop_path()?;
    let desktop_path = Path::new(&desktop_path_str);

    if !desktop_path.exists() {
        return Err("桌面路径不存在".to_string());
    }

    let (program_shortcuts_folder, other_shortcuts_folder, images_folder, other_files_folder) =
        create_target_folders(desktop_path)?;

    let skip_folders: &[&str] = &[
        "程序快捷方式",
        "其他快捷方式",
        "桌面图片文件",
        "桌面整理文件",
    ];

    let image_exts: &[&str] = &[
        "jpg", "jpeg", "png", "gif", "bmp", "webp", "tiff", "tif", "svg", "ico", "heic", "heif",
    ];

    let program_exts: &[&str] = &["exe", "bat", "cmd"];

    let mut conflicts = Vec::new();

    // 收集需要检查的目录：用户桌面 + 公共桌面
    let mut desktop_dirs = vec![desktop_path.to_path_buf()];
    if let Some(pub_path) = get_public_desktop_path() {
        let pub_path = PathBuf::from(&pub_path);
        if pub_path.exists() {
            desktop_dirs.push(pub_path);
        }
    }

    for dir in desktop_dirs {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

            if skip_folders.contains(&file_name.as_str()) {
                continue;
            }
            if file_name.eq_ignore_ascii_case("desktop.ini") {
                continue;
            }

            // 确定目标文件夹
            let target_folder = if path.is_file() {
                let ext = path.extension().map(|e| {
                    e.to_ascii_lowercase().to_string_lossy().to_string().to_lowercase()
                });
                if let Some(ref ext_str) = ext {
                    if ext_str == "lnk" {
                        let target = read_lnk_target(&path.to_string_lossy());
                        let is_program = target
                            .as_ref()
                            .map(|t| {
                                let lower = t.to_lowercase();
                                program_exts.iter().any(|e| lower.ends_with(&format!(".{}", e)))
                            })
                            .unwrap_or(false);
                        if is_program {
                            &program_shortcuts_folder
                        } else {
                            &other_shortcuts_folder
                        }
                    } else if ext_str == "url" {
                        &other_shortcuts_folder
                    } else if image_exts.contains(&ext_str.as_str()) {
                        &images_folder
                    } else {
                        &other_files_folder
                    }
                } else {
                    &other_files_folder
                }
            } else {
                &other_files_folder
            };

            let target_path = target_folder.join(&file_name);
            // 检查是否冲突（目标文件已存在，且不是同一个文件）
            if target_path.exists() {
                let target_folder_name = target_folder.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                conflicts.push(ConflictFile {
                    file_name: file_name.clone(),
                    source_path: path.to_string_lossy().to_string(),
                    target_folder: target_folder_name,
                    target_path: target_path.to_string_lossy().to_string(),
                });
            }
        }
    }

    Ok(conflicts)
}

pub fn organize_desktop(strategy: ConflictStrategy) -> Result<(usize, usize, usize, usize, Vec<String>), String> {
    let desktop_path_str = get_desktop_path()?;
    let desktop_path = Path::new(&desktop_path_str);

    if !desktop_path.exists() {
        return Err("桌面路径不存在".to_string());
    }

    let (program_shortcuts_folder, other_shortcuts_folder, images_folder, other_files_folder) =
        create_target_folders(desktop_path)?;

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

    // 收集需要整理的目录：用户桌面 + 公共桌面
    let mut desktop_dirs = vec![desktop_path.to_path_buf()];
    if let Some(pub_path) = get_public_desktop_path() {
        let pub_path = PathBuf::from(&pub_path);
        if pub_path.exists() {
            desktop_dirs.push(pub_path);
        }
    }

    for dir in desktop_dirs {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) => {
                errors.push(format!("读取目录失败 {:?}: {}", dir, e));
                continue;
            }
        };

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

/// 预创建桌面分析窗口（参考右键菜单的预创建模式）
pub fn setup_desktop_analyze_window<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), String> {
    println!("[桌面分析] 预创建桌面分析窗口...");
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
    .visible(false)
    .position(-3000.0, -3000.0)
    .build()
    .map_err(|e| format!("创建桌面分析窗口失败: {:?}", e))?;

    let win_clone = analyze_win.clone();
    analyze_win.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = win_clone.hide();
            let _ = win_clone.set_position(tauri::PhysicalPosition::new(-3000, -3000));
        }
    });

    println!("[桌面分析] 桌面分析窗口预创建完成");
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

    // 获取预创建的分析窗口
    let analyze_win = app.get_webview_window("desktop_analyze")
        .ok_or_else(|| "分析窗口未初始化".to_string())?;

    // 计算窗口在主屏幕中央的位置
    let (x, y) = if let Ok(Some(monitor)) = window.primary_monitor() {
        let work_area = monitor.work_area();
        let px = (work_area.size.width as f64 - ANALYZE_WIN_WIDTH) / 2.0 + work_area.position.x as f64;
        let py = (work_area.size.height as f64 - ANALYZE_WIN_HEIGHT) / 2.0 + work_area.position.y as f64;
        (px, py)
    } else {
        (100.0, 100.0)
    };

    // 移动窗口到正确位置并显示
    let _ = analyze_win.set_position(tauri::LogicalPosition::new(x, y));
    let _ = analyze_win.show();
    let _ = analyze_win.set_focus();

    // 确保主窗口保持展开状态
    let win_manager = app.state::<Arc<WindowManager>>();
    win_manager.expand_window(&app);

    // 发送事件通知前端刷新数据
    let manager = app.state::<Arc<DesktopAnalyzeManager>>();
    if let Some(analysis) = manager.get() {
        let _ = analyze_win.emit("desktop-analyze-reload", analysis);
    }

    Ok(true)
}

/// 获取当前桌面分析结果
#[tauri::command]
pub fn get_desktop_analysis<R: Runtime>(window: tauri::Window<R>) -> Option<DesktopAnalysis> {
    let manager = window.app_handle().state::<Arc<DesktopAnalyzeManager>>();
    manager.get()
}

/// 关闭桌面分析窗口（隐藏并移到屏幕外，不销毁）
#[tauri::command]
pub fn close_desktop_analyze<R: Runtime>(window: tauri::Window<R>) {
    if let Some(win) = window.app_handle().get_webview_window("desktop_analyze") {
        let _ = win.hide();
        let _ = win.set_position(tauri::PhysicalPosition::new(-3000, -3000));
    }
}

// ==================== 重复文件清理功能 ====================

/// 重复文件组信息
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateFileGroup {
    /// 文件哈希值
    pub hash: String,
    /// 文件大小（字节）
    pub size: u64,
    /// 重复文件列表（按名称排序）
    pub files: Vec<DuplicateFile>,
}

/// 重复文件信息
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateFile {
    /// 文件名
    pub name: String,
    /// 文件路径
    pub path: String,
    /// 所在文件夹
    pub folder: String,
}

/// 计算文件的 SHA256 哈希值
fn compute_file_hash(path: &Path) -> Result<String, String> {
    use sha2::{Sha256, Digest};
    use std::io::Read;

    let mut file = fs::File::open(path)
        .map_err(|e| format!("打开文件失败 {}: {}", path.display(), e))?;

    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = file.read(&mut buffer)
            .map_err(|e| format!("读取文件失败 {}: {}", path.display(), e))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

/// 将文件移到回收站（Windows）
fn move_to_recycle_bin(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Shell::{SHFileOperationW, SHFILEOPSTRUCTW};
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStrExt;

        const FO_DELETE: u32 = 0x0003;
        const FOF_ALLOWUNDO: u16 = 0x0040;
        const FOF_NOCONFIRMATION: u16 = 0x0010;
        const FOF_SILENT: u16 = 0x0004;
        const FOF_NOERRORUI: u16 = 0x0400;

        let path_str = path.to_string_lossy().to_string();
        let mut wide_path: Vec<u16> = OsString::from(&path_str)
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .chain(std::iter::once(0))
            .collect();

        let mut file_op: SHFILEOPSTRUCTW = unsafe { std::mem::zeroed() };
        file_op.wFunc = FO_DELETE;
        file_op.pFrom = wide_path.as_mut_ptr();
        file_op.fFlags = FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI;

        let result = unsafe { SHFileOperationW(&mut file_op) };

        if result != 0 {
            return Err(format!("移入回收站失败 (错误码: {})", result));
        }

        if file_op.fAnyOperationsAborted != 0 {
            return Err("操作被中断".to_string());
        }

        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err("仅支持 Windows 平台".to_string())
    }
}

/// 扫描四个文件夹中的重复文件
pub fn find_duplicate_files() -> Result<Vec<DuplicateFileGroup>, String> {
    let desktop_path_str = get_desktop_path()?;
    let desktop_path = Path::new(&desktop_path_str);

    let folders = [
        "程序快捷方式",
        "其他快捷方式",
        "桌面整理文件",
        "桌面图片文件",
    ];

    let mut files_by_hash: std::collections::HashMap<String, Vec<DuplicateFile>> = std::collections::HashMap::new();
    let mut size_map: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    let mut errors = Vec::new();

    for folder_name in &folders {
        let folder_path = desktop_path.join(folder_name);
        if !folder_path.exists() {
            continue;
        }

        let entries = match fs::read_dir(&folder_path) {
            Ok(e) => e,
            Err(e) => {
                errors.push(format!("读取文件夹 {} 失败: {}", folder_name, e));
                continue;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let file_name = path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            let metadata = match fs::metadata(&path) {
                Ok(m) => m,
                Err(e) => {
                    errors.push(format!("获取文件元数据失败 {}: {}", file_name, e));
                    continue;
                }
            };

            let file_size = metadata.len();
            if file_size == 0 {
                continue;
            }

            let hash = match compute_file_hash(&path) {
                Ok(h) => h,
                Err(e) => {
                    errors.push(format!("计算文件哈希失败 {}: {}", file_name, e));
                    continue;
                }
            };

            let dup_file = DuplicateFile {
                name: file_name,
                path: path.to_string_lossy().to_string(),
                folder: folder_name.to_string(),
            };

            size_map.insert(hash.clone(), file_size);
            files_by_hash.entry(hash).or_default().push(dup_file);
        }
    }

    let mut groups: Vec<DuplicateFileGroup> = files_by_hash
        .into_iter()
        .filter(|(_, files)| files.len() > 1)
        .map(|(hash, mut files)| {
            files.sort_by(|a, b| a.name.cmp(&b.name));
            let size = size_map.get(&hash).copied().unwrap_or(0);
            DuplicateFileGroup { hash, size, files }
        })
        .collect();

    groups.sort_by(|a, b| b.size.cmp(&a.size));

    println!("[清理重复文件] 发现 {} 组重复文件", groups.len());
    for (i, group) in groups.iter().enumerate() {
        println!("  组 {}: {} 字节, {} 个文件", i + 1, group.size, group.files.len());
        for f in &group.files {
            println!("    - {} ({})", f.name, f.folder);
        }
    }

    Ok(groups)
}

/// 清理重复文件：保留每组按名称排序的第一个，其余移到回收站
pub fn clean_duplicate_files() -> Result<(usize, usize, Vec<String>), String> {
    let groups = find_duplicate_files()?;

    let mut total_groups = 0;
    let mut moved_count = 0;
    let mut errors = Vec::new();

    for group in &groups {
        if group.files.len() <= 1 {
            continue;
        }

        total_groups += 1;

        for (i, file) in group.files.iter().enumerate() {
            if i == 0 {
                continue;
            }

            let path = Path::new(&file.path);
            match move_to_recycle_bin(path) {
                Ok(()) => {
                    moved_count += 1;
                    println!("[清理重复文件] 已移入回收站: {} ({})", file.name, file.folder);
                }
                Err(e) => {
                    errors.push(format!("{}: {}", file.name, e));
                }
            }
        }
    }

    println!("[清理重复文件] 完成: {} 组重复, 移入回收站 {} 个文件, 错误 {} 个",
        total_groups, moved_count, errors.len());

    Ok((total_groups, moved_count, errors))
}

/// Tauri 命令：查找重复文件
#[tauri::command]
pub fn find_duplicate_files_cmd() -> Result<Vec<DuplicateFileGroup>, String> {
    find_duplicate_files()
}

/// Tauri 命令：清理重复文件
#[tauri::command]
pub fn clean_duplicate_files_cmd() -> Result<(usize, usize, Vec<String>), String> {
    clean_duplicate_files()
}