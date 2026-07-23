mod db;
mod desktop_sort;
mod window;
mod task_timer;
mod context_menu;
mod snap_line;
mod clean_computer;
mod recycle_bin;

use std::sync::Arc;

use tauri::Manager;
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
    set_main_menu_open, is_main_menu_open, set_sub_window_open,
    set_resizing, reset_snap_state_cmd, WindowManager,
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
fn on_app_ready(_app_handle: tauri::AppHandle) -> Result<String, String> {
    APP_READY.store(true, Ordering::SeqCst);
    Ok("主窗口加载完成".to_string())
}

#[tauri::command]
fn close_welcome_window(app_handle: tauri::AppHandle) -> Result<String, String> {
    if let Some(w) = app_handle.get_webview_window("welcome") {
        let _ = w.close();
    }
    Ok("欢迎窗口已关闭".to_string())
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
    println!("[重置程序] 开始重置...");
    
    reinitialize_db()?;
    
    println!("[重置程序] 数据库已重置");

    let manager = window.app_handle().state::<Arc<WindowManager>>();
    
    // 先重置贴边状态（确保窗口展开）
    manager.reset_snap_state(&window);
    
    // 设置窗口为默认大小（300x600）
    let default_width = 300.0;
    let default_height = 600.0;
    let scale_factor = window.scale_factor().unwrap_or(1.0);
    
    // 使用 Win32 API 设置窗口大小
    use winapi::um::winuser::{SetWindowPos, SWP_NOZORDER, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOCOPYBITS};
    let hwnd = window.hwnd().unwrap();
    unsafe {
        SetWindowPos(
            hwnd.0 as *mut _,
            std::ptr::null_mut(),
            0, 0,
            (default_width * scale_factor) as i32,
            (default_height * scale_factor) as i32,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOCOPYBITS,
        );
    }
    
    // 将窗口移到主屏幕正中央（使用物理坐标）
    manager.move_to_primary_monitor_center(&window)?;
    
    // 更新配置为默认大小
    let mut default_config = window::WindowConfigData::default();
    default_config.width = default_width;
    default_config.height = default_height;
    manager.save_config(default_config);
    
    // 保存到数据库
    save_db_window_config(0.0, 0.0, default_height, false)
        .map_err(|e| e.to_string())?;
    
    // 确保窗口可见并在顶层
    let _ = window.show();
    let _ = window.set_always_on_top(true);
    let _ = window.set_focus();

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
}

#[tauri::command]
fn set_window_rect(window: tauri::Window, x: f64, y: f64, width: f64, height: f64) {
    // SWP_NOSENDCHANGING = 0x0400，阻止发送 WM_WINDOWPOSCHANGING 消息，减少事件触发
    // 避免调整大小时频繁触发 Moved/Resized 事件导致抖动
    use winapi::um::winuser::{SetWindowPos, SWP_NOZORDER, SWP_NOACTIVATE, SWP_NOCOPYBITS};
    const SWP_NOSENDCHANGING: u32 = 0x0400;

    let manager = window.app_handle().state::<Arc<WindowManager>>();

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
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOCOPYBITS | SWP_NOSENDCHANGING,
        );
    }

    manager.update_config_rect(x, y, width, height);
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
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // 单实例保护：当用户尝试启动第二个实例时，不创建新窗口，
            // 而是展开（若处于贴边收起状态）并聚焦到已存在的主窗口
            if let Some(manager) = app.try_state::<Arc<WindowManager>>() {
                // 若主窗口处于贴边收起状态，先展开恢复
                manager.expand_window(app);
            }
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
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
            set_sub_window_open,
            set_resizing,
            reset_snap_state_cmd,
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
            close_welcome_window,
        ])
        .setup(|app| {
            let manager = app.state::<Arc<WindowManager>>();
            manager.load_from_db();
            
            let mut config = manager.get_config();
            
            // 强制重置位置和尺寸
            config.width = 300.0;
            config.height = 600.0;
            config.is_locked = false;
            manager.save_config(config);
            
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
            
            // 将窗口移到主屏幕正中央（使用物理坐标，确保在不同分辨率和多屏幕环境下准确）
            if let Ok(()) = manager.move_to_primary_monitor_center(&window) {
                let primary_monitor = app.primary_monitor();
                if let Ok(Some(monitor)) = primary_monitor {
                    let work_area = monitor.work_area();
                    let cx = (work_area.size.width as f64 - 300.0) / 2.0 + work_area.position.x as f64;
                    let cy = (work_area.size.height as f64 - 600.0) / 2.0 + work_area.position.y as f64;
                    save_db_window_config(cx, cy, 600.0, false).ok();
                }
            }
            
            WindowManager::setup_window_events(manager.inner().clone(), &window);

            // 初始化时自动贴边对齐
            manager.init_snap(&window);

            let main_window = window.clone();
            let app_handle = app.handle().clone();
            
            std::thread::spawn(move || {
                // 创建日志文件
                let log_path = std::env::temp_dir().join("daily_plan_welcome.log");
                let _ = std::fs::write(&log_path, "");
                
                let log = |msg: &str| {
                    let _ = std::fs::write(&log_path, format!("{}\n{}", 
                        std::fs::read_to_string(&log_path).unwrap_or_default(), msg));
                };
                
                log("[欢迎画面] 开始创建欢迎窗口");
                
                // 使用 WebviewUrl::App 加载欢迎页面（图片已在构建时嵌入 HTML）
                let _ = tauri::WebviewWindowBuilder::new(
                    &app_handle,
                    "welcome",
                    tauri::WebviewUrl::App("welcome.html".into())
                )
                .title("欢迎")
                .inner_size(800.0, 600.0)
                .center()
                .decorations(false)
                .always_on_top(true)
                .transparent(false)
                .background_color(tauri::window::Color(255, 255, 255, 255))
                .visible(true)
                .build();
                
                log("[欢迎画面] 欢迎窗口创建完成");
                
                // 确保欢迎窗口始终在最顶层
                let ensure_welcome_on_top = || {
                    if let Some(w) = app_handle.get_webview_window("welcome") {
                        let _ = w.set_always_on_top(true);
                        let _ = w.set_focus();
                    }
                };
                ensure_welcome_on_top();

                // 等待页面加载
                std::thread::sleep(std::time::Duration::from_millis(500));
                log("[欢迎画面] 页面加载等待完成");

                // 使用 eval 直接执行 JS 更新进度条
                if let Some(w) = app_handle.get_webview_window("welcome") {
                    let _ = w.eval("document.getElementById('progressBar').style.width='20%'");
                }
                log("[欢迎画面] 进度条更新到 20%");

                let _ = setup_context_menu_window(&app_handle);
                if let Some(w) = app_handle.get_webview_window("welcome") {
                    let _ = w.eval("document.getElementById('progressBar').style.width='35%'");
                }

                let _ = setup_trash_context_menu_window(&app_handle);
                if let Some(w) = app_handle.get_webview_window("welcome") {
                    let _ = w.eval("document.getElementById('progressBar').style.width='50%'");
                }

                let _ = setup_snap_line_window(&app_handle);
                if let Some(w) = app_handle.get_webview_window("welcome") {
                    let _ = w.eval("document.getElementById('progressBar').style.width='65%'");
                }

                let _ = setup_desktop_analyze_window(&app_handle);
                if let Some(w) = app_handle.get_webview_window("welcome") {
                    let _ = w.eval("document.getElementById('progressBar').style.width='80%'");
                }

                let _ = setup_downloads_analyze_window(&app_handle);
                if let Some(w) = app_handle.get_webview_window("welcome") {
                    let _ = w.eval("document.getElementById('progressBar').style.width='90%'");
                }

                // 等待主窗口加载完成（APP_READY 由前端 on_app_ready 命令设置）
                log("[欢迎画面] 等待主窗口加载完成");
                let mut wait_count = 0;
                while !APP_READY.load(Ordering::SeqCst) && wait_count < 300 {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    wait_count += 1;
                }
                log(&format!("[欢迎画面] 主窗口加载完成 (等待次数: {})", wait_count));
                
                // 等待 500ms 让浏览器完成绘制（避免显示黑色空白窗口）
                std::thread::sleep(std::time::Duration::from_millis(500));
                log("[欢迎画面] 等待浏览器绘制完成");
                
                // 在显示主窗口之前，将主窗口的 always_on_top 设置为 false
                // 确保欢迎窗口始终在最上层，不会被主窗口遮挡
                main_window.set_always_on_top(false).ok();
                
                // 主窗口已加载完成，显示主窗口（此时 Vue 已经渲染完成）
                main_window.show().ok();
                log("[欢迎画面] 主窗口已显示");
                
                // 再次确保欢迎窗口在最顶层
                ensure_welcome_on_top();
                
                // 进度条到 100%
                if let Some(w) = app_handle.get_webview_window("welcome") {
                    let _ = w.eval("document.getElementById('progressBar').style.width='100%'");
                }
                log("[欢迎画面] 进度条更新到 100%");
                
                // 等待 3 秒（进度条维持时间）
                std::thread::sleep(std::time::Duration::from_millis(3000));
                log("[欢迎画面] 3秒等待完成，开始销毁窗口");
                
                // 使用 destroy 立即销毁窗口，避免任何视觉残留
                if let Some(w) = app_handle.get_webview_window("welcome") {
                    let _ = w.destroy();
                }
                log("[欢迎画面] 欢迎窗口已销毁");
                
                // 欢迎窗口销毁后，恢复主窗口的 always_on_top 设置
                main_window.set_always_on_top(true).ok();
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}