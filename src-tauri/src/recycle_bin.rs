use tauri::Runtime;

/// Tauri 命令：清空回收站（弹出 Windows 原生确认对话框）
#[tauri::command]
pub fn empty_recycle_bin_cmd<R: Runtime>(window: tauri::Window<R>) -> Result<bool, String> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Shell::SHEmptyRecycleBinW;
        use windows_sys::Win32::Foundation::HWND;

        let hwnd = window.hwnd().map_err(|e| format!("获取窗口句柄失败: {}", e))?;
        let hwnd_raw = hwnd.0 as HWND;

        let dw_flags: u32 = 0;

        let result = unsafe {
            SHEmptyRecycleBinW(hwnd_raw, std::ptr::null(), dw_flags)
        };

        if result == 0 {
            Ok(true)
        } else {
            if result == 1 {
                Ok(false)
            } else {
                Err(format!("清空回收站失败，错误码: {:#x}", result))
            }
        }
    }
    #[cfg(not(windows))]
    {
        Err("清空回收站功能仅支持 Windows 系统".to_string())
    }
}
