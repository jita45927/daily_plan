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
        use winapi::shared::minwindef::{DWORD, HKEY, LPBYTE};
        use winapi::um::winreg::{RegOpenKeyExW, RegQueryValueExW, RegCloseKey, HKEY_CURRENT_USER};
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStrExt;

        const KEY_READ: DWORD = 0x20019;
        const REG_DWORD: DWORD = 4;

        fn to_wide(s: &str) -> Vec<u16> {
            OsString::from(s)
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect()
        }

        unsafe fn read_reg_dword(hkey: HKEY, subkey: &str, value_name: &str) -> Option<u32> {
            let subkey_wide = to_wide(subkey);
            let mut hk_result: HKEY = std::ptr::null_mut();
            
            if RegOpenKeyExW(hkey, subkey_wide.as_ptr(), 0, KEY_READ, &mut hk_result) != 0 {
                return None;
            }

            let value_name_wide = to_wide(value_name);
            let mut data_type: DWORD = 0;
            let mut data: DWORD = 0;
            let mut data_size: DWORD = std::mem::size_of::<DWORD>() as DWORD;

            let result = RegQueryValueExW(
                hk_result,
                value_name_wide.as_ptr(),
                std::ptr::null_mut(),
                &mut data_type,
                &mut data as *mut _ as LPBYTE,
                &mut data_size,
            );

            RegCloseKey(hk_result);

            if result == 0 && data_type == REG_DWORD {
                Some(data)
            } else {
                None
            }
        }

        let subkeys = [
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\HideDesktopIcons\NewStartPanel",
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\HideDesktopIcons\ClassicStartPanel",
        ];

        for subkey in &subkeys {
            if let Some(value) = unsafe { read_reg_dword(HKEY_CURRENT_USER, subkey, clsid) } {
                // 值为 0x1 表示隐藏，值为 0x0 表示显示
                return value != 1;
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

    let mut virtual_count = 0;
    let mut program_shortcut_count = 0;
    let mut other_shortcut_count = 0;
    let mut image_count = 0;
    let mut regular_count = 0;

    for item in &items {
        match item.category.as_str() {
            "virtual" => virtual_count += 1,
            "programShortcut" => program_shortcut_count += 1,
            "otherShortcut" => other_shortcut_count += 1,
            "image" => image_count += 1,
            _ => regular_count += 1,
        }
    }

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
    let url = if cfg!(debug_assertions) {
        WebviewUrl::External("http://localhost:5173/desktop-analyze.html".parse().unwrap())
    } else {
        WebviewUrl::App("/desktop-analyze.html".into())
    };
    let analyze_win = WebviewWindowBuilder::new(
        app,
        "desktop_analyze",
        url,
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

    #[derive(Clone)]
    struct FileInfo {
        name: String,
        path: String,
        folder: String,
        size: u64,
    }

    let mut all_files: Vec<FileInfo> = Vec::new();
    let mut errors = Vec::new();
    let mut total_files = 0usize;

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

            total_files += 1;
            all_files.push(FileInfo {
                name: file_name,
                path: path.to_string_lossy().to_string(),
                folder: folder_name.to_string(),
                size: file_size,
            });
        }
    }

    println!("[清理重复文件] 共扫描 {} 个文件，正在按大小初筛...", total_files);

    let mut files_by_size: std::collections::HashMap<u64, Vec<FileInfo>> = std::collections::HashMap::new();
    for file in all_files {
        files_by_size.entry(file.size).or_default().push(file);
    }

    let candidate_count: usize = files_by_size
        .values()
        .filter(|v| v.len() > 1)
        .map(|v| v.len())
        .sum();

    println!("[清理重复文件] 大小初筛后，需计算哈希的文件: {} 个（跳过 {} 个大小唯一的文件）",
        candidate_count, total_files - candidate_count);

    let mut files_by_hash: std::collections::HashMap<String, Vec<DuplicateFile>> = std::collections::HashMap::new();
    let mut size_map: std::collections::HashMap<String, u64> = std::collections::HashMap::new();

    for (size, files) in &files_by_size {
        if files.len() <= 1 {
            continue;
        }

        for file_info in files {
            let path = Path::new(&file_info.path);
            let hash = match compute_file_hash(path) {
                Ok(h) => h,
                Err(e) => {
                    errors.push(format!("计算文件哈希失败 {}: {}", file_info.name, e));
                    continue;
                }
            };

            let dup_file = DuplicateFile {
                name: file_info.name.clone(),
                path: file_info.path.clone(),
                folder: file_info.folder.clone(),
            };

            size_map.insert(hash.clone(), *size);
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

// ==================== 文件夹分析功能 ====================

pub fn get_downloads_path() -> Result<String, String> {
    let path: PathBuf;
    
    #[cfg(windows)]
    {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;
        use windows_sys::Win32::UI::Shell::SHGetFolderPathW;
        use windows_sys::Win32::Foundation::MAX_PATH;
        
        const CSIDL_DOWNLOADS: i32 = 0x002C;
        
        let mut buffer: [u16; MAX_PATH as usize] = [0; MAX_PATH as usize];
        
        let result = unsafe {
            SHGetFolderPathW(
                0,
                CSIDL_DOWNLOADS,
                0,
                0,
                buffer.as_mut_ptr()
            )
        };
        
        if result != 0 {
            return Err("获取下载目录路径失败".to_string());
        }
        
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(MAX_PATH as usize);
        let wide_str = &buffer[0..len];
        let os_string = OsString::from_wide(wide_str);
        
        path = PathBuf::from(os_string);
    }
    
    #[cfg(not(windows))]
    {
        path = dirs::download_dir().ok_or("无法获取下载目录路径")?;
    }
    
    path.to_str().map(|s| s.to_string()).ok_or("下载目录路径转换失败".to_string())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadsItem {
    pub name: String,
    pub category: String,
    pub path: Option<String>,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadsAnalysis {
    pub downloads_path: String,
    pub items: Vec<DownloadsItem>,
    pub exe_count: usize,
    pub image_count: usize,
    pub archive_count: usize,
    pub other_count: usize,
    pub errors: Vec<String>,
}

pub struct DownloadsAnalyzeManager {
    current_analysis: Mutex<Option<DownloadsAnalysis>>,
}

impl DownloadsAnalyzeManager {
    pub fn new() -> Self {
        Self {
            current_analysis: Mutex::new(None),
        }
    }

    pub fn set(&self, analysis: DownloadsAnalysis) {
        *self.current_analysis.lock().unwrap() = Some(analysis);
    }

    pub fn get(&self) -> Option<DownloadsAnalysis> {
        self.current_analysis.lock().unwrap().clone()
    }
}

impl Default for DownloadsAnalyzeManager {
    fn default() -> Self {
        Self::new()
    }
}

pub fn analyze_downloads(custom_path: Option<String>) -> Result<DownloadsAnalysis, String> {
    let downloads_path = if let Some(path) = custom_path {
        path
    } else {
        get_downloads_path()?
    };

    println!("[文件夹分析] 目录: {}", downloads_path);

    let downloads_path_obj = Path::new(&downloads_path);
    if !downloads_path_obj.exists() {
        return Err(format!("目录不存在: {}", downloads_path));
    }

    let mut items = Vec::new();
    let mut errors = Vec::new();

    let skip_folders: &[&str] = &[
        "可执行文件",
        "图片文件",
        "其他文件",
        "压缩包",
    ];

    let image_exts: &[&str] = &[
        "jpg", "jpeg", "png", "gif", "bmp", "webp", "tiff", "tif", "svg", "ico", "heic", "heif",
        "raw", "cr2", "nef", "orf", "dng", "arw",
    ];

    let exe_exts: &[&str] = &[
        "exe", "msi", "msu", "bat", "cmd", "com", "scr", "pif",
    ];

    let archive_exts: &[&str] = &[
        "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "z", "cab", "iso", "img",
        "tgz", "tbz", "tar.gz", "tar.bz2",
    ];

    let entries = match fs::read_dir(downloads_path_obj) {
        Ok(e) => e,
        Err(e) => {
            return Err(format!("读取目录失败: {}", e));
        }
    };

    for entry in entries {
        match entry {
            Ok(entry) => {
                let path = entry.path();
                let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

                if skip_folders.contains(&file_name.as_str()) {
                    continue;
                }

                if file_name.eq_ignore_ascii_case("desktop.ini") {
                    continue;
                }

                let is_dir = path.is_dir();
                let ext_str = path.extension().map(|e| e.to_ascii_lowercase().to_string_lossy().to_string());
                let path_str = path.to_string_lossy().to_string();

                let category = if is_dir {
                    "other".to_string()
                } else if let Some(ref e) = ext_str {
                    if exe_exts.contains(&e.as_str()) {
                        "exe".to_string()
                    } else if image_exts.contains(&e.as_str()) {
                        "image".to_string()
                    } else if archive_exts.contains(&e.as_str()) {
                        "archive".to_string()
                    } else {
                        "other".to_string()
                    }
                } else {
                    "other".to_string()
                };

                println!("[文件夹分析] {}: {}", category, file_name);
                items.push(DownloadsItem {
                    name: file_name,
                    category,
                    path: Some(path_str),
                    is_dir,
                });
            }
            Err(e) => {
                errors.push(format!("读取目录项失败: {}", e));
            }
        }
    }

    println!("[文件夹分析] 枚举完成，共 {} 项", items.len());

    let mut exe_count = 0;
    let mut image_count = 0;
    let mut archive_count = 0;
    let mut other_count = 0;

    for item in &items {
        match item.category.as_str() {
            "exe" => exe_count += 1,
            "image" => image_count += 1,
            "archive" => archive_count += 1,
            _ => other_count += 1,
        }
    }

    Ok(DownloadsAnalysis {
        downloads_path,
        items,
        exe_count,
        image_count,
        archive_count,
        other_count,
        errors,
    })
}

fn create_download_target_folders(downloads_path: &Path) -> Result<(PathBuf, PathBuf, PathBuf, PathBuf), String> {
    let exe_folder = downloads_path.join("可执行文件");
    let image_folder = downloads_path.join("图片文件");
    let other_folder = downloads_path.join("其他文件");
    let archive_folder = downloads_path.join("压缩包");

    if !exe_folder.exists() {
        fs::create_dir(&exe_folder).map_err(|e| format!("创建可执行文件文件夹失败: {}", e))?;
    }

    if !image_folder.exists() {
        fs::create_dir(&image_folder).map_err(|e| format!("创建图片文件文件夹失败: {}", e))?;
    }

    if !other_folder.exists() {
        fs::create_dir(&other_folder).map_err(|e| format!("创建其他文件文件夹失败: {}", e))?;
    }

    if !archive_folder.exists() {
        fs::create_dir(&archive_folder).map_err(|e| format!("创建压缩包文件夹失败: {}", e))?;
    }

    Ok((exe_folder, image_folder, other_folder, archive_folder))
}

pub fn organize_downloads(strategy: ConflictStrategy, custom_path: Option<String>) -> Result<(usize, usize, usize, usize, Vec<String>), String> {
    let downloads_path_str = if let Some(path) = custom_path {
        path
    } else {
        get_downloads_path()?
    };
    let downloads_path = Path::new(&downloads_path_str);

    if !downloads_path.exists() {
        return Err("下载目录路径不存在".to_string());
    }

    let (exe_folder, image_folder, other_folder, archive_folder) =
        create_download_target_folders(downloads_path)?;

    let mut exe_count = 0;
    let mut image_count = 0;
    let mut other_count = 0;
    let mut archive_count = 0;
    let mut errors = Vec::new();

    let skip_folders: &[&str] = &[
        "可执行文件",
        "图片文件",
        "其他文件",
        "压缩包",
    ];

    let image_exts: &[&str] = &[
        "jpg", "jpeg", "png", "gif", "bmp", "webp", "tiff", "tif", "svg", "ico", "heic", "heif",
        "raw", "cr2", "nef", "orf", "dng", "arw",
    ];

    let exe_exts: &[&str] = &[
        "exe", "msi", "msu", "bat", "cmd", "com", "scr", "pif",
    ];

    let archive_exts: &[&str] = &[
        "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "z", "cab", "iso", "img",
        "tgz", "tbz", "tar.gz", "tar.bz2",
    ];

    let entries = match fs::read_dir(downloads_path) {
        Ok(e) => e,
        Err(e) => {
            return Err(format!("读取下载目录失败: {}", e));
        }
    };

    for entry in entries {
        match entry {
            Ok(entry) => {
                let path = entry.path();
                let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();

                if skip_folders.contains(&file_name.as_str()) {
                    continue;
                }

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
                        if exe_exts.contains(&ext_str.as_str()) {
                            let target_path = exe_folder.join(&file_name);
                            if let Err(e) = move_file(&path, &target_path, &strategy) {
                                errors.push(format!("{}: {}", file_name, e));
                            } else {
                                exe_count += 1;
                            }
                            continue;
                        }

                        if image_exts.contains(&ext_str.as_str()) {
                            let target_path = image_folder.join(&file_name);
                            if let Err(e) = move_file(&path, &target_path, &strategy) {
                                errors.push(format!("{}: {}", file_name, e));
                            } else {
                                image_count += 1;
                            }
                            continue;
                        }

                        if archive_exts.contains(&ext_str.as_str()) {
                            let target_path = archive_folder.join(&file_name);
                            if let Err(e) = move_file(&path, &target_path, &strategy) {
                                errors.push(format!("{}: {}", file_name, e));
                            } else {
                                archive_count += 1;
                            }
                            continue;
                        }
                    }

                    let target_path = other_folder.join(&file_name);
                    if let Err(e) = move_file(&path, &target_path, &strategy) {
                        errors.push(format!("{}: {}", file_name, e));
                    } else {
                        other_count += 1;
                    }
                } else if path.is_dir() {
                    let target_path = other_folder.join(&file_name);
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
        exe_count,
        image_count,
        archive_count,
        other_count,
        errors,
    ))
}

pub fn check_downloads_conflicts_before_organize(custom_path: Option<String>) -> Result<Vec<ConflictFile>, String> {
    let downloads_path_str = if let Some(path) = custom_path {
        path
    } else {
        get_downloads_path()?
    };
    let downloads_path = Path::new(&downloads_path_str);

    if !downloads_path.exists() {
        return Err("下载目录路径不存在".to_string());
    }

    let (exe_folder, image_folder, other_folder, archive_folder) =
        create_download_target_folders(downloads_path)?;

    let skip_folders: &[&str] = &[
        "可执行文件",
        "图片文件",
        "其他文件",
        "压缩包",
    ];

    let image_exts: &[&str] = &[
        "jpg", "jpeg", "png", "gif", "bmp", "webp", "tiff", "tif", "svg", "ico", "heic", "heif",
        "raw", "cr2", "nef", "orf", "dng", "arw",
    ];

    let exe_exts: &[&str] = &[
        "exe", "msi", "msu", "bat", "cmd", "com", "scr", "pif",
    ];

    let archive_exts: &[&str] = &[
        "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "z", "cab", "iso", "img",
        "tgz", "tbz", "tar.gz", "tar.bz2",
    ];

    let mut conflicts = Vec::new();

    let entries = match fs::read_dir(downloads_path) {
        Ok(e) => e,
        Err(e) => {
            return Err(format!("读取下载目录失败: {}", e));
        }
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

        let target_folder = if path.is_file() {
            let ext = path.extension().map(|e| {
                e.to_ascii_lowercase().to_string_lossy().to_string().to_lowercase()
            });
            if let Some(ref ext_str) = ext {
                if exe_exts.contains(&ext_str.as_str()) {
                    &exe_folder
                } else if image_exts.contains(&ext_str.as_str()) {
                    &image_folder
                } else if archive_exts.contains(&ext_str.as_str()) {
                    &archive_folder
                } else {
                    &other_folder
                }
            } else {
                &other_folder
            }
        } else {
            &other_folder
        };

        let target_path = target_folder.join(&file_name);
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

    Ok(conflicts)
}

// ==================== 文件夹分析窗口 ====================

const DOWNLOADS_ANALYZE_WIN_WIDTH: f64 = 720.0;
const DOWNLOADS_ANALYZE_WIN_HEIGHT: f64 = 560.0;

pub fn setup_downloads_analyze_window<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), String> {
    println!("[文件夹分析] 预创建文件夹分析窗口...");
    let url = if cfg!(debug_assertions) {
        WebviewUrl::External("http://localhost:5173/downloads-analyze.html".parse().unwrap())
    } else {
        WebviewUrl::App("/downloads-analyze.html".into())
    };
    let analyze_win = WebviewWindowBuilder::new(
        app,
        "downloads_analyze",
        url,
    )
    .title("文件夹分析")
    .inner_size(DOWNLOADS_ANALYZE_WIN_WIDTH, DOWNLOADS_ANALYZE_WIN_HEIGHT)
    .decorations(true)
    .transparent(false)
    .always_on_top(true)
    .skip_taskbar(false)
    .resizable(true)
    .visible(false)
    .position(-3000.0, -3000.0)
    .build()
    .map_err(|e| format!("创建文件夹分析窗口失败: {:?}", e))?;

    let win_clone = analyze_win.clone();
    analyze_win.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = win_clone.hide();
            let _ = win_clone.set_position(tauri::PhysicalPosition::new(-3000, -3000));
        }
    });

    println!("[文件夹分析] 文件夹分析窗口预创建完成");
    Ok(())
}

#[tauri::command]
pub fn analyze_downloads_cmd<R: Runtime>(window: tauri::Window<R>) -> Result<bool, String> {
    let app = window.app_handle();
    println!("[文件夹分析] analyze_downloads_cmd 命令被调用");

    let analysis = analyze_downloads(None)?;

    println!("[文件夹分析] 分析完成，共 {} 项 (可执行文件:{}, 图片:{}, 压缩包:{}, 其他:{})",
        analysis.items.len(),
        analysis.exe_count,
        analysis.image_count,
        analysis.archive_count,
        analysis.other_count
    );

    let manager = app.state::<Arc<DownloadsAnalyzeManager>>();
    manager.set(analysis);
    println!("[文件夹分析] 分析结果已存储到 DownloadsAnalyzeManager");

    Ok(true)
}

#[tauri::command]
pub fn show_downloads_analyze_window<R: Runtime>(window: tauri::Window<R>) -> Result<bool, String> {
    let app = window.app_handle();
    println!("[文件夹分析] show_downloads_analyze_window 命令被调用");

    let analyze_win = app.get_webview_window("downloads_analyze")
        .ok_or_else(|| {
            println!("[文件夹分析] 错误: 文件夹分析窗口未初始化");
            "文件夹分析窗口未初始化".to_string()
        })?;
    println!("[文件夹分析] 成功获取文件夹分析窗口");

    let (x, y) = if let Ok(Some(monitor)) = window.primary_monitor() {
        let work_area = monitor.work_area();
        let px = (work_area.size.width as f64 - DOWNLOADS_ANALYZE_WIN_WIDTH) / 2.0 + work_area.position.x as f64;
        let py = (work_area.size.height as f64 - DOWNLOADS_ANALYZE_WIN_HEIGHT) / 2.0 + work_area.position.y as f64;
        println!("[文件夹分析] 窗口位置: ({}, {})", px, py);
        (px, py)
    } else {
        println!("[文件夹分析] 无法获取主屏幕信息，使用默认位置");
        (100.0, 100.0)
    };

    let pos_result = analyze_win.set_position(tauri::LogicalPosition::new(x, y));
    println!("[文件夹分析] set_position 结果: {:?}", pos_result);

    let show_result = analyze_win.show();
    println!("[文件夹分析] show 结果: {:?}", show_result);

    let focus_result = analyze_win.set_focus();
    println!("[文件夹分析] set_focus 结果: {:?}", focus_result);

    let win_manager = app.state::<Arc<WindowManager>>();
    win_manager.expand_window(&app);

    let manager = app.state::<Arc<DownloadsAnalyzeManager>>();
    if let Some(analysis) = manager.get() {
        let emit_result = analyze_win.emit("downloads-analyze-reload", analysis);
        println!("[文件夹分析] emit downloads-analyze-reload 结果: {:?}", emit_result);
    } else {
        println!("[文件夹分析] 警告: 没有找到分析结果");
    }

    println!("[文件夹分析] show_downloads_analyze_window 命令执行完成");
    Ok(true)
}

#[tauri::command]
pub fn get_downloads_analysis<R: Runtime>(window: tauri::Window<R>) -> Option<DownloadsAnalysis> {
    let manager = window.app_handle().state::<Arc<DownloadsAnalyzeManager>>();
    manager.get()
}

#[tauri::command]
pub fn close_downloads_analyze<R: Runtime>(window: tauri::Window<R>) {
    if let Some(win) = window.app_handle().get_webview_window("downloads_analyze") {
        let _ = win.hide();
        let _ = win.set_position(tauri::PhysicalPosition::new(-3000, -3000));
    }
}

#[tauri::command]
pub fn check_downloads_conflicts_cmd(custom_path: Option<String>) -> Result<Vec<ConflictFile>, String> {
    check_downloads_conflicts_before_organize(custom_path)
}

#[tauri::command]
pub fn organize_downloads_cmd<R: Runtime>(_window: tauri::Window<R>, strategy: ConflictStrategy, custom_path: Option<String>) -> Result<(usize, usize, usize, usize, Vec<String>), String> {
    organize_downloads(strategy, custom_path)
}

#[tauri::command]
pub fn get_downloads_path_cmd() -> Result<String, String> {
    get_downloads_path()
}