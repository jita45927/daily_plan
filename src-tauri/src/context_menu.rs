use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, Runtime, Window, WebviewUrl};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextMenuTask {
    pub id: i64,
    pub text: String,
    pub status: bool,
    pub bold: bool,
    pub color: String,
    pub timer_type: String,
    pub timer_remaining: i32,
}

pub struct ContextMenuManager {
    current_task: Mutex<Option<ContextMenuTask>>,
    current_trash_task_id: Mutex<Option<i64>>,
}

impl ContextMenuManager {
    pub fn new() -> Self {
        Self {
            current_task: Mutex::new(None),
            current_trash_task_id: Mutex::new(None),
        }
    }

    pub fn set_current_task(&self, task: ContextMenuTask) {
        *self.current_task.lock().unwrap() = Some(task);
    }

    pub fn get_current_task(&self) -> Option<ContextMenuTask> {
        self.current_task.lock().unwrap().clone()
    }

    pub fn set_current_trash_task_id(&self, task_id: i64) {
        *self.current_trash_task_id.lock().unwrap() = Some(task_id);
    }

    pub fn get_current_trash_task_id(&self) -> Option<i64> {
        *self.current_trash_task_id.lock().unwrap()
    }
}

impl Default for ContextMenuManager {
    fn default() -> Self {
        Self::new()
    }
}

// 精确尺寸常量（与前端 CSS 完全对应）
const MENU_WIDTH: f64 = 160.0;
const BTN_HEIGHT: f64 = 25.0;      // padding(6+6) + line-height(11px * ~1.2 ≈ 13)
const DIVIDER_HEIGHT: f64 = 9.0;   // 1px + margin(4+4)
const SECTION_TITLE_HEIGHT: f64 = 18.0; // padding(4+2) + line-height(10px * ~1.2 ≈ 12)
const CONTAINER_PADDING: f64 = 8.0; // padding top+bottom (4+4)

/// 根据任务状态精确计算主右键菜单高度
fn calc_main_menu_height(status: bool) -> f64 {
    let mut height = CONTAINER_PADDING + 4.0; // 额外4px余量

    if !status {
        // 未完成任务: 12个按钮 + 3个分隔线
        // 标记已完成, 标记未完成, 删除任务, 恢复任务, | 限时任务, 定时任务, 取消定时/限时, | 加粗文字, 文字改色, 默认样式, | 删除已完成任务, 删除所有任务
        height += BTN_HEIGHT * 12.0;
        height += DIVIDER_HEIGHT * 3.0;
    } else {
        // 已完成任务: 9个按钮 + 3个分隔线（限时任务、定时任务、文字改色不显示）
        // 标记已完成, 标记未完成, 删除任务, 恢复任务, | 取消定时/限时, | 加粗文字, 默认样式, | 删除已完成任务, 删除所有任务
        height += BTN_HEIGHT * 9.0;
        height += DIVIDER_HEIGHT * 3.0;
    }

    height
}

/// 回收站右键菜单高度（固定）
fn calc_trash_menu_height() -> f64 {
    // 彻底清除该任务, 恢复该任务, | [清理回收站标题], 一周前, 两周前, 一个月前, 清理全部
    CONTAINER_PADDING + BTN_HEIGHT * 2.0 + DIVIDER_HEIGHT + SECTION_TITLE_HEIGHT + BTN_HEIGHT * 4.0 + 4.0
}

/// 计算菜单位置（避免超出屏幕边界）
fn calculate_menu_position<R: Runtime>(
    window: &Window<R>,
    screen_x: f64,
    screen_y: f64,
    menu_width: f64,
    menu_height: f64,
) -> Option<(i32, i32)> {
    let monitor = match window.current_monitor() {
        Ok(Some(m)) => m,
        _ => return None,
    };

    let scale = monitor.scale_factor();
    let work_area = monitor.work_area();
    let wa_pos = work_area.position;
    let wa_size = work_area.size;

    let primary_scale = window.primary_monitor()
        .ok()
        .flatten()
        .map(|m| m.scale_factor())
        .unwrap_or(1.0);

    let css_offset_x = wa_pos.x as f64 / primary_scale;
    let css_offset_y = wa_pos.y as f64 / primary_scale;

    let phys_x = wa_pos.x as f64 + (screen_x - css_offset_x) * scale;
    let phys_y = wa_pos.y as f64 + (screen_y - css_offset_y) * scale;
    let phys_width = menu_width * scale;
    let phys_height = menu_height * scale;

    let window_pos = window.outer_position();
    let window_center_x = match window_pos {
        Ok(pos) => pos.x as f64 + 150.0 * scale,
        Err(_) => phys_x,
    };
    let screen_center_x = wa_pos.x as f64 + wa_size.width as f64 / 2.0;

    let mut final_x: f64;
    let mut final_y = phys_y;

    if window_center_x < screen_center_x {
        final_x = phys_x - phys_width;
    } else {
        final_x = phys_x;
    }

    if final_x + phys_width > (wa_pos.x + wa_size.width as i32) as f64 {
        final_x = (wa_pos.x + wa_size.width as i32) as f64 - phys_width;
    }
    if final_x < wa_pos.x as f64 {
        final_x = wa_pos.x as f64;
    }

    if phys_y + phys_height > (wa_pos.y + wa_size.height as i32) as f64 {
        final_y = (wa_pos.y + wa_size.height as i32) as f64 - phys_height;
    }
    if final_y < wa_pos.y as f64 {
        final_y = wa_pos.y as f64;
    }

    Some((final_x as i32, final_y as i32))
}

pub fn setup_context_menu_window<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), String> {
    let menu_win = tauri::WebviewWindowBuilder::new(
        app,
        "context_menu",
        WebviewUrl::App("/context-menu.html".into()),
    )
    .title("右键菜单")
    .inner_size(MENU_WIDTH, 310.0)
    .decorations(false)
    .transparent(false)
    .always_on_top(true)
    .visible(false)
    .skip_taskbar(true)
    .resizable(false)
    .position(-3000.0, -3000.0)
    .build()
    .map_err(|e| format!("failed to create context menu window: {:?}", e))?;

    let win_clone = menu_win.clone();
    menu_win.on_window_event(move |event| {
        if let tauri::WindowEvent::Focused(false) = event {
            let _ = win_clone.hide();
        }
    });

    Ok(())
}

#[tauri::command]
pub fn show_context_menu<R: Runtime>(
    window: Window<R>,
    screen_x: f64,
    screen_y: f64,
    task: ContextMenuTask,
) {
    let app = window.app_handle();
    let manager = app.state::<Arc<ContextMenuManager>>();
    manager.set_current_task(task.clone());

    let menu_width = MENU_WIDTH;
    let menu_height = calc_main_menu_height(task.status);

    let (final_x, final_y) = match calculate_menu_position(&window, screen_x, screen_y, menu_width, menu_height) {
        Some(pos) => pos,
        None => return,
    };

    let scale = match window.current_monitor() {
        Ok(Some(m)) => m.scale_factor(),
        _ => return,
    };
    let phys_width = (menu_width * scale) as u32;
    let phys_height = (menu_height * scale) as u32;

    if let Some(menu_win) = app.get_webview_window("context_menu") {
        let _ = menu_win.set_size(tauri::PhysicalSize::new(phys_width, phys_height));
        let _ = menu_win.set_position(tauri::PhysicalPosition::new(final_x, final_y));
        let _ = menu_win.set_always_on_top(true);
        let _ = menu_win.show();
        let _ = menu_win.set_focus();
        let _ = menu_win.emit("context-menu-reload", ());
    }
}

#[tauri::command]
pub fn close_context_menu<R: Runtime>(window: Window<R>) {
    if let Some(win) = window.app_handle().get_webview_window("context_menu") {
        let _ = win.hide();
    }
}

#[tauri::command]
pub fn get_context_menu_task<R: Runtime>(
    window: Window<R>,
) -> Option<ContextMenuTask> {
    let manager = window.app_handle().state::<Arc<ContextMenuManager>>();
    manager.get_current_task()
}

#[tauri::command]
pub fn context_menu_action<R: Runtime>(
    window: Window<R>,
    action: String,
    task_id: i64,
    value: Option<String>,
) {
    let app = window.app_handle();
    let _ = app.emit(
        "context_menu_command",
        serde_json::json!({
            "action": action,
            "taskId": task_id,
            "value": value,
        }),
    );
    // 发出事件后立即隐藏窗口，避免竞态条件
    let _ = window.hide();
}

pub fn setup_trash_context_menu_window<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), String> {
    let menu_win = tauri::WebviewWindowBuilder::new(
        app,
        "trash_context_menu",
        WebviewUrl::App("/trash-context-menu.html".into()),
    )
    .title("回收站右键菜单")
    .inner_size(MENU_WIDTH, 185.0)
    .decorations(false)
    .transparent(false)
    .always_on_top(true)
    .visible(false)
    .skip_taskbar(true)
    .resizable(false)
    .position(-3000.0, -3000.0)
    .build()
    .map_err(|e| format!("failed to create trash context menu window: {:?}", e))?;

    let win_clone = menu_win.clone();
    menu_win.on_window_event(move |event| {
        if let tauri::WindowEvent::Focused(false) = event {
            let _ = win_clone.hide();
        }
    });

    Ok(())
}

#[tauri::command]
pub fn show_trash_context_menu<R: Runtime>(
    window: Window<R>,
    screen_x: f64,
    screen_y: f64,
    task_id: i64,
) {
    let app = window.app_handle();
    let manager = app.state::<Arc<ContextMenuManager>>();
    manager.set_current_trash_task_id(task_id);

    let menu_width = MENU_WIDTH;
    let menu_height = calc_trash_menu_height();

    let (final_x, final_y) = match calculate_menu_position(&window, screen_x, screen_y, menu_width, menu_height) {
        Some(pos) => pos,
        None => return,
    };

    let scale = match window.current_monitor() {
        Ok(Some(m)) => m.scale_factor(),
        _ => return,
    };
    let phys_width = (menu_width * scale) as u32;
    let phys_height = (menu_height * scale) as u32;

    if let Some(menu_win) = app.get_webview_window("trash_context_menu") {
        let _ = menu_win.set_size(tauri::PhysicalSize::new(phys_width, phys_height));
        let _ = menu_win.set_position(tauri::PhysicalPosition::new(final_x, final_y));
        let _ = menu_win.set_always_on_top(true);
        let _ = menu_win.show();
        let _ = menu_win.set_focus();
        let _ = menu_win.emit("trash-context-menu-reload", task_id);
    }
}

#[tauri::command]
pub fn close_trash_context_menu<R: Runtime>(window: Window<R>) {
    if let Some(win) = window.app_handle().get_webview_window("trash_context_menu") {
        let _ = win.hide();
    }
}

#[tauri::command]
pub fn get_trash_context_menu_task<R: Runtime>(
    window: Window<R>,
) -> Option<i64> {
    let manager = window.app_handle().state::<Arc<ContextMenuManager>>();
    manager.get_current_trash_task_id()
}

#[tauri::command]
pub fn trash_context_menu_action<R: Runtime>(
    window: Window<R>,
    action: String,
    task_id: i64,
) {
    let app = window.app_handle();
    let manager = app.state::<Arc<ContextMenuManager>>();

    // 对于单任务操作，从 manager 获取已存储的 task ID（更可靠）
    let effective_task_id = if task_id == 0 {
        manager.get_current_trash_task_id().unwrap_or(0)
    } else {
        task_id
    };

    let _ = app.emit(
        "trash_context_menu_command",
        serde_json::json!({
            "action": action,
            "taskId": effective_task_id,
        }),
    );
    // 发出事件后立即隐藏窗口，避免竞态条件
    let _ = window.hide();
}
