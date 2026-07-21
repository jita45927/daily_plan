use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use serde::Serialize;
use super::common::{ConflictStrategy, ConflictFile, move_file, move_dir};

pub fn get_downloads_path() -> Result<String, String> {
    dirs::download_dir()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .ok_or("无法获取系统下载目录".to_string())
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