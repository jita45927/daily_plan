mod db;
mod desktop_sort;
mod window;
mod task_timer;
mod context_menu;
mod snap_line;

use std::sync::Arc;
use base64::Engine;

use tauri::Manager;
use db::{delete_all_tasks, delete_completed_tasks, delete_task, get_all_tasks, get_db_window_config, get_deleted_tasks, insert_task, move_task_to_trash, move_completed_to_trash, move_all_to_trash, permanently_delete_task, clear_trash_by_period, reinitialize_db, reorder_tasks, restore_task, save_db_window_config, update_task};
use desktop_sort::{
    get_desktop_path, organize_desktop, ConflictStrategy, DesktopAnalyzeManager,
    analyze_desktop_cmd, show_analyze_window, get_desktop_analysis, close_desktop_analyze,
    setup_desktop_analyze_window,
};
use window::{
    get_window_config, save_window_config, set_always_on_top,
    start_dragging, stop_dragging, toggle_window_lock, collapse_to_snap_line, WindowManager,
};
use task_timer::{
    calibrate_timer_cmd, get_timer_status_cmd, start_countdown_cmd, start_scheduled_timer_cmd,
    stop_timer_cmd, TimerManager,
};
use context_menu::{
    close_context_menu, context_menu_action, get_context_menu_task, setup_context_menu_window,
    show_context_menu, ContextMenuManager,
    setup_trash_context_menu_window, show_trash_context_menu, close_trash_context_menu,
    get_trash_context_menu_task, trash_context_menu_action,
};
use snap_line::{SnapLineManager, setup_snap_line_window, expand_from_snap_line};

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
fn exit_app<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    app.exit(0);
}

#[tauri::command]
fn insert_task_cmd(
    text: &str,
    status: bool,
    color: &str,
    bold: bool,
    timer_type: &str,
    timer_value: i32,
    timer_remaining: i32,
) -> Result<db::Task, String> {
    insert_task(text, status, color, bold, timer_type, timer_value, timer_remaining)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_all_tasks_cmd() -> Result<Vec<db::Task>, String> {
    get_all_tasks().map_err(|e| e.to_string())
}

#[tauri::command]
fn update_task_cmd(
    id: i64,
    text: &str,
    status: bool,
    color: &str,
    bold: bool,
    timer_type: &str,
    timer_value: i32,
    timer_remaining: i32,
) -> Result<db::Task, String> {
    update_task(id, text, status, color, bold, timer_type, timer_value, timer_remaining)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_task_cmd(id: i64) -> Result<bool, String> {
    delete_task(id).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_completed_tasks_cmd() -> Result<i64, String> {
    delete_completed_tasks().map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_all_tasks_cmd() -> Result<i64, String> {
    delete_all_tasks().map_err(|e| e.to_string())
}

#[tauri::command]
fn move_task_to_trash_cmd(task_id: i64) -> Result<bool, String> {
    move_task_to_trash(task_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_deleted_tasks_cmd() -> Result<Vec<db::DeletedTask>, String> {
    get_deleted_tasks().map_err(|e| e.to_string())
}

#[tauri::command]
fn restore_task_cmd(deleted_id: i64) -> Result<bool, String> {
    restore_task(deleted_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn permanently_delete_task_cmd(deleted_id: i64) -> Result<bool, String> {
    permanently_delete_task(deleted_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn clear_trash_by_period_cmd(period_days: i64) -> Result<i64, String> {
    clear_trash_by_period(period_days).map_err(|e| e.to_string())
}

#[tauri::command]
fn move_completed_to_trash_cmd() -> Result<i64, String> {
    move_completed_to_trash().map_err(|e| e.to_string())
}

#[tauri::command]
fn move_all_to_trash_cmd() -> Result<i64, String> {
    move_all_to_trash().map_err(|e| e.to_string())
}

#[tauri::command]
fn reinitialize_db_cmd() -> Result<bool, String> {
    reinitialize_db().map_err(|e| e.to_string())
}

#[tauri::command]
fn reorder_tasks_cmd(task_ids: Vec<i64>, status: bool) -> Result<bool, String> {
    reorder_tasks(task_ids, status).map_err(|e| e.to_string())
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
fn get_db_window_config_cmd() -> Result<db::WindowConfig, String> {
    get_db_window_config().map_err(|e| e.to_string())
}

#[tauri::command]
fn save_db_window_config_cmd(x: f64, y: f64, height: f64, locked: bool) -> Result<db::WindowConfig, String> {
    save_db_window_config(x, y, height, locked).map_err(|e| e.to_string())
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
fn run_organize_desktop() -> Result<bool, String> {
    let desktop_path = get_desktop_path()?;
    
    let cwd = std::env::current_dir().unwrap_or_default();
    let parent_cwd = cwd.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    
    let possible_paths = [
        cwd.join("text_Clean_up the_desktop").join("organize_desktop.py"),
        parent_cwd.join("text_Clean_up the_desktop").join("organize_desktop.py"),
        std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.join("organize_desktop.py"))).unwrap_or_default(),
        std::env::current_exe().ok().and_then(|p| p.parent().and_then(|p| p.parent()).map(|p| p.join("organize_desktop.py"))).unwrap_or_default(),
        std::env::current_exe().ok().and_then(|p| p.parent().and_then(|p| p.parent()).and_then(|p| p.parent()).map(|p| p.join("organize_desktop.py"))).unwrap_or_default(),
    ];
    
    let mut script_path = None;
    for path in &possible_paths {
        if path.exists() {
            script_path = Some(path.clone());
            break;
        }
    }
    
    let script_path = match script_path {
        Some(p) => p,
        None => {
            let paths_str: Vec<String> = possible_paths.iter().map(|p| p.display().to_string()).collect();
            return Err(format!("整理脚本不存在！\n搜索路径:\n{}", paths_str.join("\n")));
        }
    };
    
    println!("整理脚本路径: {}", script_path.display());
    println!("桌面路径: {}", desktop_path);
    
    let result = std::process::Command::new("python")
        .arg(&script_path)
        .arg(&desktop_path)
        .spawn();
    
    match result {
        Ok(_) => {
            println!("整理脚本启动成功");
            Ok(true)
        }
        Err(e) => {
            println!("启动整理脚本失败: {}", e);
            let python_path = match std::env::var("PYTHONPATH") {
                Ok(p) => p,
                Err(_) => "未设置".to_string()
            };
            Err(format!("启动整理脚本失败: {}\n可能需要安装Python并配置环境变量\nPYTHONPATH: {}", e, python_path))
        }
    }
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
        .manage(Arc::new(WindowManager::new()))
        .manage(Arc::new(TimerManager::new()))
        .manage(Arc::new(ContextMenuManager::new()))
        .manage(Arc::new(SnapLineManager::new()))
        .manage(Arc::new(DesktopAnalyzeManager::new()))
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
            run_organize_desktop,
            analyze_desktop_cmd,
            show_analyze_window,
            get_desktop_analysis,
            close_desktop_analyze,
            start_countdown_cmd,
            start_scheduled_timer_cmd,
            stop_timer_cmd,
            get_timer_status_cmd,
            calibrate_timer_cmd,
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
                std::thread::sleep(std::time::Duration::from_millis(1000));
                
                let cwd = std::env::current_dir().unwrap_or_default();
                
                let mut image_path = cwd.join("public").join("welcome.jpg");
                if !image_path.exists() {
                    image_path = cwd.parent().unwrap_or(&cwd).join("public").join("welcome.jpg");
                }
                if !image_path.exists() {
                    image_path = cwd.join("dist").join("welcome.jpg");
                }
                if !image_path.exists() {
                    image_path = cwd.parent().unwrap_or(&cwd).join("dist").join("welcome.jpg");
                }
                
                let base64_image = match std::fs::read(&image_path) {
                    Ok(data) => base64::engine::general_purpose::STANDARD.encode(&data),
                    Err(_e) => {
                        let _ = setup_context_menu_window(&app_handle);
                        let _ = setup_trash_context_menu_window(&app_handle);
                        let _ = setup_snap_line_window(&app_handle);
                        let _ = setup_desktop_analyze_window(&app_handle);
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
html, body {{ width: 100%; height: 100%; overflow: hidden; background: transparent; }}
img {{ width: 100%; height: 100%; display: block; object-fit: cover; }}
</style>
</head>
<body>
<img src="data:image/jpeg;base64,{}" alt="Welcome" />
<script>setTimeout(function() {{ window.close(); }}, 4000);</script>
</body>
</html>"#,
                    base64_image
                );
                
                let temp_path = dirs::cache_dir().unwrap_or_default().join("daily_plan_welcome.html");
                let _ = std::fs::write(&temp_path, &html_content);
                
                let file_url = format!("file:///{}", temp_path.to_string_lossy().replace('\\', "/"));
                let welcome_window = match tauri::WebviewWindowBuilder::new(
                    &app_handle,
                    "welcome",
                    tauri::WebviewUrl::External(file_url.parse().unwrap())
                )
                .title("欢迎")
                .inner_size(800.0, 600.0)
                .center()
                .decorations(false)
                .always_on_top(true)
                .transparent(true)
                .background_color(tauri::window::Color(0, 0, 0, 0))
                .visible(true)
                .build() {
                    Ok(w) => w,
                    Err(_e) => {
                        let _ = setup_context_menu_window(&app_handle);
                        let _ = setup_trash_context_menu_window(&app_handle);
                        let _ = setup_snap_line_window(&app_handle);
                        let _ = setup_desktop_analyze_window(&app_handle);
                        return;
                    }
                };

                let welcome_clone = welcome_window.clone();
                let app_handle2 = app_handle.clone();

                std::thread::sleep(std::time::Duration::from_millis(4000));
                let _ = welcome_clone.close();

                std::thread::sleep(std::time::Duration::from_millis(800));
                let _ = setup_context_menu_window(&app_handle2);
                let _ = setup_trash_context_menu_window(&app_handle2);
                let _ = setup_snap_line_window(&app_handle2);
                let _ = setup_desktop_analyze_window(&app_handle2);
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}