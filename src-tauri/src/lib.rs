mod db;
mod desktop_sort;
mod window;
mod task_timer;
mod context_menu;
mod snap_line;
mod clean_computer;
mod recycle_bin;

use std::sync::Arc;
use base64::Engine;

use tauri::{Manager, Emitter};
use tauri_plugin_dialog;
use db::{reinitialize_db, save_db_window_config,
    insert_task_cmd, get_all_tasks_cmd, update_task_cmd, delete_task_cmd,
    delete_completed_tasks_cmd, delete_all_tasks_cmd, move_task_to_trash_cmd,
    get_deleted_tasks_cmd, restore_task_cmd, permanently_delete_task_cmd,
    clear_trash_by_period_cmd, move_completed_to_trash_cmd, move_all_to_trash_cmd,
    reinitialize_db_cmd, reorder_tasks_cmd, get_db_window_config_cmd, save_db_window_config_cmd,
};
use desktop_sort::{
    get_desktop_path, organize_desktop, ConflictStrategy, DesktopAnalyzeManager,
    analyze_desktop_cmd, show_analyze_window, get_desktop_analysis, close_desktop_analyze,
    setup_desktop_analyze_window, check_conflicts_before_organize, ConflictFile,
    find_duplicate_files_cmd, clean_duplicate_files_cmd,
    DownloadsAnalyzeManager,
    analyze_downloads_cmd, show_downloads_analyze_window, get_downloads_analysis,
    close_downloads_analyze, setup_downloads_analyze_window, check_downloads_conflicts_cmd,
    organize_downloads_cmd, get_downloads_path_cmd,
    find_duplicate_files_for_folder_cmd, clean_duplicate_files_for_folder_cmd,
};
use window::{
    get_window_config, save_window_config, set_always_on_top,
    start_dragging, stop_dragging, toggle_window_lock, collapse_to_snap_line,
    set_main_menu_open, is_main_menu_open, WindowManager,
};
use task_timer::{
    calibrate_timer_cmd, get_timer_status_cmd, start_countdown_cmd, start_scheduled_timer_cmd,
    restore_scheduled_timer_cmd, stop_timer_cmd, play_alarm_cmd, stop_alarm_cmd, TimerManager,
};
use context_menu::{
    close_context_menu, context_menu_action, get_context_menu_task, setup_context_menu_window,
    show_context_menu, ContextMenuManager,
    setup_trash_context_menu_window, show_trash_context_menu, close_trash_context_menu,
    get_trash_context_menu_task, trash_context_menu_action,
};
use snap_line::{SnapLineManager, setup_snap_line_window, expand_from_snap_line};
use clean_computer::{CleanComputerManager, clean_computer_cmd, get_clean_computer_status};
use recycle_bin::empty_recycle_bin_cmd;

use std::sync::atomic::{AtomicBool, Ordering};

static APP_READY: AtomicBool = AtomicBool::new(false);

#[tauri::command]
fn on_app_ready(app_handle: tauri::AppHandle) -> Result<String, String> {
    APP_READY.store(true, Ordering::SeqCst);
    let _ = app_handle.emit_to("welcome", "app_ready", serde_json::json!({}));
    Ok("主窗口加载完成".to_string())
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn exit_app<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    app.exit(0);
}

#[tauri::command]
fn reset_app_cmd(window: tauri::Window) -> Result<bool, String> {
    reinitialize_db().map_err(|e| e.to_string())?;

    let primary_monitor = window.app_handle().primary_monitor()
        .map_err(|e| format!("获取主屏幕信息失败: {}", e))?
        .ok_or_else(|| "无法获取主屏幕信息".to_string())?;
    let work_area = primary_monitor.work_area();
    let center_x = (work_area.size.width as f64 - 300.0) / 2.0 + work_area.position.x as f64;
    let center_y = (work_area.size.height as f64 - 600.0) / 2.0 + work_area.position.y as f64;

    save_db_window_config(center_x, center_y, 600.0, false).map_err(|e| e.to_string())?;

    let manager = window.app_handle().state::<Arc<WindowManager>>();
    let mut default_config = window::WindowConfigData::default();
    default_config.x = center_x;
    default_config.y = center_y;
    default_config.height = 600.0;
    manager.save_config(default_config);
    manager.apply_config_to_window(&window);
    manager.reset_snap_state(&window);

    if let Some(menu_win) = window.app_handle().get_webview_window("context_menu") {
        let _ = menu_win.hide();
    }

    Ok(true)
}

#[tauri::command]
fn get_desktop_path_cmd() -> Result<String, String> {
    get_desktop_path()
}

#[tauri::command]
fn organize_desktop_cmd(strategy: ConflictStrategy) -> Result<(usize, usize, usize, usize, Vec<String>), String> {
    organize_desktop(strategy)
}

#[tauri::command]
fn check_conflicts_cmd() -> Result<Vec<ConflictFile>, String> {
    check_conflicts_before_organize()
}

#[tauri::command]
fn set_window_size(window: tauri::Window, width: f64, height: f64) {
    use winapi::um::winuser::{SetWindowPos, SWP_NOZORDER, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOCOPYBITS};

    let manager = window.app_handle().state::<Arc<WindowManager>>();
    manager.set_resizing(true);

    let scale_factor = window.scale_factor().unwrap_or(1.0);
    let hwnd = window.hwnd().unwrap();

    unsafe {
        SetWindowPos(
            hwnd.0 as *mut _,
            std::ptr::null_mut(),
            0, 0,
            (width * scale_factor) as i32,
            (height * scale_factor) as i32,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOCOPYBITS,
        );
    }

    manager.update_config_size(width, height);
    manager.set_resizing(false);
}

#[tauri::command]
fn set_window_rect(window: tauri::Window, x: f64, y: f64, width: f64, height: f64) {
    // 移除 WM_SETREDRAW：WebView2 有自己的合成器，禁用/启用重绘会导致白屏闪烁
    // 仅使用 SWP_NOCOPYBITS 避免中间状态被复制绘制
    use winapi::um::winuser::{SetWindowPos, SWP_NOZORDER, SWP_NOACTIVATE, SWP_NOCOPYBITS};

    let manager = window.app_handle().state::<Arc<WindowManager>>();
    manager.set_resizing(true);

    let scale_factor = window.scale_factor().unwrap_or(1.0);
    let hwnd = window.hwnd().unwrap();

    unsafe {
        SetWindowPos(
            hwnd.0 as *mut _,
            std::ptr::null_mut(),
            (x * scale_factor) as i32,
            (y * scale_factor) as i32,
            (width * scale_factor) as i32,
            (height * scale_factor) as i32,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOCOPYBITS,
        );
    }

    manager.update_config_rect(x, y, width, height);
    manager.set_resizing(false);
}

#[tauri::command]
fn get_window_position(window: tauri::Window) -> Result<(f64, f64, f64, f64), String> {
    let pos = window.outer_position().map_err(|e| e.to_string())?;
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let scale_factor = window.scale_factor().unwrap_or(1.0);
    // 返回逻辑像素 (x, y, width, height)
    // 前端使用后端返回的 height 作为基准，避免 window.innerHeight 整数取整
    // 与 outer_size/scale_factor 浮点数之间的舍入误差累积
    Ok((
        pos.x as f64 / scale_factor,
        pos.y as f64 / scale_factor,
        size.width as f64 / scale_factor,
        size.height as f64 / scale_factor,
    ))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::new(WindowManager::new()))
        .manage(Arc::new(TimerManager::new()))
        .manage(Arc::new(ContextMenuManager::new()))
        .manage(Arc::new(SnapLineManager::new()))
        .manage(Arc::new(DesktopAnalyzeManager::new()))
        .manage(Arc::new(DownloadsAnalyzeManager::new()))
        .manage(Arc::new(CleanComputerManager::new()))
        .invoke_handler(tauri::generate_handler![
            greet,
            exit_app,
            toggle_window_lock,
            get_window_config,
            save_window_config,
            set_always_on_top,
            start_dragging,
            stop_dragging,
            set_window_size,
            set_window_rect,
            get_window_position,
            insert_task_cmd,
            get_all_tasks_cmd,
            update_task_cmd,
            delete_task_cmd,
            delete_completed_tasks_cmd,
            delete_all_tasks_cmd,
            move_task_to_trash_cmd,
            get_deleted_tasks_cmd,
            restore_task_cmd,
            permanently_delete_task_cmd,
            clear_trash_by_period_cmd,
            move_completed_to_trash_cmd,
            move_all_to_trash_cmd,
            reinitialize_db_cmd,
            reorder_tasks_cmd,
            reset_app_cmd,
            get_db_window_config_cmd,
            save_db_window_config_cmd,
            get_desktop_path_cmd,
            organize_desktop_cmd,
            check_conflicts_cmd,
            analyze_desktop_cmd,
            show_analyze_window,
            get_desktop_analysis,
            close_desktop_analyze,
            start_countdown_cmd,
            start_scheduled_timer_cmd,
            restore_scheduled_timer_cmd,
            stop_timer_cmd,
            get_timer_status_cmd,
            play_alarm_cmd,
            stop_alarm_cmd,
            calibrate_timer_cmd,
            on_app_ready,
            show_context_menu,
            close_context_menu,
            get_context_menu_task,
            context_menu_action,
            show_trash_context_menu,
            close_trash_context_menu,
            get_trash_context_menu_task,
            trash_context_menu_action,
            collapse_to_snap_line,
            expand_from_snap_line,
            set_main_menu_open,
            is_main_menu_open,
            find_duplicate_files_cmd,
            clean_duplicate_files_cmd,
            get_downloads_path_cmd,
            organize_downloads_cmd,
            analyze_downloads_cmd,
            show_downloads_analyze_window,
            get_downloads_analysis,
            close_downloads_analyze,
            check_downloads_conflicts_cmd,
            find_duplicate_files_for_folder_cmd,
            clean_duplicate_files_for_folder_cmd,
            clean_computer_cmd,
            get_clean_computer_status,
            empty_recycle_bin_cmd,
        ])
        .setup(|app| {
            let manager = app.state::<Arc<WindowManager>>();
            manager.load_from_db();
            
            let mut config = manager.get_config();
            if config.x == 0.0 && config.y == 0.0 {
                if let Ok(Some(primary_monitor)) = app.primary_monitor() {
                    let work_area = primary_monitor.work_area();
                    let center_x = (work_area.size.width as f64 - 300.0) / 2.0 + work_area.position.x as f64;
                    let center_y = (work_area.size.height as f64 - 600.0) / 2.0 + work_area.position.y as f64;
                    config.x = center_x;
                    config.y = center_y;
                    manager.save_config(config);
                    save_db_window_config(center_x, center_y, 600.0, false).ok();
                }
            }
            
            let window = app.get_window("main").unwrap_or_else(|| {
                tauri::WindowBuilder::new(app, "main")
                    .title("每日计划")
                    .inner_size(300.0, 600.0)
                    .decorations(false)
                    .always_on_top(true)
                    .visible(false)
                    .build()
                    .expect("failed to create window")
            });
            
            manager.apply_config_to_window(&window);
            WindowManager::setup_window_events(manager.inner().clone(), &window);

            // 初始化时自动贴边对齐
            manager.init_snap(&window);

            // 主窗口先显示
            window.show().ok();

            let app_handle = app.handle().clone();
            
            std::thread::spawn(move || {
                // 获取可执行文件所在目录（安装后的路径）
                let exe_dir = match std::env::current_exe() {
                    Ok(exe_path) => exe_path.parent().unwrap_or(&exe_path).to_path_buf(),
                    Err(_) => std::env::current_dir().unwrap_or_default(),
                };
                
                // 尝试多个可能的图片路径（安装后在 resources 目录）
                let mut image_path = exe_dir.join("resources").join("welcome.jpg");
                if !image_path.exists() {
                    image_path = exe_dir.join("welcome.jpg");
                }
                if !image_path.exists() {
                    let cwd = std::env::current_dir().unwrap_or_default();
                    image_path = cwd.join("public").join("welcome.jpg");
                }
                if !image_path.exists() {
                    let cwd = std::env::current_dir().unwrap_or_default();
                    image_path = cwd.parent().unwrap_or(&cwd).join("public").join("welcome.jpg");
                }
                if !image_path.exists() {
                    let cwd = std::env::current_dir().unwrap_or_default();
                    image_path = cwd.join("dist").join("welcome.jpg");
                }
                
                let base64_image = match std::fs::read(&image_path) {
                    Ok(data) => base64::engine::general_purpose::STANDARD.encode(&data),
                    Err(_e) => {
                        let _ = setup_context_menu_window(&app_handle);
                        let _ = setup_trash_context_menu_window(&app_handle);
                        let _ = setup_snap_line_window(&app_handle);
                        let _ = setup_desktop_analyze_window(&app_handle);
                        let _ = setup_downloads_analyze_window(&app_handle);
                        return;
                    }
                };
                
                let html_content = format!(
                    r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8" />
<meta name="viewport" content="width=device-width, initial-scale=1.0" />
<title>欢迎</title>
<style>
* {{ margin: 0; padding: 0; box-sizing: border-box; }}
html, body {{ width: 100%; height: 100%; overflow: hidden; }}
.welcome-container {{
  width: 100%;
  height: 100%;
  background-image: url(data:image/jpeg;base64,{});
  background-size: cover;
  background-position: center;
  transition: opacity 0.8s ease-out;
  position: relative;
}}
.welcome-container.fade-out {{ opacity: 0; }}
.progress-section {{
  position: absolute;
  bottom: 40px;
  left: 50%;
  transform: translateX(-50%);
  width: 300px;
}}
.progress-bar-container {{
  width: 100%;
  height: 4px;
  background: rgba(0, 0, 0, 0.3);
  border-radius: 2px;
  overflow: hidden;
}}
.progress-bar {{
  height: 100%;
  background: rgba(255, 255, 255, 0.9);
  border-radius: 2px;
  width: 0%;
  transition: width 0.3s ease;
}}
</style>
</head>
<body>
<div class="welcome-container" id="welcomeContainer">
  <div class="progress-section">
    <div class="progress-bar-container">
      <div class="progress-bar" id="progressBar"></div>
    </div>
  </div>
</div>
<script>
const progressBar = document.getElementById('progressBar');
const welcomeContainer = document.getElementById('welcomeContainer');
window.__TAURI_IPC__ = window.__TAURI_IPC__ || window.__tauri_ipc__;
function updateProgress(percent) {{ progressBar.style.width = percent + '%'; }}
function fadeOut() {{
  welcomeContainer.classList.add('fade-out');
  setTimeout(() => {{ if (window.close) window.close(); }}, 800);
}}
if (window.__TAURI_IPC__) {{
  try {{
    window.__TAURI_IPC__.listen('progress_update', (event) => {{ updateProgress(event.payload.percent); }});
    window.__TAURI_IPC__.listen('app_ready', () => {{ updateProgress(100); setTimeout(fadeOut, 3000); }});
  }} catch (e) {{ setTimeout(fadeOut, 3000); }}
}} else {{ setTimeout(fadeOut, 3000); }}
updateProgress(10);
</script>
</body>
</html>"#,
                    base64_image
                );
                
                let temp_path = dirs::cache_dir().unwrap_or_default().join("daily_plan_welcome.html");
                let _ = std::fs::write(&temp_path, &html_content);
                
                let file_url = format!("file:///{}", temp_path.to_string_lossy().replace('\\', "/"));
                let _ = tauri::WebviewWindowBuilder::new(
                    &app_handle,
                    "welcome",
                    tauri::WebviewUrl::External(file_url.parse().unwrap())
                )
                .title("欢迎")
                .inner_size(800.0, 600.0)
                .center()
                .decorations(false)
                .always_on_top(true)
                .transparent(false)
                .background_color(tauri::window::Color(0, 0, 0, 0))
                .visible(true)
                .build();

                std::thread::sleep(std::time::Duration::from_millis(200));

                let _ = app_handle.emit_to("welcome", "progress_update", serde_json::json!({ "percent": 20 }));

                let _ = setup_context_menu_window(&app_handle);
                let _ = app_handle.emit_to("welcome", "progress_update", serde_json::json!({ "percent": 35 }));

                let _ = setup_trash_context_menu_window(&app_handle);
                let _ = app_handle.emit_to("welcome", "progress_update", serde_json::json!({ "percent": 50 }));

                let _ = setup_snap_line_window(&app_handle);
                let _ = app_handle.emit_to("welcome", "progress_update", serde_json::json!({ "percent": 65 }));

                let _ = setup_desktop_analyze_window(&app_handle);
                let _ = app_handle.emit_to("welcome", "progress_update", serde_json::json!({ "percent": 80 }));

                let _ = setup_downloads_analyze_window(&app_handle);
                let _ = app_handle.emit_to("welcome", "progress_update", serde_json::json!({ "percent": 90 }));

                let mut wait_count = 0;
                while !APP_READY.load(Ordering::SeqCst) && wait_count < 300 {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    wait_count += 1;
                }
                
                if let Some(w) = app_handle.get_webview_window("welcome") {
                    let _ = w.close();
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}