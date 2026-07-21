use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub enum ConflictStrategy {
    Overwrite,
    Rename,
    Skip,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictFile {
    pub file_name: String,
    pub source_path: String,
    pub target_folder: String,
    pub target_path: String,
}

pub fn get_public_desktop_path() -> Option<String> {
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

pub fn read_lnk_target(lnk_path: &str) -> Option<String> {
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

pub fn is_desktop_icon_visible(clsid: &str) -> bool {
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
                return value != 1;
            }
        }

        clsid.eq_ignore_ascii_case("{645FF040-5081-101B-9F08-00AA002F954E}")
    }
    #[cfg(not(windows))]
    {
        let _ = clsid;
        false
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

pub fn is_image_file(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp"
            | "tiff" | "tif" | "svg" | "ico" | "heic" | "heif"
    )
}

pub fn compute_file_hash(path: &Path) -> Result<String, String> {
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

pub fn move_to_recycle_bin(path: &Path) -> Result<(), String> {
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

pub fn create_target_folders(desktop_path: &Path) -> Result<(PathBuf, PathBuf, PathBuf, PathBuf), String> {
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

pub fn generate_unique_path(target_folder: &Path, file_name: &str) -> PathBuf {
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

pub fn move_file_safe(source: &Path, target: &Path) -> Result<(), String> {
    match fs::rename(source, target) {
        Ok(()) => Ok(()),
        Err(_) => {
            fs::copy(source, target)
                .map_err(|e| format!("复制文件失败(跨盘符): {}", e))?;
            fs::remove_file(source)
                .map_err(|e| format!("删除源文件失败(跨盘符): {}", e))?;
            Ok(())
        }
    }
}

pub fn move_dir_safe(source: &Path, target: &Path) -> Result<(), String> {
    match fs::rename(source, target) {
        Ok(()) => Ok(()),
        Err(_) => {
            copy_dir_recursive(source, target)
                .map_err(|e| format!("复制文件夹失败(跨盘符): {}", e))?;
            fs::remove_dir_all(source)
                .map_err(|e| format!("删除源文件夹失败(跨盘符): {}", e))?;
            Ok(())
        }
    }
}

pub fn copy_dir_recursive(source: &Path, target: &Path) -> Result<(), String> {
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

pub fn move_file(source: &Path, target: &Path, strategy: &ConflictStrategy) -> Result<(), String> {
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

pub fn move_dir(source: &Path, target: &Path, strategy: &ConflictStrategy) -> Result<(), String> {
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