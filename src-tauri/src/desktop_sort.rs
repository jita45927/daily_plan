use std::fs;
use std::path::{Path, PathBuf};

#[derive(serde::Deserialize)]
pub enum ConflictStrategy {
    Overwrite,
    Rename,
    Skip,
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

fn create_target_folders(desktop_path: &Path) -> Result<(PathBuf, PathBuf), String> {
    let shortcuts_folder = desktop_path.join("快捷方式");
    let other_files_folder = desktop_path.join("其他文件");
    
    if !shortcuts_folder.exists() {
        fs::create_dir(&shortcuts_folder).map_err(|e| format!("创建快捷方式文件夹失败: {}", e))?;
    }
    
    if !other_files_folder.exists() {
        fs::create_dir(&other_files_folder).map_err(|e| format!("创建其他文件文件夹失败: {}", e))?;
    }
    
    Ok((shortcuts_folder, other_files_folder))
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

pub fn organize_desktop(strategy: ConflictStrategy) -> Result<(usize, usize, Vec<String>), String> {
    let desktop_path_str = get_desktop_path()?;
    let desktop_path = Path::new(&desktop_path_str);
    
    if !desktop_path.exists() {
        return Err("桌面路径不存在".to_string());
    }
    
    let (shortcuts_folder, other_files_folder) = create_target_folders(desktop_path)?;
    
    let entries = fs::read_dir(desktop_path).map_err(|e| format!("读取桌面目录失败: {}", e))?;
    
    let mut shortcuts_count = 0;
    let mut other_count = 0;
    let mut errors = Vec::new();
    
    for entry in entries {
        match entry {
            Ok(entry) => {
                let path = entry.path();
                let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                
                if file_name == "快捷方式" || file_name == "其他文件" {
                    continue;
                }
                
                if path.is_file() {
                    if let Some(ext) = path.extension() {
                        if ext.to_ascii_lowercase() == "lnk" {
                            let target = shortcuts_folder.join(&file_name);
                            if let Err(e) = move_file(&path, &target, &strategy) {
                                errors.push(format!("{}: {}", file_name, e));
                            } else {
                                shortcuts_count += 1;
                            }
                            continue;
                        }
                    }
                    
                    let target = other_files_folder.join(&file_name);
                    if let Err(e) = move_file(&path, &target, &strategy) {
                        errors.push(format!("{}: {}", file_name, e));
                    } else {
                        other_count += 1;
                    }
                } else if path.is_dir() {
                    let target = other_files_folder.join(&file_name);
                    if let Err(e) = move_dir(&path, &target, &strategy) {
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
    
    Ok((shortcuts_count, other_count, errors))
}