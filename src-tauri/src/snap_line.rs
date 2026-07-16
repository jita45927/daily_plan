use std::sync::Arc;

use tauri::{Manager, Runtime, WebviewUrl, WebviewWindowBuilder};
use crate::window::WindowManager;

pub struct SnapLineManager {
}

impl SnapLineManager {
    pub fn new() -> Self {
        Self {
        }
    }
}

impl Default for SnapLineManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 用 SetWindowRgn 精确裁剪窗口可见区域，完全消除透明边框/阴影
/// 窗口内容区域外的所有像素都不可见、不可点击
/// 注意：SetWindowRgn 坐标系是相对于窗口外框（含阴影）左上角的，
/// 必须用 ClientToScreen 换算内容区位置后才能精确对齐
pub fn set_window_exact_region<R: Runtime>(
    window: &tauri::WebviewWindow<R>,
    content_width: i32,
    content_height: i32,
) {
    use winapi::shared::windef::{POINT, RECT};
    use winapi::um::wingdi::CreateRectRgn;
    use winapi::um::winuser::{ClientToScreen, GetWindowRect, SetWindowRgn};

    let hwnd = match window.hwnd() {
        Ok(h) => h,
        Err(_) => return,
    };

    unsafe {
        // 获取窗口外框矩形（屏幕坐标）
        let mut wnd_rect: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd.0 as *mut _, &mut wnd_rect) == 0 {
            return;
        }

        // 获取客户区左上角的屏幕坐标
        let mut client_origin: POINT = std::mem::zeroed();
        if ClientToScreen(hwnd.0 as *mut _, &mut client_origin) == 0 {
            return;
        }

        // 计算客户区相对于窗口外框左上角的偏移
        let offset_x = client_origin.x - wnd_rect.left;
        let offset_y = client_origin.y - wnd_rect.top;

        // region 正好覆盖内容区，裁掉四周的透明阴影边框
        let hrgn = CreateRectRgn(
            offset_x,
            offset_y,
            offset_x + content_width,
            offset_y + content_height,
        );

        if hrgn.is_null() {
            return;
        }

        SetWindowRgn(hwnd.0 as *mut _, hrgn, 1);
        // SetWindowRgn 接管 hrgn 所有权，无需手动删除
    }
}

/// 禁用窗口 DWM 阴影，确保贴边线是纯净的黄色线条无额外边框效果
fn disable_window_shadow<R: Runtime>(window: &tauri::WebviewWindow<R>) {
    use winapi::um::dwmapi::DwmSetWindowAttribute;

    // DWMWA_NCRENDERING_POLICY = 2, DWMNCRP_DISABLED = 1
    const DWMWA_NCRENDERING_POLICY: u32 = 2;
    const DWMNCRP_DISABLED: u32 = 1;

    let hwnd = match window.hwnd() {
        Ok(h) => h,
        Err(_) => return,
    };

    let policy: u32 = DWMNCRP_DISABLED;
    unsafe {
        DwmSetWindowAttribute(
            hwnd.0 as *mut _,
            DWMWA_NCRENDERING_POLICY,
            &policy as *const _ as *const _,
            std::mem::size_of::<u32>() as u32,
        );
    }
}

/// 预创建贴边线窗口（初始隐藏在屏幕外）
pub fn setup_snap_line_window<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), String> {
    let snap_win = WebviewWindowBuilder::new(
        app,
        "snap_line",
        WebviewUrl::App("/snap-line.html".into()),
    )
    .title("贴边线")
    .inner_size(5.0, 600.0)
    .decorations(false)
    .transparent(false)
    .always_on_top(true)
    .visible(false)
    .skip_taskbar(true)
    .resizable(false)
    .position(-3000.0, -3000.0)
    .build()
    .map_err(|e| format!("failed to create snap line window: {:?}", e))?;

    // 禁用 DWM 阴影，确保贴边线是纯净的 10px 黄色线条
    disable_window_shadow(&snap_win);

    Ok(())
}

#[tauri::command]
pub fn expand_from_snap_line<R: Runtime>(window: tauri::Window<R>) {
    let app = window.app_handle();
    let manager = app.state::<Arc<WindowManager>>();
    manager.expand_window(app);
}
