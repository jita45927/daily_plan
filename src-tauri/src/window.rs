use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::fs::OpenOptions;
use std::io::Write;

use serde::{Deserialize, Serialize};
use tauri::{
    Manager, PhysicalPosition, PhysicalSize, Runtime, Window, Emitter,
};
use tokio::time::sleep;
use crate::context_menu::{close_context_menu, close_trash_context_menu};
use crate::db;
use crate::snap_line::set_window_exact_region_with_offset;

/// 调试日志函数：将日志写入文件，因为 Tauri GUI 应用的 stderr 不会输出到控制台
fn debug_log(msg: &str) {
    let exe_path = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return,
    };
    let log_path = exe_path.parent()
        .map(|p| p.join("debug.log"))
        .unwrap_or_else(|| std::path::PathBuf::from("debug.log"));
    
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let _ = writeln!(file, "[{}] {}", timestamp, msg);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowConfigData {
    pub is_locked: bool,
    pub width: f64,
    pub height: f64,
    pub x: f64,
    pub y: f64,
    pub always_on_top: bool,
}

impl Default for WindowConfigData {
    fn default() -> Self {
        Self {
            is_locked: false,
            width: 300.0,
            height: 600.0,
            x: 0.0,
            y: 0.0,
            always_on_top: true,
        }
    }
}

#[derive(Debug, Clone)]
struct ScreenEdge {
    edge_type: EdgeType,
    position: i32,
    is_shared: bool,
    /// 边缘所属的屏幕索引（用于双屏幕时区分边缘归属）
    monitor_index: usize,
}

#[derive(Debug, Clone, PartialEq)]
enum EdgeType {
    Left,
    Right,
}

/// 贴边线所在边（用于收起后显示的黄线位置）
#[derive(Debug, Clone, PartialEq)]
enum LineEdge {
    Top(Option<usize>),
    Left(Option<usize>),
    Right(Option<usize>),
}

fn is_window_visible<R: Runtime>(app_handle: &tauri::AppHandle<R>, window_name: &str) -> bool {
    app_handle
        .get_webview_window(window_name)
        .map(|w| w.is_visible().unwrap_or(false))
        .unwrap_or(false)
}

pub struct WindowManager {
    config: Mutex<WindowConfigData>,
    drag_threshold: i32,
    is_dragging: Mutex<bool>,
    is_resizing: Mutex<bool>,
    drag_debounce: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    /// 当前贴边线所在边（None 表示未贴边）
    snap_line_edge: Mutex<Option<LineEdge>>,
    /// 窗口是否已收起（显示贴边线，主窗口移到屏幕外）
    is_collapsed: Mutex<bool>,
    /// 鼠标是否曾经进入过窗口区域（避免初始化时鼠标不在窗口上就立即收起）
    mouse_was_in_window: Mutex<bool>,
    /// 收起前的窗口外框位置（用于展开时恢复）
    collapsed_position: Mutex<Option<tauri::PhysicalPosition<i32>>>,
    /// 左键菜单是否打开（打开时禁用收起）
    is_main_menu_open: Mutex<bool>,
    /// 窗口是否有焦点（用于检测窗口被最小化的情况）
    is_focused: Mutex<bool>,
    /// 上次失去焦点的时间（用于防抖，区分鼠标离开和系统最小化）
    last_focus_loss_time: Mutex<Option<Instant>>,
    /// 是否有子窗口/对话框/菜单打开（打开时禁用收起）
    is_sub_window_open: Mutex<bool>,
    /// 是否跳过下次贴边计算（用于用户主动解除贴边后避免立即重新贴边）
    skip_next_snap: Mutex<bool>,
}

impl WindowManager {
    pub fn new() -> Self {
        Self {
            config: Mutex::new(WindowConfigData::default()),
            drag_threshold: 40,
            is_dragging: Mutex::new(false),
            is_resizing: Mutex::new(false),
            drag_debounce: Mutex::new(None),
            snap_line_edge: Mutex::new(None),
            is_collapsed: Mutex::new(false),
            mouse_was_in_window: Mutex::new(false),
            collapsed_position: Mutex::new(None),
            is_main_menu_open: Mutex::new(false),
            is_focused: Mutex::new(true),
            last_focus_loss_time: Mutex::new(None),
            is_sub_window_open: Mutex::new(false),
            skip_next_snap: Mutex::new(false),
        }
    }

    pub fn setup_window_events<R: Runtime>(manager: Arc<WindowManager>, window: &Window<R>) {
        let weak_window = window.to_owned();

        // 启动全局轮询：检测鼠标是否离开窗口区域，触发自动收起
        // 不依赖前端 mouseleave（拖拽时不触发、初始化时鼠标不在窗口上也不触发）
        Self::start_collapse_watcher(Arc::clone(&manager), window.to_owned());

        window.on_window_event(move |event| {
            match event {
                tauri::WindowEvent::Resized(size) => {
                    manager.handle_resize(&weak_window, size);
                }
                tauri::WindowEvent::Moved(position) => {
                    manager.handle_move(&weak_window, position);

                    // 拖拽结束检测：使用防抖定时器
                    // 原生 start_dragging 会接管鼠标事件，前端 mouseup 不会触发
                    // 所以用 Moved 事件 + 防抖来检测拖拽结束
                    // 注意：正在调整大小时不执行贴边逻辑
                    if *manager.is_dragging.lock().unwrap() && !*manager.is_resizing.lock().unwrap() {
                        // 取消之前的防抖定时器
                        if let Some(handle) = manager.drag_debounce.lock().unwrap().take() {
                            handle.abort();
                        }

                        let win = weak_window.clone();
                        let mgr = Arc::clone(&manager);

                        // 启动新的防抖定时器：
                        // 100ms 无 Moved → 执行贴边
                        // 300ms 无 Moved → 认为拖拽结束，清除 is_dragging
                        let handle = tauri::async_runtime::spawn(async move {
                            // 第一阶段：100ms 后执行贴边对齐
                            sleep(Duration::from_millis(100)).await;

                            // 如果已停止拖拽或正在调整大小，返回
                            if !*mgr.is_dragging.lock().unwrap() || *mgr.is_resizing.lock().unwrap() {
                                return;
                            }

                            mgr.perform_snap(&win);
                            mgr.save_to_db();

                            // 第二阶段：再等 200ms（共 300ms），如果仍无新移动，认为拖拽结束
                            sleep(Duration::from_millis(200)).await;

                            // 如果已停止拖拽或正在调整大小，返回
                            if !*mgr.is_dragging.lock().unwrap() || *mgr.is_resizing.lock().unwrap() {
                                return;
                            }

                            // 300ms 无新的 Moved 事件 → 拖拽结束
                            *mgr.is_dragging.lock().unwrap() = false;
                            *mgr.mouse_was_in_window.lock().unwrap() = true;
                        });

                        *manager.drag_debounce.lock().unwrap() = Some(handle);
                    }
                }
                tauri::WindowEvent::CloseRequested { .. } => {
                    manager.save_to_db();
                }
                tauri::WindowEvent::Destroyed => {
                    let app = weak_window.app_handle();
                    app.exit(0);
                }
                tauri::WindowEvent::Focused(focused) => {
                    *manager.is_focused.lock().unwrap() = *focused;
                    if *focused {
                        // 聚焦时清除失焦时间戳
                        *manager.last_focus_loss_time.lock().unwrap() = None;
                        manager.handle_window_focused(&weak_window);
                    } else {
                        // 失焦时记录时间戳（用于防抖）
                        *manager.last_focus_loss_time.lock().unwrap() = Some(Instant::now());
                    }
                }
                tauri::WindowEvent::ScaleFactorChanged { .. } => {
                    manager.handle_monitor_change(&weak_window);
                }
                _ => {}
            }
        });
    }

    /// 启动全局鼠标轮询，检测鼠标离开窗口后自动收起
    fn start_collapse_watcher<R: Runtime>(manager: Arc<WindowManager>, window: Window<R>) {
        tauri::async_runtime::spawn(async move {
            // 启动后等待 2 秒，确保窗口初始化和首次贴边完成
            sleep(Duration::from_millis(2000)).await;

            loop {
                sleep(Duration::from_millis(150)).await;

                // 检查是否需要检测收起
                let should_check = {
                    let collapsed = *manager.is_collapsed.lock().unwrap();
                    let dragging = *manager.is_dragging.lock().unwrap();
                    let resizing = *manager.is_resizing.lock().unwrap();
                    let has_edge = manager.snap_line_edge.lock().unwrap().is_some();
                    let sub_window_open = *manager.is_sub_window_open.lock().unwrap();
                    !collapsed && !dragging && !resizing && has_edge && !sub_window_open
                };

                if !should_check {
                    continue;
                }
                
                // 获取 app handle
                let app = window.app_handle();
                
                // 系统最小化的窗口不执行收起逻辑
                if window.is_minimized().unwrap_or(false) {
                    continue;
                }
                
                // 安全检查：如果处于收起状态，确保状态一致
                let is_collapsed = *manager.is_collapsed.lock().unwrap();
                if is_collapsed {
                    // 使用原子方法确保黄线和主窗口状态严格关联
                    manager.set_collapsed_state(&app, true);
                    continue;
                }

                // 如果其他窗口正在显示，则不收起主窗口（避免焦点冲突）
                if is_window_visible(&app, "desktop_analyze") 
                    || is_window_visible(&app, "downloads_analyze")
                    || is_window_visible(&app, "context_menu")
                    || is_window_visible(&app, "trash_context_menu")
                {
                    continue;
                }

                // 如果左键菜单正在打开，则不收起主窗口
                if *manager.is_main_menu_open.lock().unwrap() {
                    continue;
                }

                // 获取全局鼠标位置
                let mouse = match get_cursor_pos() {
                    Some(p) => p,
                    None => continue,
                };

                // 获取主窗口外框位置和大小
                let pos = match window.outer_position() {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                let size = match window.outer_size() {
                    Ok(s) => s,
                    Err(_) => continue,
                };

                // 检查鼠标是否在主窗口外框内
                let in_window = mouse.x >= pos.x
                    && mouse.x <= pos.x + size.width as i32
                    && mouse.y >= pos.y
                    && mouse.y <= pos.y + size.height as i32;

                if in_window {
                    // 鼠标在窗口内，标记为"已进入"
                    *manager.mouse_was_in_window.lock().unwrap() = true;
                } else {
                    // 鼠标不在窗口内，只有之前进入过才触发收起
                    let was_in = *manager.mouse_was_in_window.lock().unwrap();
                    if was_in {
                        // 检查是否在最近失焦的防抖时间内（300ms）
                        // 如果是，说明可能是点击任务栏导致的系统最小化，不执行收起
                        let should_collapse = {
                            if let Some(loss_time) = *manager.last_focus_loss_time.lock().unwrap() {
                                loss_time.elapsed() > Duration::from_millis(300)
                            } else {
                                true
                            }
                        };
                        
                        if should_collapse {
                            debug_log(&format!(
                                "[collapse_watcher] mouse=({},{}), window=({},{} size={},{})",
                                mouse.x, mouse.y, pos.x, pos.y, size.width, size.height
                            ));
                            *manager.mouse_was_in_window.lock().unwrap() = false;
                            manager.collapse_window(&window);
                        } else {
                            debug_log(&format!(
                                "[collapse_watcher] SKIP (focus loss debounce): mouse=({},{}), window=({},{}), elapsed={:?}",
                                mouse.x, mouse.y, pos.x, pos.y,
                                manager.last_focus_loss_time.lock().unwrap().map(|t| t.elapsed())
                            ));
                        }
                    } else {
                        // 鼠标在外，但从未进入过窗口，记录一次用于诊断
                        debug_log(&format!(
                            "[collapse_watcher] mouse OUT, was_in=false: mouse=({},{}), window=({},{} size={},{})",
                            mouse.x, mouse.y, pos.x, pos.y, size.width, size.height
                        ));
                    }
                }
                
                // 周期性不变量检查：确保黄线和主窗口状态严格关联
                // 防止任何竞态条件导致状态不一致
                let is_collapsed = *manager.is_collapsed.lock().unwrap();
                let main_pos = window.outer_position().unwrap_or(tauri::PhysicalPosition::new(0, 0));
                let main_is_offscreen = main_pos.x < -1000 || main_pos.y < -1000;
                
                if let Some(snap_win) = app.get_webview_window("snap_line") {
                    let snap_visible = snap_win.is_visible().unwrap_or(false);
                    
                    // 不变量：收起状态 ⟺ 黄线可见且主窗口在屏幕外
                    if is_collapsed && (!snap_visible || !main_is_offscreen) {
                        debug_log(&format!(
                            "[invariant] VIOLATION: is_collapsed=true, snap_visible={}, main_offscreen={}, main_pos=({},{}), fixing...",
                            snap_visible, main_is_offscreen, main_pos.x, main_pos.y
                        ));
                        manager.set_collapsed_state(&app, true);
                    } else if !is_collapsed && snap_visible {
                        debug_log(&format!(
                            "[invariant] VIOLATION: is_collapsed=false, snap_visible=true, main_pos=({},{}), hiding snap...",
                            main_pos.x, main_pos.y
                        ));
                        manager.set_collapsed_state(&app, false);
                    }
                }
            }
        });
    }

    fn handle_resize<R: Runtime>(&self, window: &Window<R>, size: &PhysicalSize<u32>) {
        if *self.is_resizing.lock().unwrap() {
            return;
        }
        let scale_factor = window.scale_factor().unwrap_or(1.0);
        let mut config = self.config.lock().unwrap();
        config.height = size.height as f64 / scale_factor;
    }

    fn handle_move<R: Runtime>(&self, window: &Window<R>, position: &PhysicalPosition<i32>) {
        if *self.is_resizing.lock().unwrap() {
            return;
        }
        // 收起状态下不更新位置（避免屏幕外位置被保存）
        if *self.is_collapsed.lock().unwrap() {
            return;
        }
        let scale_factor = window.scale_factor().unwrap_or(1.0);
        let mut config = self.config.lock().unwrap();
        if config.is_locked {
            return;
        }
        config.x = position.x as f64 / scale_factor;
        config.y = position.y as f64 / scale_factor;
    }

    pub fn set_resizing(&self, resizing: bool) {
        *self.is_resizing.lock().unwrap() = resizing;
        
        if resizing {
            // 开始调整大小时，取消拖拽防抖定时器并重置拖拽状态
            // 防止拖拽逻辑和调整大小逻辑冲突导致窗口抖动
            if let Some(handle) = self.drag_debounce.lock().unwrap().take() {
                handle.abort();
            }
            *self.is_dragging.lock().unwrap() = false;
        }
    }

    pub fn update_config_size(&self, width: f64, height: f64) {
        let mut config = self.config.lock().unwrap();
        config.width = width;
        config.height = height;
    }

    pub fn update_config_rect(&self, x: f64, y: f64, width: f64, height: f64) {
        let mut config = self.config.lock().unwrap();
        config.x = x;
        config.y = y;
        config.width = width;
        config.height = height;
    }

    pub fn start_dragging<R: Runtime>(&self, window: &Window<R>) {
        // 如果窗口被标记为收起，将窗口移回原来的位置并隐藏黄线
        if *self.is_collapsed.lock().unwrap() {
            *self.is_collapsed.lock().unwrap() = false;

            // 将窗口移回原来的位置
            let pos = *self.collapsed_position.lock().unwrap();
            if let Some(p) = pos {
                let _ = window.set_position(p);
            }

            let app = window.app_handle();
            if let Some(snap_win) = app.get_webview_window("snap_line") {
                let _ = snap_win.hide();
            }
        }
        *self.is_dragging.lock().unwrap() = true;
    }

    pub fn stop_dragging<R: Runtime>(&self, window: &Window<R>) {
        // 前端 mouseup 备用接口
        let was_dragging = *self.is_dragging.lock().unwrap();
        if was_dragging {
            // 先执行 perform_snap（此时 is_dragging 仍为 true，watcher 不会干扰）
            self.perform_snap(window);
            self.save_to_db();
        }
        // 最后设置 is_dragging = false，让 watcher 开始检测
        *self.is_dragging.lock().unwrap() = false;
        // 拖拽刚结束，鼠标肯定在窗口上，标记为"已进入"
        // 避免 watcher 下次轮询时鼠标已离开但 mouse_was_in_window=false 不触发收起
        *self.mouse_was_in_window.lock().unwrap() = true;
    }

    /// 初始化时自动对齐（非拖拽场景）
    pub fn init_snap<R: Runtime>(&self, window: &Window<R>) {
        self.perform_snap(window);
        self.save_to_db();
    }

    pub fn reset_snap_state<R: Runtime>(&self, window: &Window<R>) {
        *self.snap_line_edge.lock().unwrap() = None;
        *self.is_collapsed.lock().unwrap() = false;
        *self.collapsed_position.lock().unwrap() = None;
        *self.mouse_was_in_window.lock().unwrap() = false;
        
        // 设置跳过下次贴边计算，避免窗口移动后立即重新贴边
        *self.skip_next_snap.lock().unwrap() = true;

        if let Some(snap_win) = window.app_handle().get_webview_window("snap_line") {
            let _ = snap_win.hide();
        }
    }

    /// 将窗口移到主屏幕正中央
    pub fn move_to_primary_monitor_center<R: Runtime>(&self, window: &Window<R>) -> Result<(), String> {
        let primary_monitor = window.app_handle().primary_monitor()
            .map_err(|e| format!("获取主屏幕信息失败: {}", e))?
            .ok_or_else(|| "无法获取主屏幕信息".to_string())?;
        
        let work_area = primary_monitor.work_area();
        let window_size = match window.inner_size() {
            Ok(s) => s,
            Err(e) => return Err(format!("获取窗口大小失败: {}", e)),
        };
        
        // 使用物理坐标计算主屏幕中央位置
        let center_x = (work_area.size.width as i32 - window_size.width as i32) / 2 + work_area.position.x;
        let center_y = (work_area.size.height as i32 - window_size.height as i32) / 2 + work_area.position.y;
        
        let _ = window.set_position(tauri::PhysicalPosition::new(center_x, center_y));
        
        Ok(())
    }

    fn perform_snap<R: Runtime>(&self, window: &Window<R>) {
        // 收起状态下不执行贴边计算
        if *self.is_collapsed.lock().unwrap() {
            return;
        }
        
        // 如果设置了跳过下次贴边，则跳过本次计算并清除标志
        if *self.skip_next_snap.lock().unwrap() {
            *self.skip_next_snap.lock().unwrap() = false;
            return;
        }

        let outer_pos = match window.outer_position() {
            Ok(pos) => pos,
            Err(_) => return,
        };

        let inner_pos = match window.inner_position() {
            Ok(pos) => pos,
            Err(_) => return,
        };

        let inner_size = match window.inner_size() {
            Ok(s) => s,
            Err(_) => return,
        };

        debug_log(&format!(
            "[perform_snap] outer=({},{}), inner=({},{}), size=({},{})",
            outer_pos.x, outer_pos.y, inner_pos.x, inner_pos.y, inner_size.width, inner_size.height
        ));

        // 计算外框到内框的偏移（阴影边框宽度）
        let shadow_offset_x = inner_pos.x - outer_pos.x;
        let shadow_offset_y = inner_pos.y - outer_pos.y;

        // X 轴贴边计算（左/右边缘）
        let (aligned_inner_x, snap_edge_x) = self.calculate_snap_x(window, inner_pos.x, inner_size.width);

        // Y 轴贴边计算（顶部边缘）
        let aligned_inner_y = self.calculate_snap_y(window, inner_pos.y);

        // 使用与黄线相同的方法计算主窗口目标位置
        // 关键：直接基于屏幕 work_area 坐标，而非依赖 shadow_offset 的转换
        let (target_outer_x, target_outer_y) = self.calculate_main_window_snap_position(
            window, aligned_inner_x, aligned_inner_y, shadow_offset_x, shadow_offset_y
        );

        let needs_move = target_outer_x != outer_pos.x || target_outer_y != outer_pos.y;
        let scale_factor = window.scale_factor().unwrap_or(1.0);

        debug_log(&format!(
            "[perform_snap] shadow_offset=({},{}), aligned_inner=({},{}), target_outer=({},{}), needs_move={}",
            shadow_offset_x, shadow_offset_y, aligned_inner_x, aligned_inner_y, target_outer_x, target_outer_y, needs_move
        ));

        if needs_move {
            window.set_position(tauri::PhysicalPosition::new(target_outer_x, target_outer_y)).ok();

            // 闭环验证：读取设置后的实际位置，如果有偏差则再次调整
            // 确保窗口内框精确对齐到屏幕边缘，补偿 Windows DWM 可能引入的位置偏移
            // 注意：set_position 是同步调用 Win32 API，但 DWM 的实际移动是异步的
            // 我们通过多次验证来确保最终位置正确
            for attempt in 1..=5 {
                if let (Ok(new_outer), Ok(new_inner)) = (window.outer_position(), window.inner_position()) {
                    let x_diff = aligned_inner_x - new_inner.x;
                    let y_diff = aligned_inner_y - new_inner.y;
                    
                    debug_log(&format!(
                        "[perform_snap] verify attempt {}: new_outer=({},{}), new_inner=({},{}), diff=({},{}), target_inner=({},{})",
                        attempt, new_outer.x, new_outer.y, new_inner.x, new_inner.y,
                        x_diff, y_diff, aligned_inner_x, aligned_inner_y
                    ));

                    // 偏差超过1像素就调整
                    if x_diff.abs() <= 1 && y_diff.abs() <= 1 {
                        break;
                    }

                    let corrected_outer_x = new_outer.x + x_diff;
                    let corrected_outer_y = new_outer.y + y_diff;
                    
                    debug_log(&format!(
                        "[perform_snap] correcting to outer=({},{})",
                        corrected_outer_x, corrected_outer_y
                    ));
                    
                    window.set_position(tauri::PhysicalPosition::new(corrected_outer_x, corrected_outer_y)).ok();
                }
            }

            // 更新配置（存储逻辑坐标）
            {
                let mut config = self.config.lock().unwrap();
                if let Ok(final_outer) = window.outer_position() {
                    config.x = final_outer.x as f64 / scale_factor;
                    config.y = final_outer.y as f64 / scale_factor;
                    debug_log(&format!(
                        "[perform_snap] saved config: outer=({},{}), logical=({},{}), scale={}",
                        final_outer.x, final_outer.y, config.x, config.y, scale_factor
                    ));
                }
            }
        }

        // 获取贴边时使用的屏幕索引（用于黄线定位）
        let snap_monitor_index = snap_edge_x.as_ref().map(|e| e.monitor_index);

        // 确定贴边线所在边
        let screen_top_y = snap_monitor_index.and_then(|idx| {
            window.app_handle().available_monitors().ok()
                .and_then(|monitors| monitors.get(idx).cloned())
        }).or_else(|| window.current_monitor().ok().flatten())
            .map(|m| m.work_area().position.y)
            .unwrap_or(0);
        let is_at_top = aligned_inner_y == screen_top_y;

        let line_edge = if is_at_top {
            // 贴上边（无论是否同时贴左/右边）→ 上方应用线条
            Some(LineEdge::Top(snap_monitor_index))
        } else if snap_edge_x.is_some() {
            // 仅贴左/右边（未贴上边）
            match snap_edge_x.unwrap().edge_type {
                EdgeType::Left => Some(LineEdge::Left(snap_monitor_index)),
                EdgeType::Right => Some(LineEdge::Right(snap_monitor_index)),
            }
        } else {
            None
        };

        *self.snap_line_edge.lock().unwrap() = line_edge.clone();
        
        // 贴边后立即更新黄线位置和尺寸，确保收起时黄线尺寸正确
        if let Some(edge) = line_edge {
            let app = window.app_handle();
            if let Some(snap_win) = app.get_webview_window("snap_line") {
                self.position_snap_line(&snap_win, window, &edge, snap_monitor_index);
            }
        }
    }

    /// 计算主窗口贴边后的目标外框位置
    /// 使用与黄线相同的定位方法：直接基于屏幕 work_area 绝对坐标，确保精确对齐
    fn calculate_main_window_snap_position<R: Runtime>(
        &self,
        window: &Window<R>,
        target_inner_x: i32,
        target_inner_y: i32,
        shadow_offset_x: i32,
        shadow_offset_y: i32,
    ) -> (i32, i32) {
        // 初始估算：基于 shadow_offset 转换
        let mut outer_x = target_inner_x - shadow_offset_x;
        let mut outer_y = target_inner_y - shadow_offset_y;

        // 如果贴到了屏幕边缘，使用与黄线相同的方法（屏幕 work_area 坐标）精确定位
        let monitors = match window.app_handle().available_monitors() {
            Ok(m) => m,
            Err(_) => return (outer_x, outer_y),
        };

        // 找到目标内框位置所在的屏幕
        let target_right = target_inner_x + 1; // 用1像素宽的矩形来判断
        for monitor in &monitors {
            let wa = monitor.work_area();
            let wa_left = wa.position.x;
            let wa_right = wa.position.x + wa.size.width as i32;
            let wa_top = wa.position.y;

            // 左边对齐：目标内框 X 等于屏幕左边缘
            if target_inner_x == wa_left {
                // 与黄线定位逻辑一致：外框 X = 屏幕左边缘 - shadow_offset
                outer_x = wa_left - shadow_offset_x;
                debug_log(&format!(
                    "[calc_main_snap] LEFT edge snap: wa_left={}, shadow_x={}, outer_x={}",
                    wa_left, shadow_offset_x, outer_x
                ));
            }

            // 右边对齐：目标内框右边缘等于屏幕右边缘
            let inner_width = match window.inner_size() {
                Ok(s) => s.width as i32,
                Err(_) => 300,
            };
            if target_inner_x + inner_width == wa_right {
                outer_x = wa_right - inner_width - shadow_offset_x;
                debug_log(&format!(
                    "[calc_main_snap] RIGHT edge snap: wa_right={}, inner_w={}, shadow_x={}, outer_x={}",
                    wa_right, inner_width, shadow_offset_x, outer_x
                ));
            }

            // 顶部对齐
            if target_inner_y == wa_top {
                outer_y = wa_top - shadow_offset_y;
                debug_log(&format!(
                    "[calc_main_snap] TOP edge snap: wa_top={}, shadow_y={}, outer_y={}",
                    wa_top, shadow_offset_y, outer_y
                ));
            }
        }

        (outer_x, outer_y)
    }

    /// Y 轴贴边：窗口接近屏幕顶部时对齐到顶部
    fn calculate_snap_y<R: Runtime>(&self, window: &Window<R>, window_y: i32) -> i32 {
        let monitor = match window.current_monitor() {
            Ok(Some(m)) => m,
            _ => return window_y,
        };
        let work_area = monitor.work_area();
        let top_y = work_area.position.y;
        let dist = window_y - top_y;
        if dist.abs() <= self.drag_threshold {
            return top_y;
        }
        window_y
    }

    fn calculate_snap_x<R: Runtime>(&self, window: &Window<R>, window_x: i32, window_width: u32) -> (i32, Option<ScreenEdge>) {
        let monitors = match window.app_handle().available_monitors() {
            Ok(m) => m,
            Err(_) => return (window_x, None),
        };

        if monitors.is_empty() {
            return (window_x, None);
        }

        let edges = self.get_all_monitor_edges(&monitors);
        let window_right = window_x + window_width as i32;
        let window_center_x = window_x + (window_width / 2) as i32;

        // 调试：输出所有屏幕边界信息
        for (i, m) in monitors.iter().enumerate() {
            let wa = m.work_area();
            debug_log(&format!(
                "[calc_snap_x] monitor[{}]: work_area=({},{} {}x{})",
                i, wa.position.x, wa.position.y, wa.size.width, wa.size.height
            ));
        }
        debug_log(&format!(
            "[calc_snap_x] input: window_x={}, window_right={}, threshold={}",
            window_x, window_right, self.drag_threshold
        ));

        // 找到窗口中心所在的屏幕索引（备用）
        let _current_monitor_index = monitors.iter().enumerate().find(|(_, m)| {
            let work_area = m.work_area();
            let wa_pos = work_area.position;
            let wa_size = work_area.size;
            window_center_x >= wa_pos.x && window_center_x < wa_pos.x + wa_size.width as i32
        }).map(|(idx, _)| idx);

        // 优先级 1: 共享边（双屏幕衔接处）
        for edge in &edges {
            if edge.is_shared {
                let dist_left = edge.position - window_x;
                let dist_right = window_right - edge.position;

                if dist_left.abs() <= self.drag_threshold || dist_right.abs() <= self.drag_threshold {
                    debug_log(&format!(
                        "[calc_snap_x] shared edge hit: pos={}, dist_left={}, dist_right={}, prefer_left={}",
                        edge.position, dist_left, dist_right, dist_left >= dist_right
                    ));
                    // 使用有符号距离判断贴边方向（保证贴边后稳定，不会反复跳动）：
                    // - dist_left >= dist_right：窗口主体在共享边左侧 → 贴到左屏幕的右边缘
                    // - dist_left < dist_right：窗口主体在共享边右侧 → 贴到右屏幕的左边缘
                    // 贴边后距离关系保持不变，因此不会产生抖动
                    let prefer_left_edge = dist_left >= dist_right;

                    // 找到对应的 ScreenEdge（包含正确的 monitor_index）
                    let target_edge = edges.iter()
                        .find(|e| e.position == edge.position && 
                               ((prefer_left_edge && e.edge_type == EdgeType::Right) ||
                                (!prefer_left_edge && e.edge_type == EdgeType::Left)))
                        .cloned();

                    if prefer_left_edge {
                        let new_x = edge.position - window_width as i32;
                        return (new_x, target_edge);
                    } else {
                        return (edge.position, target_edge);
                    }
                }
            }
        }

        // 优先级 2: 普通左/右边缘
        for edge in &edges {
            if !edge.is_shared {
                match edge.edge_type {
                    EdgeType::Left => {
                        let dist = window_x - edge.position;
                        if dist.abs() <= self.drag_threshold {
                            return (edge.position, Some(edge.clone()));
                        }
                    }
                    EdgeType::Right => {
                        let dist = window_right - edge.position;
                        if dist.abs() <= self.drag_threshold {
                            let new_x = edge.position - window_width as i32;
                            return (new_x, Some(edge.clone()));
                        }
                    }
                }
            }
        }

        // 优先级 3: 外侧边缘夹紧（防止窗口被拖到屏幕外不可见区域）
        // 当窗口左边缘越过最左屏幕左边界（window_x < leftmost），或右边缘越过
        // 最右屏幕右边界（window_right > rightmost）时，强制贴到对应外侧边缘，
        // 避免窗口和黄线一起消失在不可见区域。
        {
            let leftmost = edges.iter()
                .filter(|e| e.edge_type == EdgeType::Left)
                .min_by_key(|e| e.position);
            let rightmost = edges.iter()
                .filter(|e| e.edge_type == EdgeType::Right)
                .max_by_key(|e| e.position);

            if let Some(left_edge) = leftmost {
                if window_x < left_edge.position {
                    debug_log(&format!(
                        "[calc_snap_x] CLAMP LEFT: window_x={} < leftmost={}, snapping to {}",
                        window_x, left_edge.position, left_edge.position
                    ));
                    return (left_edge.position, Some(left_edge.clone()));
                }
            }
            if let Some(right_edge) = rightmost {
                if window_right > right_edge.position {
                    let new_x = right_edge.position - window_width as i32;
                    debug_log(&format!(
                        "[calc_snap_x] CLAMP RIGHT: window_right={} > rightmost={}, snapping to {}",
                        window_right, right_edge.position, new_x
                    ));
                    return (new_x, Some(right_edge.clone()));
                }
            }
        }

        (window_x, None)
    }

    fn get_all_monitor_edges(&self, monitors: &[tauri::Monitor]) -> Vec<ScreenEdge> {
        let mut edges: Vec<ScreenEdge> = Vec::new();
        let mut edge_positions: Vec<i32> = Vec::new();

        for (monitor_index, monitor) in monitors.iter().enumerate() {
            let work_area = monitor.work_area();
            let wa_pos = work_area.position;
            let wa_size = work_area.size;

            let left_x = wa_pos.x;
            let right_x = wa_pos.x + wa_size.width as i32;

            edges.push(ScreenEdge {
                edge_type: EdgeType::Left,
                position: left_x,
                is_shared: false,
                monitor_index,
            });

            edges.push(ScreenEdge {
                edge_type: EdgeType::Right,
                position: right_x,
                is_shared: false,
                monitor_index,
            });

            edge_positions.push(left_x);
            edge_positions.push(right_x);
        }

        // 检测共享边：相同坐标出现 >= 2 次说明两个屏幕共享此边
        edge_positions.sort();
        edge_positions.dedup();

        for pos in edge_positions {
            let count = edges.iter().filter(|e| e.position == pos).count();
            if count >= 2 {
                for edge in edges.iter_mut() {
                    if edge.position == pos {
                        edge.is_shared = true;
                    }
                }
            }
        }

        edges
    }

    /// 收起窗口：将主窗口移到屏幕外（保留任务栏图标，无动画），在贴边位置显示黄色贴边线
    pub fn collapse_window<R: Runtime>(&self, main_window: &Window<R>) {
        // 拖拽/调整大小过程中不收起
        if *self.is_dragging.lock().unwrap() || *self.is_resizing.lock().unwrap() {
            return;
        }

        // 固定窗口模式下不收起（保留贴边对齐，但禁用收起）
        if self.config.lock().unwrap().is_locked {
            return;
        }

        // 已收起则跳过
        if *self.is_collapsed.lock().unwrap() {
            return;
        }

        // 未贴边则跳过
        let line_edge = self.snap_line_edge.lock().unwrap().clone();
        if line_edge.is_none() {
            debug_log("[collapse] snap_line_edge is None, skipping");
            return;
        }

        debug_log(&format!("[collapse] triggered, line_edge={:?}", line_edge));

        let app = main_window.app_handle();
        let snap_win = match app.get_webview_window("snap_line") {
            Some(w) => w,
            None => {
                return;
            }
        };

        // 定位贴边线
        let (edge_variant, monitor_index) = match line_edge.unwrap() {
            LineEdge::Top(idx) => (LineEdge::Top(None), idx),
            LineEdge::Left(idx) => (LineEdge::Left(None), idx),
            LineEdge::Right(idx) => (LineEdge::Right(None), idx),
        };
        self.position_snap_line(&snap_win, main_window, &edge_variant, monitor_index);

        // 关闭所有菜单
        close_context_menu(main_window.clone());
        close_trash_context_menu(main_window.clone());

        // 通知前端关闭主菜单（主菜单是前端Teleport组件）
        let _ = main_window.emit("window_collapsed", serde_json::json!({}));

        // 新思路：基于黄线的精确位置反推并更新主窗口的存储位置
        // 这样展开时窗口能精确恢复到与黄线对齐的位置
        // 黄线位置基于屏幕 work_area 绝对坐标计算，是"权威"位置
        let main_inner_size = main_window.inner_size().unwrap_or(tauri::PhysicalSize::new(300, 600));
        let main_outer_pos = main_window.outer_position().unwrap_or(tauri::PhysicalPosition::new(0, 0));
        let main_inner_pos = main_window.inner_position().unwrap_or(tauri::PhysicalPosition::new(0, 0));
        let shadow_offset_x = main_inner_pos.x - main_outer_pos.x;
        let shadow_offset_y = main_inner_pos.y - main_outer_pos.y;
        let scale_factor = main_window.scale_factor().unwrap_or(1.0);

        // 获取贴边屏幕的 work_area（与 position_snap_line 使用相同的屏幕选择逻辑）
        let snap_monitor = monitor_index.and_then(|idx| {
            main_window.app_handle().available_monitors().ok()
                .and_then(|monitors| monitors.get(idx).cloned())
        }).or_else(|| main_window.current_monitor().ok().flatten());

        let wa = snap_monitor.as_ref()
            .map(|m| m.work_area())
            .cloned()
            .unwrap_or_else(|| tauri::PhysicalRect {
                position: tauri::PhysicalPosition::new(0, 0),
                size: tauri::PhysicalSize::new(1920, 1080),
            });

        // 根据贴边方向，从屏幕 work_area 反推主窗口的精确外框位置
        let (precise_outer_x, precise_outer_y) = match &edge_variant {
            LineEdge::Left(_) => {
                // 左贴边：主窗口内框 X = 屏幕左边缘
                let inner_x = wa.position.x;
                let outer_x = inner_x - shadow_offset_x;
                // Y 坐标保持当前窗口位置（垂直位置不变）
                (outer_x, main_outer_pos.y)
            }
            LineEdge::Right(_) => {
                // 右贴边：主窗口内框右边 = 屏幕右边缘
                let wa_right = wa.position.x + wa.size.width as i32;
                let outer_x = wa_right - main_inner_size.width as i32 - shadow_offset_x;
                (outer_x, main_outer_pos.y)
            }
            LineEdge::Top(_) => {
                // 上贴边：主窗口内框 Y = 屏幕顶部
                let inner_y = wa.position.y;
                let outer_y = inner_y - shadow_offset_y;
                // X 坐标保持当前窗口位置（水平位置不变）
                (main_outer_pos.x, outer_y)
            }
        };

        // 更新 config 和 collapsed_position 为基于黄线反推的精确位置
        {
            let mut config = self.config.lock().unwrap();
            config.x = precise_outer_x as f64 / scale_factor;
            config.y = precise_outer_y as f64 / scale_factor;
            debug_log(&format!(
                "[collapse] updated config from snap line: precise_outer=({},{}), logical=({},{}), wa=({},{},{},{})",
                precise_outer_x, precise_outer_y, config.x, config.y,
                wa.position.x, wa.position.y, wa.size.width, wa.size.height
            ));
        }
        *self.collapsed_position.lock().unwrap() = 
            Some(tauri::PhysicalPosition::new(precise_outer_x, precise_outer_y));

        // 使用原子方法设置收起状态
        self.set_collapsed_state(&app, true);
    }
    
    /// 原子设置收起状态，确保黄线和主窗口状态严格关联
    /// collapsed=true: 显示黄线，主窗口移到屏幕外
    /// collapsed=false: 隐藏黄线，主窗口恢复到正确位置并显示
    pub fn set_collapsed_state<R: Runtime>(&self, app: &tauri::AppHandle<R>, collapsed: bool) {
        let main_window = match app.get_webview_window("main") {
            Some(w) => w,
            None => return,
        };
        let snap_win = match app.get_webview_window("snap_line") {
            Some(w) => w,
            None => return,
        };
        
        debug_log(&format!("[set_collapsed_state] collapsed={}, is_collapsed={}", collapsed, *self.is_collapsed.lock().unwrap()));
        
        if collapsed {
            // 收起：显示黄线，主窗口移到屏幕外
            let _ = snap_win.show();
            let _ = main_window.set_position(tauri::PhysicalPosition::new(-3000, -3000));
            debug_log("[set_collapsed_state] snap_win shown, main moved to -3000,-3000");
        } else {
            // 展开：隐藏黄线，主窗口恢复位置并显示
            let _ = snap_win.hide();
            debug_log("[set_collapsed_state] snap_win hidden");
            let _ = main_window.unminimize();
            let _ = main_window.show();
            
            // 恢复到收起前保存的位置（collapse_window 中已基于黄线反推更新）
            let pos = *self.collapsed_position.lock().unwrap();
            if let Some(p) = pos {
                let _ = main_window.set_position(p);
                debug_log(&format!("[set_collapsed_state] main restored to ({},{})", p.x, p.y));

                // 展开后验证：读取实际位置，如果有偏差则补偿调整
                // 确保窗口精确恢复到与黄线对齐的位置
                if let (Ok(actual_outer), Ok(actual_inner)) = (main_window.outer_position(), main_window.inner_position()) {
                    // 计算 shadow_offset（实际内框与外框的差）
                    let actual_shadow_x = actual_inner.x - actual_outer.x;
                    // 目标内框位置 = 目标外框位置 + shadow_offset
                    let target_inner_x = p.x + actual_shadow_x;
                    let x_diff = target_inner_x - actual_inner.x;
                    
                    if x_diff.abs() > 1 {
                        debug_log(&format!(
                            "[set_collapsed_state] EXPAND verify: target_inner_x={}, actual_inner={}, diff={}, correcting...",
                            target_inner_x, actual_inner.x, x_diff
                        ));
                        let corrected_outer = tauri::PhysicalPosition::new(actual_outer.x + x_diff, actual_outer.y);
                        let _ = main_window.set_position(corrected_outer);
                    }
                }
            } else {
                let config = self.config.lock().unwrap();
                let _ = main_window.set_position(tauri::LogicalPosition::new(config.x, config.y));
            }
        }
        
        *self.is_collapsed.lock().unwrap() = collapsed;
    }

    /// 展开窗口：隐藏贴边线，将主窗口移回原来的位置（无动画）
    pub fn expand_window<R: Runtime>(&self, app: &tauri::AppHandle<R>) {
        let is_collapsed = *self.is_collapsed.lock().unwrap();
        if !is_collapsed {
            return;
        }
        
        // 展开后标记鼠标已进入窗口，防止立即触发收起
        *self.mouse_was_in_window.lock().unwrap() = true;
        
        self.set_collapsed_state(app, false);
    }

    /// 根据贴边方向定位贴边线窗口
    /// 关键：snap_line 窗口自身有阴影偏移，需扣除后才能让可见区域精确贴边
    fn position_snap_line<R: Runtime>(
        &self,
        snap_win: &tauri::WebviewWindow<R>,
        main_window: &Window<R>,
        edge: &LineEdge,
        snap_monitor_index: Option<usize>,
    ) {
        // 主窗口的 inner_position 即为贴边后的屏幕边缘坐标
        let main_inner_pos = match main_window.inner_position() {
            Ok(p) => p,
            Err(_) => return,
        };
        let main_inner_size = match main_window.inner_size() {
            Ok(s) => s,
            Err(_) => return,
        };
        let scale_factor = main_window.scale_factor().unwrap_or(1.0);

        // 获取贴边屏幕的工作区域（用于确定黄线高度）
        let snap_monitor = snap_monitor_index.and_then(|idx| {
            main_window.app_handle().available_monitors().ok()
                .and_then(|monitors| monitors.get(idx).cloned())
        }).or_else(|| main_window.current_monitor().ok().flatten());

        // 预先提取屏幕顶部 Y（仅用于顶部贴边时对齐屏幕顶部）
        let screen_top_y = snap_monitor.as_ref()
            .map(|m| m.work_area().position.y)
            .unwrap_or(main_inner_pos.y);

        // 贴边线厚度：5 逻辑像素（视觉大小）
        let thickness = (5.0 * scale_factor) as i32;
        // 鼠标热区扩展：在贴边方向外扩展的像素数（不影响视觉）
        let hotzone_extend = (5.0 * scale_factor) as i32;

        // 计算窗口总尺寸（包含热区）和可见内容区位置
        // 热区扩展方向：向屏幕内部扩展，确保鼠标可以到达
        let (window_w, window_h, visible_offset_x, visible_offset_y, visible_w, visible_h) = match edge {
            LineEdge::Top(_) => {
                // 顶部贴边：窗口向下扩展热区，可见区域在顶部，热区在下方
                (main_inner_size.width as i32, thickness + hotzone_extend, 0, 0, main_inner_size.width as i32, thickness)
            }
            LineEdge::Left(_) => {
                // 左侧贴边：窗口向右扩展热区，可见区域在左侧，热区在右侧
                // 高度与主窗口一致，确保黄线尺寸与窗口对齐
                (thickness + hotzone_extend, main_inner_size.height as i32, 0, 0, thickness, main_inner_size.height as i32)
            }
            LineEdge::Right(_) => {
                // 右侧贴边：窗口向左扩展热区，可见区域在右侧，热区在左侧
                // 高度与主窗口一致，确保黄线尺寸与窗口对齐
                (thickness + hotzone_extend, main_inner_size.height as i32, hotzone_extend, 0, thickness, main_inner_size.height as i32)
            }
        };

        // 先设置窗口总尺寸（包含热区）
        let _ = snap_win.set_size(PhysicalSize::new(window_w as u32, window_h as u32));

        // 获取 snap_line 窗口自身的 outer/inner position 差值（阴影偏移）
        // 注意：此时窗口可能还在屏幕外 (-3000,-3000)，但 shadow_offset 与位置无关
        let snap_outer = match snap_win.outer_position() {
            Ok(p) => p,
            Err(_) => return,
        };
        let snap_inner = match snap_win.inner_position() {
            Ok(p) => p,
            Err(_) => return,
        };
        let snap_shadow_x = snap_inner.x - snap_outer.x;
        let snap_shadow_y = snap_inner.y - snap_outer.y;

        // 计算目标位置：让可见区域精确贴边，热区向屏幕内部扩展
        // 关键：黄线的 X 坐标基于屏幕边缘（固定值），而非主窗口当前位置。
        // 因为窗口可能在贴边后继续被微拖到屏幕外（如 inner=-7），
        // 如果黄线跟随窗口位置，就会出现在屏幕外不可见。
        // Y 坐标仍与主窗口对齐（保持垂直位置一致）。
        let (outer_x, outer_y) = match edge {
            LineEdge::Top(_) => {
                // 水平线：可见区域 y 对齐屏幕顶部，窗口从顶部开始向下扩展
                (main_inner_pos.x - snap_shadow_x, screen_top_y - snap_shadow_y)
            }
            LineEdge::Left(_) => {
                // 左贴边：黄线 X 固定在屏幕左边缘（不跟随窗口位置）
                let screen_left_x = snap_monitor.as_ref()
                    .map(|m| m.work_area().position.x)
                    .unwrap_or(main_inner_pos.x);
                (screen_left_x - snap_shadow_x, main_inner_pos.y - snap_shadow_y)
            }
            LineEdge::Right(_) => {
                // 右贴边：黄线 X 固定在屏幕右边缘（不跟随窗口位置）
                let screen_right_x = snap_monitor.as_ref()
                    .map(|m| m.work_area().position.x + m.work_area().size.width as i32)
                    .unwrap_or(main_inner_pos.x + main_inner_size.width as i32);
                (screen_right_x - window_w - snap_shadow_x, main_inner_pos.y - snap_shadow_y)
            }
        };

        let _ = snap_win.set_position(PhysicalPosition::new(outer_x, outer_y));

        debug_log(&format!(
            "[position_snap_line] edge={:?}, monitor={:?}, main_inner=({},{}), main_size=({},{}), snap_outer=({},{}), snap_shadow=({},{}), visible=({},{},{},{})",
            edge, snap_monitor_index, main_inner_pos.x, main_inner_pos.y, main_inner_size.width, main_inner_size.height,
            outer_x, outer_y, snap_shadow_x, snap_shadow_y, visible_offset_x, visible_offset_y, visible_w, visible_h
        ));

        // 用 SetWindowRgn 精确裁剪窗口区域，只保留可见的黄线部分
        // visible_offset_x/y 是可见区域相对于窗口内容区左上角的偏移
        set_window_exact_region_with_offset(snap_win, visible_offset_x, visible_offset_y, visible_w, visible_h);
    }

    fn handle_window_focused<R: Runtime>(&self, window: &Window<R>) {
        let _ = window.emit("app_focused", serde_json::json!({}));
        
        let is_collapsed = *self.is_collapsed.lock().unwrap();
        let is_minimized = window.is_minimized().unwrap_or(false);
        let pos = window.outer_position().unwrap_or(tauri::PhysicalPosition::new(0, 0));
        let has_snap_edge = self.snap_line_edge.lock().unwrap().is_some();
        
        // 使用原子方法设置状态，确保黄线和主窗口严格关联
        if is_collapsed {
            // 收起状态：确保黄线可见、主窗口在屏幕外
            self.set_collapsed_state(&window.app_handle(), true);
        } else {
            // 窗口未收起但可能被最小化或移到屏幕外
            let is_offscreen = pos.x < -1000 || pos.y < -1000;
            
            if is_minimized || is_offscreen {
                if has_snap_edge {
                    // 有贴边，恢复到收起状态（显示黄线）
                    self.collapse_window(window);
                } else {
                    // 没有贴边，展开显示主窗口
                    self.set_collapsed_state(&window.app_handle(), false);
                }
            }
        }
    }

    fn handle_monitor_change<R: Runtime>(&self, window: &Window<R>) {
        // 显示器变化时重新执行贴边对齐，确保窗口位置正确
        self.perform_snap(window);
        
        // 如果当前有贴边，更新黄线位置和尺寸
        let line_edge = self.snap_line_edge.lock().unwrap().clone();
        if let Some(edge) = line_edge {
            let (edge_variant, monitor_index) = match edge {
                LineEdge::Top(idx) => (LineEdge::Top(None), idx),
                LineEdge::Left(idx) => (LineEdge::Left(None), idx),
                LineEdge::Right(idx) => (LineEdge::Right(None), idx),
            };
            let app = window.app_handle();
            if let Some(snap_win) = app.get_webview_window("snap_line") {
                self.position_snap_line(&snap_win, window, &edge_variant, monitor_index);
            }
        }
        
        self.save_to_db();
    }

    pub fn toggle_lock<R: Runtime>(&self, _window: &Window<R>) -> bool {
        let mut config = self.config.lock().unwrap();
        config.is_locked = !config.is_locked;
        let locked = config.is_locked;
        drop(config);

        self.save_to_db();
        locked
    }

    /// 设置子窗口/对话框/菜单的打开状态
    pub fn set_sub_window_open(&self, open: bool) {
        *self.is_sub_window_open.lock().unwrap() = open;
    }

    pub fn get_config(&self) -> WindowConfigData {
        self.config.lock().unwrap().clone()
    }

    pub fn save_config(&self, config: WindowConfigData) -> bool {
        *self.config.lock().unwrap() = config;
        true
    }

    pub fn set_always_on_top<R: Runtime>(&self, window: &Window<R>, enabled: bool) {
        let mut config = self.config.lock().unwrap();
        config.always_on_top = enabled;
        window.set_always_on_top(enabled).ok();
    }

    pub fn save_to_db(&self) {
        let config = self.config.lock().unwrap();
        // 只保存有效位置（避免保存屏幕外位置）
        if config.x >= -100.0 && config.y >= -100.0 {
            let _ = db::save_db_window_config(config.x, config.y, config.height, config.is_locked);
        }
    }

    pub fn load_from_db(&self) {
        match db::get_db_window_config() {
            Ok(db_config) => {
                let mut config = self.config.lock().unwrap();
                // 验证位置是否有效（避免使用屏幕外的位置）
                if db_config.x >= -100.0 && db_config.y >= -100.0 {
                    config.x = db_config.x;
                    config.y = db_config.y;
                }
                // 从数据库读取高度，限制在合理范围内
                config.height = if db_config.height >= 300.0 && db_config.height <= 9999.0 {
                    db_config.height
                } else {
                    600.0
                };
                config.is_locked = db_config.locked;
            }
            Err(_) => {}
        }
    }

    pub fn apply_config_to_window<R: Runtime>(&self, window: &Window<R>) {
        let (x, y, width, height, always_on_top) = {
            let config = self.config.lock().unwrap();
            (config.x, config.y, config.width, config.height, config.always_on_top)
        };
        window.set_position(tauri::LogicalPosition::new(x, y)).ok();
        window.set_size(tauri::LogicalSize::new(width, height)).ok();
        window.set_always_on_top(always_on_top).ok();
    }
}

#[tauri::command]
pub fn toggle_window_lock(window: tauri::Window) -> bool {
    let manager = window.app_handle().state::<Arc<WindowManager>>();
    manager.toggle_lock(&window)
}

#[tauri::command]
pub fn get_window_config(window: tauri::Window) -> WindowConfigData {
    let manager = window.app_handle().state::<Arc<WindowManager>>();
    manager.get_config()
}

#[tauri::command]
pub fn save_window_config(window: tauri::Window, config: WindowConfigData) -> bool {
    let manager = window.app_handle().state::<Arc<WindowManager>>();
    manager.save_config(config)
}

#[tauri::command]
pub fn set_always_on_top(window: tauri::Window, enabled: bool) {
    let manager = window.app_handle().state::<Arc<WindowManager>>();
    manager.set_always_on_top(&window, enabled);
}

#[tauri::command]
pub fn start_dragging(window: tauri::Window) {
    let manager = window.app_handle().state::<Arc<WindowManager>>();
    manager.start_dragging(&window);
    window.start_dragging().ok();
}

#[tauri::command]
pub fn stop_dragging(window: tauri::Window) {
    let manager = window.app_handle().state::<Arc<WindowManager>>();
    manager.stop_dragging(&window);
}

#[tauri::command]
pub fn collapse_to_snap_line(window: tauri::Window) {
    let manager = window.app_handle().state::<Arc<WindowManager>>();
    manager.collapse_window(&window);
}

#[tauri::command]
pub fn set_main_menu_open(window: tauri::Window, open: bool) {
    let manager = window.app_handle().state::<Arc<WindowManager>>();
    *manager.is_main_menu_open.lock().unwrap() = open;
}

#[tauri::command]
pub fn is_main_menu_open(window: tauri::Window) -> bool {
    let manager = window.app_handle().state::<Arc<WindowManager>>();
    let x = *manager.is_main_menu_open.lock().unwrap();
    x
}

#[tauri::command]
pub fn set_resizing(window: tauri::Window, resizing: bool) {
    let manager = window.app_handle().state::<Arc<WindowManager>>();
    manager.set_resizing(resizing);
}

#[tauri::command]
pub fn reset_snap_state_cmd(window: tauri::Window) {
    let manager = window.app_handle().state::<Arc<WindowManager>>();
    manager.reset_snap_state(&window);
}

#[tauri::command]
pub fn set_sub_window_open(window: tauri::Window, open: bool) {
    let manager = window.app_handle().state::<Arc<WindowManager>>();
    manager.set_sub_window_open(open);
}

/// 用 Win32 GetCursorPos 获取全局鼠标物理坐标
/// 即使鼠标不在窗口内也能获取，不受 Tauri 拖拽接管影响
fn get_cursor_pos() -> Option<PhysicalPosition<i32>> {
    use winapi::shared::windef::POINT;
    use winapi::um::winuser::GetCursorPos;

    let mut point: POINT = unsafe { std::mem::zeroed() };
    let result = unsafe { GetCursorPos(&mut point) };
    if result != 0 {
        Some(PhysicalPosition::new(point.x, point.y))
    } else {
        None
    }
}
