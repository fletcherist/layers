#ifndef VST3_GUI_H
#define VST3_GUI_H

#ifdef __cplusplus
extern "C" {
#endif

typedef struct Vst3GuiHandle Vst3GuiHandle;

// Create GUI from a .vst3 bundle path and plugin UID string.
// Returns NULL if plugin has no GUI or loading fails.
Vst3GuiHandle* vst3_gui_open(const char* vst3_path, const char* uid, const char* title);

// Open plugin without GUI window (no native window, works from any thread).
// For audio processing, state, and parameters only. Call vst3_gui_open to get a window later.
Vst3GuiHandle* vst3_gui_open_headless(const char* vst3_path, const char* uid);

// Hide the GUI window (does NOT destroy the plugin instance).
void vst3_gui_close(Vst3GuiHandle* handle);

// Destroy the GUI handle: detach view, terminate component/controller, close window.
void vst3_gui_destroy(Vst3GuiHandle* handle);

// Show a previously hidden GUI window.
void vst3_gui_show(Vst3GuiHandle* handle);

// Check if window is still visible. Returns 1 if visible, 0 if hidden/closed.
int vst3_gui_is_open(Vst3GuiHandle* handle);

// Get view size. Returns 0 on success, -1 on error.
int vst3_gui_get_size(Vst3GuiHandle* handle, float* width, float* height);

// Get component state as a byte buffer. Returns number of bytes written, or -1 on error.
// Pass NULL/0 to query required size.
int vst3_gui_get_state(Vst3GuiHandle* handle, unsigned char* data, int capacity);

// Restore component + controller state from a byte buffer (previously obtained via get_state).
// Returns 0 on success, -1 on error.
int vst3_gui_set_state(Vst3GuiHandle* handle, const unsigned char* data, int size);

// Get total number of parameters on the controller.
int vst3_gui_get_parameter_count(Vst3GuiHandle* handle);

// Get normalized parameter value by index. Returns 0 on success, -1 on error.
int vst3_gui_get_parameter(Vst3GuiHandle* handle, int index, double* value);

// Set normalized parameter value by index. Returns 0 on success, -1 on error.
int vst3_gui_set_parameter(Vst3GuiHandle* handle, int index, double value);

// --- Audio processing (instruments) ---

// Set up audio processing. Returns 0 on success, -1 on error.
int vst3_gui_setup_processing(Vst3GuiHandle* handle, double sample_rate, int block_size);

// Process audio. Returns 0 on success, -1 on error.
int vst3_gui_process(Vst3GuiHandle* handle,
                     const float* const* inputs, int num_input_channels,
                     float** outputs, int num_output_channels,
                     int num_frames);

// Queue MIDI events for the next process() call.
// midi_data: packed [status, data1, data2] triples (3 bytes per event).
// sample_offsets: per-event sample offset within the next block.
int vst3_gui_send_midi(Vst3GuiHandle* handle,
                       const unsigned char* midi_data, int num_events,
                       const int* sample_offsets);

// Query audio bus channel counts (valid after setup_processing).
int vst3_gui_get_audio_input_channels(Vst3GuiHandle* handle);
int vst3_gui_get_audio_output_channels(Vst3GuiHandle* handle);

// --- Plugin scanning ---

// Opaque scan result handle.
typedef struct Vst3ScanResult Vst3ScanResult;

// Scan for installed VST3 plugins. Returns NULL on failure.
Vst3ScanResult* vst3_gui_scan(void);

// Number of plugins found.
int vst3_gui_scan_count(const Vst3ScanResult* result);

// Access fields of the i-th plugin. Returns "" if index is out of range.
const char* vst3_gui_scan_get_name(const Vst3ScanResult* result, int index);
const char* vst3_gui_scan_get_vendor(const Vst3ScanResult* result, int index);
const char* vst3_gui_scan_get_uid(const Vst3ScanResult* result, int index);
const char* vst3_gui_scan_get_path(const Vst3ScanResult* result, int index);
const char* vst3_gui_scan_get_subcategories(const Vst3ScanResult* result, int index);

// Free scan result.
void vst3_gui_scan_free(Vst3ScanResult* result);

#ifdef __cplusplus
}
#endif

#endif // VST3_GUI_H
