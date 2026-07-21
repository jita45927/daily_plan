use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use serde::Serialize;
use super::common::{
    ConflictStrategy, ConflictFile, get_public_desktop_path, read_lnk_target,
    is_desktop_icon_visible, get_desktop_path, is_image_file, create_target_folders,
    move_file, move_dir,
};

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

fn get_desktop_virtual_icons() -> Vec<DesktopItem> {
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

pub fn analyze_desktop() -> Result<DesktopAnalysis, String> {
    let desktop_path = get_desktop_path()?;
    let public_desktop_path = get_public_desktop_path();

    println!("[桌面分析] 用户桌面: {}", desktop_path);
    if let Some(ref p) = public_desktop_path {
        println!("[桌面分析] 公共桌面: {}", p);
    }

    let handle = std::thread::spawn(move || {
        #[cfg(windows)]
        {
            use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
            let co_hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
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

    let virtual_items = get_desktop_virtual_icons();
    items.extend(virtual_items);

    enumerate_desktop_folder(desktop_path, &mut items, &mut errors);

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
                        let is_image = ext.as_ref().and_then(|e| e.to_str()).map(|e| is_image_file(e)).unwrap_or(false);

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

                            if ext_str == "url" {
                                let target_path = other_shortcuts_folder.join(&file_name);
                                if let Err(e) = move_file(&path, &target_path, &strategy) {
                                    errors.push(format!("{}: {}", file_name, e));
                                } else {
                                    other_shortcut_count += 1;
                                }
                                continue;
                            }

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

                        let target_path = other_files_folder.join(&file_name);
                        if let Err(e) = move_file(&path, &target_path, &strategy) {
                            errors.push(format!("{}: {}", file_name, e));
                        } else {
                            other_count += 1;
                        }
                    } else if path.is_dir() {
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