mod db;
mod desktop_sort;
mod window;
mod task_timer;
mod context_menu;
mod snap_line;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Manager;
use db::{delete_all_tasks, delete_completed_tasks, delete_task, get_all_tasks, get_db_window_config, get_deleted_tasks, insert_task, move_task_to_trash, move_completed_to_trash, move_all_to_trash, permanently_delete_task, clear_trash_by_period, reinitialize_db, reorder_tasks, restore_task, save_db_window_config, update_task};
use desktop_sort::{get_desktop_path, organize_desktop, ConflictStrategy};
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
fn organize_desktop_cmd(strategy: ConflictStrategy) -> Result<(usize, usize, Vec<String>), String> {
    organize_desktop(strategy)
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

            // 主窗口先显示（带动 WebView2 运行时初始化 + 加载主程序），然后立即创建欢迎窗口盖在上面
            // 两者并行加载，欢迎窗口只显示图片（更轻量），主程序在后台初始化
            // 欢迎窗口 always_on_top 保证在主窗口上层
            window.show().ok();

            let app_handle = app.handle().clone();
            let _main_window = window.clone();
            std::thread::spawn(move || {
                // 用 AtomicBool 标记页面加载完成，确保倒计时从页面就绪后开始
                let page_loaded = Arc::new(AtomicBool::new(false));
                let page_loaded_for_cb = page_loaded.clone();

                let welcome_window = match tauri::WebviewWindowBuilder::new(
                    &app_handle,
                    "welcome",
                    tauri::WebviewUrl::App("/welcome.html".into())
                )
                .title("欢迎")
                .inner_size(800.0, 600.0)
                .center()
                .decorations(false)
                .always_on_top(true)
                .transparent(true)
                .background_color(tauri::window::Color(0, 0, 0, 0))
                .visible(true)
                .on_page_load(move |_webview, payload| {
                    if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                        page_loaded_for_cb.store(true, Ordering::SeqCst);
                    }
                })
                .build() {
                    Ok(w) => w,
                    Err(_e) => {
                        std::thread::sleep(std::time::Duration::from_millis(800));
                        let _ = setup_context_menu_window(&app_handle);
                        let _ = setup_trash_context_menu_window(&app_handle);
                        let _ = setup_snap_line_window(&app_handle);
                        return;
                    }
                };

                let welcome_clone = welcome_window.clone();
                let app_handle2 = app_handle.clone();
                let page_loaded_for_thread = page_loaded.clone();
                std::thread::spawn(move || {
                    let start = std::time::Instant::now();
                    while !page_loaded_for_thread.load(Ordering::SeqCst) {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        if start.elapsed() > std::time::Duration::from_secs(6) {
                            break;
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(4000));

                    let _ = welcome_clone.close();

                    std::thread::sleep(std::time::Duration::from_millis(800));
                    let _ = setup_context_menu_window(&app_handle2);
                    let _ = setup_trash_context_menu_window(&app_handle2);
                    let _ = setup_snap_line_window(&app_handle2);
                });
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}