/// Sets the embedded Windows `.ico` resource (#1, our symmetrical Y logo)
/// as both the titlebar (`ICON_SMALL`) and taskbar/Alt-Tab (`ICON_BIG`) icon
/// for our native GPUI window.
#[cfg(target_os = "windows")]
pub fn apply_window_icon_by_title(title: &str) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    const IMAGE_ICON: u32 = 1;
    const LR_DEFAULTSIZE: u32 = 0x0040;
    const LR_SHARED: u32 = 0x8000;
    const WM_SETICON: u32 = 0x0080;
    const ICON_SMALL: usize = 0;
    const ICON_BIG: usize = 1;

    unsafe extern "system" {
        fn GetModuleHandleW(lp_module_name: *const u16) -> *mut core::ffi::c_void;
        fn LoadImageW(
            h_inst: *mut core::ffi::c_void,
            name: *const u16,
            img_type: u32,
            cx: i32,
            cy: i32,
            fu_load: u32,
        ) -> *mut core::ffi::c_void;
        fn FindWindowW(
            lp_class_name: *const u16,
            lp_window_name: *const u16,
        ) -> *mut core::ffi::c_void;
        fn GetActiveWindow() -> *mut core::ffi::c_void;
        fn SendMessageW(
            hwnd: *mut core::ffi::c_void,
            msg: u32,
            wparam: usize,
            lparam: isize,
        ) -> isize;
    }

    let wide_title: Vec<u16> = OsStr::new(title).encode_wide().chain(Some(0)).collect();

    unsafe {
        let hinst = GetModuleHandleW(core::ptr::null());
        let res_id = 1usize as *const u16;
        let big_icon = LoadImageW(
            hinst,
            res_id,
            IMAGE_ICON,
            48,
            48,
            LR_DEFAULTSIZE | LR_SHARED,
        );
        let small_icon = LoadImageW(hinst, res_id, IMAGE_ICON, 16, 16, LR_SHARED);

        let mut hwnd = FindWindowW(core::ptr::null(), wide_title.as_ptr());
        if hwnd.is_null() {
            hwnd = GetActiveWindow();
        }
        if !hwnd.is_null() {
            if !big_icon.is_null() {
                SendMessageW(hwnd, WM_SETICON, ICON_BIG, big_icon as isize);
            }
            if !small_icon.is_null() {
                SendMessageW(hwnd, WM_SETICON, ICON_SMALL, small_icon as isize);
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn apply_window_icon_by_title(_title: &str) {}
