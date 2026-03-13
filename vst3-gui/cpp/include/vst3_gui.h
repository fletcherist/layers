#ifndef VST3_GUI_H
#define VST3_GUI_H

#ifdef __cplusplus
extern "C" {
#endif

typedef struct Vst3GuiHandle Vst3GuiHandle;

// Create GUI from a .vst3 bundle path and plugin UID string.
// Returns NULL if plugin has no GUI or loading fails.
Vst3GuiHandle* vst3_gui_open(const char* vst3_path, const char* uid, const char* title);

// Close and destroy GUI.
void vst3_gui_close(Vst3GuiHandle* handle);

// Check if window is still open. Returns 1 if open, 0 if closed.
int vst3_gui_is_open(Vst3GuiHandle* handle);

// Get view size. Returns 0 on success, -1 on error.
int vst3_gui_get_size(Vst3GuiHandle* handle, float* width, float* height);

#ifdef __cplusplus
}
#endif

#endif // VST3_GUI_H
