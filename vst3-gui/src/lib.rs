use std::ffi::{CStr, CString};
use std::path::PathBuf;

extern "C" {
    fn vst3_gui_open(
        path: *const std::ffi::c_char,
        uid: *const std::ffi::c_char,
        title: *const std::ffi::c_char,
    ) -> *mut std::ffi::c_void;
    fn vst3_gui_open_headless(
        path: *const std::ffi::c_char,
        uid: *const std::ffi::c_char,
    ) -> *mut std::ffi::c_void;
    fn vst3_gui_close(handle: *mut std::ffi::c_void);
    fn vst3_gui_destroy(handle: *mut std::ffi::c_void);
    fn vst3_gui_show(handle: *mut std::ffi::c_void);
    fn vst3_gui_is_open(handle: *mut std::ffi::c_void) -> i32;
    fn vst3_gui_get_size(handle: *mut std::ffi::c_void, w: *mut f32, h: *mut f32) -> i32;
    fn vst3_gui_get_state(handle: *mut std::ffi::c_void, data: *mut u8, capacity: i32) -> i32;
    fn vst3_gui_set_state(handle: *mut std::ffi::c_void, data: *const u8, size: i32) -> i32;
    fn vst3_gui_get_parameter_count(handle: *mut std::ffi::c_void) -> i32;
    fn vst3_gui_get_parameter(handle: *mut std::ffi::c_void, index: i32, value: *mut f64) -> i32;
    fn vst3_gui_set_parameter(handle: *mut std::ffi::c_void, index: i32, value: f64) -> i32;
    fn vst3_gui_setup_processing(handle: *mut std::ffi::c_void, sample_rate: f64, block_size: i32) -> i32;
    fn vst3_gui_process(
        handle: *mut std::ffi::c_void,
        inputs: *const *const f32,
        num_input_channels: i32,
        outputs: *mut *mut f32,
        num_output_channels: i32,
        num_frames: i32,
    ) -> i32;
    fn vst3_gui_send_midi(
        handle: *mut std::ffi::c_void,
        midi_data: *const u8,
        num_events: i32,
        sample_offsets: *const i32,
    ) -> i32;
    fn vst3_gui_get_audio_input_channels(handle: *mut std::ffi::c_void) -> i32;
    fn vst3_gui_get_audio_output_channels(handle: *mut std::ffi::c_void) -> i32;

    // Scanning
    fn vst3_gui_scan() -> *mut std::ffi::c_void;
    fn vst3_gui_scan_count(result: *const std::ffi::c_void) -> i32;
    fn vst3_gui_scan_get_name(result: *const std::ffi::c_void, index: i32) -> *const std::ffi::c_char;
    fn vst3_gui_scan_get_vendor(result: *const std::ffi::c_void, index: i32) -> *const std::ffi::c_char;
    fn vst3_gui_scan_get_uid(result: *const std::ffi::c_void, index: i32) -> *const std::ffi::c_char;
    fn vst3_gui_scan_get_path(result: *const std::ffi::c_void, index: i32) -> *const std::ffi::c_char;
    fn vst3_gui_scan_get_subcategories(result: *const std::ffi::c_void, index: i32) -> *const std::ffi::c_char;
    fn vst3_gui_scan_free(result: *mut std::ffi::c_void);
}

pub struct Vst3Gui {
    handle: *mut std::ffi::c_void,
}

// The handle is a pointer to a C++ struct that manages its own thread safety.
// GUI operations must happen on the main thread (macOS/Windows requirement),
// which is where we always call these functions from.
unsafe impl Send for Vst3Gui {}

impl Vst3Gui {
    /// Open a VST3 plugin's native GUI window.
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

    /// Open a VST3 plugin without a GUI window (no native window, works from any thread).
    /// For audio processing, state, and parameters only.
    pub fn open_headless(vst3_path: &str, uid: &str) -> Option<Self> {
        let c_path = CString::new(vst3_path).ok()?;
        let c_uid = CString::new(uid).ok()?;

        let handle =
            unsafe { vst3_gui_open_headless(c_path.as_ptr(), c_uid.as_ptr()) };

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

    /// Hide the plugin window (keeps the plugin instance alive).
    pub fn hide(&self) {
        unsafe { vst3_gui_close(self.handle) }
    }

    /// Show a previously hidden plugin window.
    pub fn show(&self) {
        unsafe { vst3_gui_show(self.handle) }
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

    /// Get the number of parameters on the GUI's controller.
    pub fn parameter_count(&self) -> usize {
        let n = unsafe { vst3_gui_get_parameter_count(self.handle) };
        if n < 0 { 0 } else { n as usize }
    }

    /// Get a normalized parameter value by index.
    pub fn get_parameter(&self, index: usize) -> Option<f64> {
        let mut value: f64 = 0.0;
        let ret = unsafe { vst3_gui_get_parameter(self.handle, index as i32, &mut value) };
        if ret == 0 { Some(value) } else { None }
    }

    /// Set a normalized parameter value by index.
    pub fn set_parameter(&self, index: usize, value: f64) -> bool {
        unsafe { vst3_gui_set_parameter(self.handle, index as i32, value) == 0 }
    }

    /// Get the GUI component's current state as bytes.
    pub fn get_state(&self) -> Option<Vec<u8>> {
        unsafe {
            let size = vst3_gui_get_state(self.handle, std::ptr::null_mut(), 0);
            if size <= 0 {
                return None;
            }
            let mut buf = vec![0u8; size as usize];
            let read = vst3_gui_get_state(self.handle, buf.as_mut_ptr(), size);
            if read <= 0 {
                return None;
            }
            buf.resize(read as usize, 0);
            Some(buf)
        }
    }

    /// Restore component + controller state from bytes previously saved via `get_state()`.
    pub fn set_state(&self, data: &[u8]) -> bool {
        unsafe { vst3_gui_set_state(self.handle, data.as_ptr(), data.len() as i32) == 0 }
    }

    /// Get all parameter values as a Vec of (index, normalized_value) pairs.
    pub fn get_all_parameters(&self) -> Vec<f64> {
        let count = self.parameter_count();
        let mut values = Vec::with_capacity(count);
        for i in 0..count {
            values.push(self.get_parameter(i).unwrap_or(0.0));
        }
        values
    }

    /// Set all parameter values from a slice of normalized values.
    pub fn set_all_parameters(&self, values: &[f64]) {
        let count = self.parameter_count().min(values.len());
        for i in 0..count {
            self.set_parameter(i, values[i]);
        }
    }

    /// Set up audio processing (call once after open, before process).
    pub fn setup_processing(&self, sample_rate: f64, block_size: i32) -> bool {
        unsafe { vst3_gui_setup_processing(self.handle, sample_rate, block_size) == 0 }
    }

    /// Process one audio block. `inputs` and `outputs` are per-channel slices.
    pub fn process(
        &self,
        inputs: &[&[f32]],
        outputs: &mut [&mut [f32]],
        num_frames: usize,
    ) -> bool {
        let input_ptrs: Vec<*const f32> = inputs.iter().map(|s| s.as_ptr()).collect();
        let mut output_ptrs: Vec<*mut f32> = outputs.iter_mut().map(|s| s.as_mut_ptr()).collect();
        unsafe {
            vst3_gui_process(
                self.handle,
                if input_ptrs.is_empty() { std::ptr::null() } else { input_ptrs.as_ptr() },
                input_ptrs.len() as i32,
                if output_ptrs.is_empty() { std::ptr::null_mut() } else { output_ptrs.as_mut_ptr() },
                output_ptrs.len() as i32,
                num_frames as i32,
            ) == 0
        }
    }

    /// Queue a MIDI Note On for the next process() call.
    pub fn send_midi_note_on(&self, note: u8, velocity: u8, channel: u8, sample_offset: i32) {
        let midi_data: [u8; 3] = [0x90 | (channel & 0x0F), note, velocity];
        unsafe {
            vst3_gui_send_midi(self.handle, midi_data.as_ptr(), 1, &sample_offset);
        }
    }

    /// Queue a MIDI Note Off for the next process() call.
    pub fn send_midi_note_off(&self, note: u8, velocity: u8, channel: u8, sample_offset: i32) {
        let midi_data: [u8; 3] = [0x80 | (channel & 0x0F), note, velocity];
        unsafe {
            vst3_gui_send_midi(self.handle, midi_data.as_ptr(), 1, &sample_offset);
        }
    }

    /// Get the number of audio input channels (valid after setup_processing).
    pub fn audio_input_channels(&self) -> usize {
        let n = unsafe { vst3_gui_get_audio_input_channels(self.handle) };
        if n < 0 { 0 } else { n as usize }
    }

    /// Get the number of audio output channels (valid after setup_processing).
    pub fn audio_output_channels(&self) -> usize {
        let n = unsafe { vst3_gui_get_audio_output_channels(self.handle) };
        if n < 0 { 0 } else { n as usize }
    }
}

impl Drop for Vst3Gui {
    fn drop(&mut self) {
        unsafe { vst3_gui_destroy(self.handle) }
    }
}

// ---------------------------------------------------------------------------
// Plugin scanning
// ---------------------------------------------------------------------------

/// A plugin discovered during scanning.
#[derive(Debug, Clone)]
pub struct ScannedPlugin {
    pub name: String,
    pub vendor: String,
    pub uid: String,
    pub path: PathBuf,
    pub subcategories: String,
}

impl ScannedPlugin {
    /// Returns true if the subcategories indicate this is an instrument.
    pub fn is_instrument(&self) -> bool {
        let sc = self.subcategories.to_ascii_lowercase();
        sc.contains("instrument") || sc.contains("synth") || sc.contains("sampler")
    }

    /// Returns true if the subcategories indicate this is an effect (or "Other").
    pub fn is_effect(&self) -> bool {
        !self.is_instrument()
    }
}

/// Scan for all installed VST3 plugins.
/// Returns an empty Vec if no plugins are found or scanning fails.
pub fn scan_plugins() -> Vec<ScannedPlugin> {
    unsafe {
        let result = vst3_gui_scan();
        if result.is_null() {
            return Vec::new();
        }

        let count = vst3_gui_scan_count(result);
        let mut plugins = Vec::with_capacity(count.max(0) as usize);

        for i in 0..count {
            let name = c_ptr_to_string(vst3_gui_scan_get_name(result, i));
            let vendor = c_ptr_to_string(vst3_gui_scan_get_vendor(result, i));
            let uid = c_ptr_to_string(vst3_gui_scan_get_uid(result, i));
            let path_str = c_ptr_to_string(vst3_gui_scan_get_path(result, i));
            let subcategories = c_ptr_to_string(vst3_gui_scan_get_subcategories(result, i));

            plugins.push(ScannedPlugin {
                name,
                vendor,
                uid,
                path: PathBuf::from(path_str),
                subcategories,
            });
        }

        vst3_gui_scan_free(result);
        plugins
    }
}

unsafe fn c_ptr_to_string(ptr: *const std::ffi::c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned()
}
