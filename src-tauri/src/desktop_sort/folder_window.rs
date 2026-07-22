use std::sync::Arc;
use tauri::{Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};
use crate::window::WindowManager;
use super::folder_analyze::{
    DownloadsAnalyzeManager, DownloadsAnalysis, analyze_downloads,
    check_downloads_conflicts_before_organize, organize_downloads, get_downloads_path,
};
use super::common::ConflictStrategy;

const DOWNLOADS_ANALYZE_WIN_WIDTH: f64 = 720.0;
const DOWNLOADS_ANALYZE_WIN_HEIGHT: f64 = 560.0;

#[cfg(target_os = "windows")]
fn disable_minimize_maximize<R>(window: &tauri::WebviewWindow<R>)
where
    R: tauri::Runtime,
{
    use winapi::um::winuser::{GetWindowLongW, SetWindowLongW, GWL_STYLE, WS_MINIMIZEBOX, WS_MAXIMIZEBOX};
    
    let hwnd = match window.hwnd() {
        Ok(h) => h,
        Err(_) => return,
    };
    
    unsafe {
        let style = GetWindowLongW(hwnd.0 as *mut _, GWL_STYLE);
        let new_style = style & !((WS_MINIMIZEBOX | WS_MAXIMIZEBOX) as i32);
        SetWindowLongW(hwnd.0 as *mut _, GWL_STYLE, new_style);
    }
}

#[cfg(not(target_os = "windows"))]
fn disable_minimize_maximize<R>(_window: &tauri::WebviewWindow<R>)
where
    R: tauri::Runtime,
{
}

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
    .resizable(false)
    .visible(false)
    .position(-3000.0, -3000.0)
    .build()
    .map_err(|e| format!("创建文件夹分析窗口失败: {:?}", e))?;

    // 禁用最小化和最大化按钮，只保留关闭按钮
    disable_minimize_maximize(&analyze_win);

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
pub fn analyze_downloads_cmd<R: Runtime>(window: tauri::Window<R>, custom_path: Option<String>) -> Result<bool, String> {
    let app = window.app_handle();
    println!("[文件夹分析] analyze_downloads_cmd 命令被调用, custom_path: {:?}", custom_path);

    let analysis = analyze_downloads(custom_path)?;

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

    disable_minimize_maximize(&analyze_win);

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
pub fn check_downloads_conflicts_cmd(custom_path: Option<String>) -> Result<Vec<super::common::ConflictFile>, String> {
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