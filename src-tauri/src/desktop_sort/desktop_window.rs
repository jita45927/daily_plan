use std::sync::Arc;
use tauri::{Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};
use crate::window::WindowManager;
use super::desktop_analyze::{DesktopAnalyzeManager, DesktopAnalysis, analyze_desktop};

const ANALYZE_WIN_WIDTH: f64 = 720.0;
const ANALYZE_WIN_HEIGHT: f64 = 560.0;

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

#[tauri::command]
pub fn show_analyze_window<R: Runtime>(window: tauri::Window<R>) -> Result<bool, String> {
    let app = window.app_handle();

    let analyze_win = app.get_webview_window("desktop_analyze")
        .ok_or_else(|| "分析窗口未初始化".to_string())?;

    let (x, y) = if let Ok(Some(monitor)) = window.primary_monitor() {
        let work_area = monitor.work_area();
        let px = (work_area.size.width as f64 - ANALYZE_WIN_WIDTH) / 2.0 + work_area.position.x as f64;
        let py = (work_area.size.height as f64 - ANALYZE_WIN_HEIGHT) / 2.0 + work_area.position.y as f64;
        (px, py)
    } else {
        (100.0, 100.0)
    };

    let _ = analyze_win.set_position(tauri::LogicalPosition::new(x, y));
    let _ = analyze_win.show();
    let _ = analyze_win.set_focus();

    let win_manager = app.state::<Arc<WindowManager>>();
    win_manager.expand_window(&app);

    let manager = app.state::<Arc<DesktopAnalyzeManager>>();
    if let Some(analysis) = manager.get() {
        let _ = analyze_win.emit("desktop-analyze-reload", analysis);
    }

    Ok(true)
}

#[tauri::command]
pub fn get_desktop_analysis<R: Runtime>(window: tauri::Window<R>) -> Option<DesktopAnalysis> {
    let manager = window.app_handle().state::<Arc<DesktopAnalyzeManager>>();
    manager.get()
}

#[tauri::command]
pub fn close_desktop_analyze<R: Runtime>(window: tauri::Window<R>) {
    if let Some(win) = window.app_handle().get_webview_window("desktop_analyze") {
        let _ = win.hide();
        let _ = win.set_position(tauri::PhysicalPosition::new(-3000, -3000));
    }
}