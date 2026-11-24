use crate::protocol::VideoRect;

#[cfg(target_os = "windows")]
fn find_own_window_by_title(title: &str) -> isize {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    struct SearchCtx {
        pid: u32,
        target: Vec<u16>,
        found: isize,
    }

    unsafe extern "system" {
        fn GetCurrentProcessId() -> u32;
        fn EnumWindows(
            lp_enum_func: unsafe extern "system" fn(isize, isize) -> i32,
            l_param: isize,
        ) -> i32;
        fn GetWindowThreadProcessId(hwnd: isize, lpdw_process_id: *mut u32) -> u32;
        fn GetWindowTextW(hwnd: isize, lp_string: *mut u16, n_max_count: i32) -> i32;
        fn FindWindowW(
            lp_class_name: *const u16,
            lp_window_name: *const u16,
        ) -> *mut core::ffi::c_void;
    }

    unsafe extern "system" fn enum_cb(hwnd: isize, l_param: isize) -> i32 {
        unsafe {
            let ctx = &mut *(l_param as *mut SearchCtx);
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid != ctx.pid {
                return 1;
            }
            let mut buf = [0u16; 256];
            let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
            if len > 0 && &buf[..len as usize] == ctx.target.as_slice() {
                ctx.found = hwnd;
                return 0;
            }
            1
        }
    }

    let target: Vec<u16> = OsStr::new(title).encode_wide().collect();
    let mut ctx = SearchCtx {
        pid: unsafe { GetCurrentProcessId() },
        target,
        found: 0,
    };
    unsafe {
        EnumWindows(enum_cb, (&mut ctx as *mut SearchCtx) as isize);
        if ctx.found != 0 {
            return ctx.found;
        }
        let wide: Vec<u16> = OsStr::new(title).encode_wide().chain(Some(0)).collect();
        FindWindowW(core::ptr::null(), wide.as_ptr()) as isize
    }
}

/// Computes the physical screen rectangle of a client-coordinate box inside the Riptide window,
/// returning `None` if Riptide is minimized, hidden, or not the foreground app (so the embedded
/// video window never floats on top of other applications when the user switches windows).
#[cfg(target_os = "windows")]
pub fn host_video_screen_rect(
    host_title: &str,
    client_x: f32,
    client_y: f32,
    client_w: f32,
    client_h: f32,
    scale_factor: f32,
) -> Option<VideoRect> {
    #[repr(C)]
    struct PointW {
        x: i32,
        y: i32,
    }

    unsafe extern "system" {
        fn IsWindowVisible(hwnd: isize) -> i32;
        fn IsIconic(hwnd: isize) -> i32;
        fn GetForegroundWindow() -> isize;
        fn ClientToScreen(hwnd: isize, lp_point: *mut PointW) -> i32;
    }

    if client_w < 40.0 || client_h < 40.0 {
        return None;
    }

    let host_hwnd = find_own_window_by_title(host_title);
    if host_hwnd == 0 {
        return None;
    }

    unsafe {
        if IsWindowVisible(host_hwnd) == 0 || IsIconic(host_hwnd) != 0 {
            return None;
        }
        let fg = GetForegroundWindow();
        let wv_hwnd = find_own_window_by_title("Riptide — YouTube Music Webapp (Sign In / Inspector)");
        if fg != host_hwnd && (wv_hwnd == 0 || fg != wv_hwnd) {
            return None;
        }

        let mut pt = PointW {
            x: (client_x * scale_factor).round() as i32,
            y: (client_y * scale_factor).round() as i32,
        };
        if ClientToScreen(host_hwnd, &mut pt) == 0 {
            return None;
        }

        Some(VideoRect {
            x: pt.x,
            y: pt.y,
            w: (client_w * scale_factor).round().max(1.0) as i32,
            h: (client_h * scale_factor).round().max(1.0) as i32,
        })
    }
}

#[cfg(not(target_os = "windows"))]
pub fn host_video_screen_rect(
    _host_title: &str,
    _client_x: f32,
    _client_y: f32,
    _client_w: f32,
    _client_h: f32,
    _scale_factor: f32,
) -> Option<VideoRect> {
    None
}

/// Computes the full monitor physical rectangle for the monitor hosting Riptide,
/// returning `None` if Riptide is minimized, hidden, or not the foreground app.
#[cfg(target_os = "windows")]
pub fn host_monitor_screen_rect(host_title: &str) -> Option<VideoRect> {
    const MONITOR_DEFAULTTONEAREST: u32 = 2;

    #[repr(C)]
    struct RectW {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[repr(C)]
    struct MonitorInfoW {
        cb_size: u32,
        rc_monitor: RectW,
        rc_work: RectW,
        dw_flags: u32,
    }

    unsafe extern "system" {
        fn IsWindowVisible(hwnd: isize) -> i32;
        fn IsIconic(hwnd: isize) -> i32;
        fn GetForegroundWindow() -> isize;
        fn MonitorFromWindow(hwnd: isize, dw_flags: u32) -> isize;
        fn GetMonitorInfoW(h_monitor: isize, lpmi: *mut MonitorInfoW) -> i32;
    }

    let host_hwnd = find_own_window_by_title(host_title);
    if host_hwnd == 0 {
        return None;
    }

    unsafe {
        if IsWindowVisible(host_hwnd) == 0 || IsIconic(host_hwnd) != 0 {
            return None;
        }
        let fg = GetForegroundWindow();
        let wv_hwnd =
            find_own_window_by_title("Riptide — YouTube Music Webapp (Sign In / Inspector)");
        if fg != host_hwnd && (wv_hwnd == 0 || fg != wv_hwnd) {
            return None;
        }

        let monitor = MonitorFromWindow(host_hwnd, MONITOR_DEFAULTTONEAREST);
        if monitor == 0 {
            return None;
        }
        let mut info = MonitorInfoW {
            cb_size: core::mem::size_of::<MonitorInfoW>() as u32,
            rc_monitor: RectW {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            rc_work: RectW {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            dw_flags: 0,
        };
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return None;
        }

        let w = (info.rc_monitor.right - info.rc_monitor.left).max(1);
        let h = (info.rc_monitor.bottom - info.rc_monitor.top).max(1);
        Some(VideoRect {
            x: info.rc_monitor.left,
            y: info.rc_monitor.top,
            w,
            h,
        })
    }
}

#[cfg(not(target_os = "windows"))]
pub fn host_monitor_screen_rect(_host_title: &str) -> Option<VideoRect> {
    None
}

/// Positions the underlying WebView2 window as a frameless, non-activating, taskbar-hidden
/// video viewport reliably above the Riptide window while Riptide is foreground, or hides it
/// and restores standard window decorations when `rect` is `None`.
#[cfg(target_os = "windows")]
pub fn sync_webview_video_window(wv_hwnd: isize, _host_title: &str, rect: Option<VideoRect>) {
    const GWL_STYLE: i32 = -16;
    const GWL_EXSTYLE: i32 = -20;

    const WS_POPUP: isize = 0x8000_0000u32 as isize;
    const WS_VISIBLE: isize = 0x1000_0000;
    const WS_CLIPCHILDREN: isize = 0x0200_0000;
    const WS_CLIPSIBLINGS: isize = 0x0400_0000;
    const WS_OVERLAPPEDWINDOW: isize = 0x00CF_0000;

    const WS_EX_TOOLWINDOW: isize = 0x0000_0080;
    const WS_EX_NOACTIVATE: isize = 0x0800_0000;
    const WS_EX_APPWINDOW: isize = 0x0004_0000;

    const HWND_TOPMOST: isize = -1;
    const HWND_NOTOPMOST: isize = -2;

    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_SHOWWINDOW: u32 = 0x0040;
    const SWP_FRAMECHANGED: u32 = 0x0020;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOSIZE: u32 = 0x0001;
    const SW_HIDE: i32 = 0;
    const SW_SHOWNOACTIVATE: i32 = 4;

    unsafe extern "system" {
        fn GetWindowLongPtrW(hwnd: isize, n_index: i32) -> isize;
        fn SetWindowLongPtrW(hwnd: isize, n_index: i32, dw_new_long: isize) -> isize;
        fn SetWindowPos(
            hwnd: isize,
            hwnd_insert_after: isize,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            u_flags: u32,
        ) -> i32;
        fn ShowWindow(hwnd: isize, n_cmd_show: i32) -> i32;
    }

    if wv_hwnd == 0 {
        return;
    }

    unsafe {
        if let Some(r) = rect {
            let style = WS_POPUP | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS;
            let ex = (GetWindowLongPtrW(wv_hwnd, GWL_EXSTYLE) & !WS_EX_APPWINDOW)
                | WS_EX_TOOLWINDOW
                | WS_EX_NOACTIVATE;
            SetWindowLongPtrW(wv_hwnd, GWL_STYLE, style);
            SetWindowLongPtrW(wv_hwnd, GWL_EXSTYLE, ex);

            // Riptide is guaranteed foreground while `rect` is `Some` (and hides immediately on
            // blur/minimize/overlay), so HWND_TOPMOST keeps the video cleanly above Riptide even
            // when clicking controls inside Riptide.
            SetWindowPos(
                wv_hwnd,
                HWND_TOPMOST,
                r.x,
                r.y,
                r.w,
                r.h,
                SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_FRAMECHANGED,
            );
            ShowWindow(wv_hwnd, SW_SHOWNOACTIVATE);
        } else {
            ShowWindow(wv_hwnd, SW_HIDE);
            let style = WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN | WS_CLIPSIBLINGS;
            let ex = GetWindowLongPtrW(wv_hwnd, GWL_EXSTYLE) & !(WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE);
            SetWindowLongPtrW(wv_hwnd, GWL_STYLE, style);
            SetWindowLongPtrW(wv_hwnd, GWL_EXSTYLE, ex);
            SetWindowPos(
                wv_hwnd,
                HWND_NOTOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn sync_webview_video_window(_wv_hwnd: isize, _host_title: &str, _rect: Option<VideoRect>) {}
