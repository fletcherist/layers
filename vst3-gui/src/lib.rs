use std::ffi::CString;

extern "C" {
    fn vst3_gui_open(
        path: *const std::ffi::c_char,
        uid: *const std::ffi::c_char,
        title: *const std::ffi::c_char,
    ) -> *mut std::ffi::c_void;
    fn vst3_gui_close(handle: *mut std::ffi::c_void);
    fn vst3_gui_is_open(handle: *mut std::ffi::c_void) -> i32;
    fn vst3_gui_get_size(handle: *mut std::ffi::c_void, w: *mut f32, h: *mut f32) -> i32;
}

pub struct Vst3Gui {
    handle: *mut std::ffi::c_void,
}

// The handle is a pointer to a C++ struct that manages its own thread safety.
// GUI operations must happen on the main thread (macOS requirement), which is
// where we always call these functions from.
unsafe impl Send for Vst3Gui {}

impl Vst3Gui {
    /// Open a VST3 plugin's native GUI window.
    ///
    /// - `vst3_path`: path to the .vst3 bundle
    /// - `uid`: plugin unique ID (32 hex chars)
    /// - `title`: window title
    ///
    /// Returns `None` if the plugin has no GUI or loading fails.
    pub fn open(vst3_path: &str, uid: &str, title: &str) -> Option<Self> {
        let c_path = CString::new(vst3_path).ok()?;
        let c_uid = CString::new(uid).ok()?;
        let c_title = CString::new(title).ok()?;

        let handle =
            unsafe { vst3_gui_open(c_path.as_ptr(), c_uid.as_ptr(), c_title.as_ptr()) };

        if handle.is_null() {
            None
        } else {
            Some(Vst3Gui { handle })
        }
    }

    /// Returns true if the plugin window is still visible.
    pub fn is_open(&self) -> bool {
        unsafe { vst3_gui_is_open(self.handle) != 0 }
    }

    /// Get the current view size, or `None` on error.
    pub fn get_size(&self) -> Option<(f32, f32)> {
        let mut w: f32 = 0.0;
        let mut h: f32 = 0.0;
        let ret = unsafe { vst3_gui_get_size(self.handle, &mut w, &mut h) };
        if ret == 0 {
            Some((w, h))
        } else {
            None
        }
    }
}

impl Drop for Vst3Gui {
    fn drop(&mut self) {
        unsafe { vst3_gui_close(self.handle) }
    }
}
