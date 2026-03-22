#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include "vst3_gui_internal.h"

// --- Window class registration (once per process) ---

static const wchar_t* VST3_WND_CLASS = L"Vst3GuiPluginWindow";
static bool sWindowClassRegistered = false;

static LRESULT CALLBACK Vst3WndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    switch (msg) {
    case WM_CLOSE:
        ShowWindow(hwnd, SW_HIDE);
        return 0; // Hide instead of destroy — matches macOS behavior
    default:
        return DefWindowProcW(hwnd, msg, wParam, lParam);
    }
}

static bool ensureWindowClass() {
    if (sWindowClassRegistered) return true;

    WNDCLASSEXW wc = {};
    wc.cbSize = sizeof(wc);
    wc.style = CS_HREDRAW | CS_VREDRAW;
    wc.lpfnWndProc = Vst3WndProc;
    wc.hInstance = GetModuleHandleW(nullptr);
    wc.hCursor = LoadCursor(nullptr, IDC_ARROW);
    wc.hbrBackground = (HBRUSH)(COLOR_WINDOW + 1);
    wc.lpszClassName = VST3_WND_CLASS;

    if (RegisterClassExW(&wc)) {
        sWindowClassRegistered = true;
        return true;
    }

    // May already be registered from a previous load
    if (GetLastError() == ERROR_CLASS_ALREADY_EXISTS) {
        sWindowClassRegistered = true;
        return true;
    }

    fprintf(stderr, "vst3_gui_win: failed to register window class\n");
    return false;
}

// --- PlugFrame implementation for resize support (Windows) ---

class WinPlugFrameImpl : public IPlugFrame {
public:
    WinPlugFrameImpl(HWND hwnd) : refCount(1), hwnd_(hwnd) {}

    tresult PLUGIN_API resizeView(IPlugView* view, ViewRect* newSize) override {
        if (!newSize || !hwnd_) return kResultFalse;

        int w = newSize->right - newSize->left;
        int h = newSize->bottom - newSize->top;

        // Adjust for window chrome (title bar, borders)
        RECT windowRect = { 0, 0, w, h };
        DWORD style = (DWORD)GetWindowLongW(hwnd_, GWL_STYLE);
        DWORD exStyle = (DWORD)GetWindowLongW(hwnd_, GWL_EXSTYLE);
        AdjustWindowRectEx(&windowRect, style, FALSE, exStyle);

        SetWindowPos(hwnd_, nullptr, 0, 0,
                     windowRect.right - windowRect.left,
                     windowRect.bottom - windowRect.top,
                     SWP_NOMOVE | SWP_NOZORDER);

        return kResultOk;
    }

    tresult PLUGIN_API queryInterface(const TUID _iid, void** obj) override {
        if (FUnknownPrivate::iidEqual(_iid, IPlugFrame::iid) ||
            FUnknownPrivate::iidEqual(_iid, FUnknown::iid)) {
            *obj = this;
            addRef();
            return kResultOk;
        }
        *obj = nullptr;
        return kNoInterface;
    }

    uint32 PLUGIN_API addRef() override { return ++refCount; }
    uint32 PLUGIN_API release() override {
        if (--refCount == 0) { delete this; return 0; }
        return refCount;
    }

private:
    std::atomic<uint32> refCount;
    HWND hwnd_;
};

// --- Platform helpers ---

void platform_close_window(Vst3GuiHandle* handle) {
    if (!handle || !handle->window) return;
    HWND hwnd = (HWND)handle->window;
    if (IsWindowVisible(hwnd)) {
        ShowWindow(hwnd, SW_HIDE);
    }
}

void platform_destroy_window(Vst3GuiHandle* handle) {
    if (!handle) return;

    // Release plug frame
    if (handle->plugFrame) {
        auto* pf = (WinPlugFrameImpl*)handle->plugFrame;
        pf->release();
        handle->plugFrame = nullptr;
    }

    // Destroy HWND
    if (handle->window) {
        DestroyWindow((HWND)handle->window);
        handle->window = nullptr;
    }
}

// --- GUI open (Windows) ---

extern "C" {

Vst3GuiHandle* vst3_gui_open(const char* vst3_path, const char* uid_str, const char* title) {
    if (!vst3_path || !uid_str) return nullptr;

    if (!ensureWindowClass()) return nullptr;

    try {
    std::string errMsg;

    // 1. Load VST3 module
    auto module = VST3::Hosting::Module::create(vst3_path, errMsg);
    if (!module) {
        fprintf(stderr, "vst3_gui: failed to load module '%s': %s\n", vst3_path, errMsg.c_str());
        return nullptr;
    }

    auto factory = module->getFactory();
    if (!factory.get()) {
        fprintf(stderr, "vst3_gui: no factory in module\n");
        return nullptr;
    }

    // 2. Find and create IComponent + IEditController
    IPtr<IComponent> component;
    IPtr<IEditController> controller;
    bool isSingleComponent = false;

    std::string targetNorm = normalize_uid(std::string(uid_str));

    for (auto& classInfo : factory.classInfos()) {
        if (classInfo.category() != kVstAudioEffectClass) continue;

        std::string classNorm = normalize_uid(classInfo.ID().toString());
        if (targetNorm != classNorm) continue;

        fprintf(stderr, "vst3_gui: found matching class '%s'\n", classInfo.name().c_str());

        component = factory.createInstance<IComponent>(classInfo.ID());
        if (!component) {
            fprintf(stderr, "vst3_gui: failed to create IComponent\n");
            continue;
        }

        if (component->initialize(&getHostApp()) != kResultOk) {
            fprintf(stderr, "vst3_gui: failed to initialize IComponent\n");
            component = nullptr;
            continue;
        }

        if (component->queryInterface(IEditController::iid, (void**)&controller) == kResultTrue) {
            isSingleComponent = true;
            fprintf(stderr, "vst3_gui: using single-component controller\n");
        } else {
            TUID controllerCID;
            if (component->getControllerClassId(controllerCID) == kResultTrue) {
                controller = factory.createInstance<IEditController>(VST3::UID(controllerCID));
                if (controller) {
                    if (controller->initialize(&getHostApp()) != kResultOk) {
                        fprintf(stderr, "vst3_gui: failed to initialize controller\n");
                        controller = nullptr;
                    } else {
                        fprintf(stderr, "vst3_gui: created separate controller\n");
                    }
                }
            }
        }

        if (controller) break;
        component->terminate();
        component = nullptr;
    }

    if (!controller) {
        fprintf(stderr, "vst3_gui: no IEditController found for uid '%s'\n", uid_str);
        if (component) component->terminate();
        return nullptr;
    }

    // 3. Set component handler
    auto* componentHandler = new ComponentHandlerImpl();
    controller->setComponentHandler(componentHandler);

    // 4. Connect component ↔ controller
    IPtr<ConnectionProxy> componentCP;
    IPtr<ConnectionProxy> controllerCP;

    if (!isSingleComponent) {
        auto compICP = U::cast<IConnectionPoint>(component);
        auto ctrlICP = U::cast<IConnectionPoint>(controller);
        if (compICP && ctrlICP) {
            componentCP = owned(new ConnectionProxy(compICP));
            controllerCP = owned(new ConnectionProxy(ctrlICP));
            componentCP->connect(ctrlICP);
            controllerCP->connect(compICP);
            fprintf(stderr, "vst3_gui: connected component ↔ controller\n");
        }
    }

    // 5. Synchronize component state → controller
    {
        MemoryStream stream;
        if (component->getState(&stream) == kResultOk) {
            stream.seek(0, IBStream::kIBSeekSet, nullptr);
            controller->setComponentState(&stream);
            fprintf(stderr, "vst3_gui: synced component state to controller\n");
        }
    }

    // 6. Create view
    IPtr<IPlugView> view = owned(controller->createView("editor"));
    if (!view) {
        fprintf(stderr, "vst3_gui: plugin has no editor view\n");
        if (componentCP) { componentCP->disconnect(); controllerCP->disconnect(); }
        if (!isSingleComponent && controller) controller->terminate();
        if (component) component->terminate();
        componentHandler->release();
        return nullptr;
    }

    // Check platform support
    if (view->isPlatformTypeSupported(kPlatformTypeHWND) != kResultOk) {
        fprintf(stderr, "vst3_gui: plugin view does not support HWND\n");
        if (componentCP) { componentCP->disconnect(); controllerCP->disconnect(); }
        if (!isSingleComponent && controller) controller->terminate();
        if (component) component->terminate();
        componentHandler->release();
        return nullptr;
    }

    // 7. Get initial size
    ViewRect rect = {};
    if (view->getSize(&rect) != kResultOk) {
        rect.left = 0; rect.top = 0;
        rect.right = 800; rect.bottom = 600;
    }

    int viewW = rect.right - rect.left;
    int viewH = rect.bottom - rect.top;
    if (viewW < 100) viewW = 800;
    if (viewH < 100) viewH = 600;

    // 8. Create HWND
    DWORD style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
    DWORD exStyle = 0;

    RECT windowRect = { 0, 0, viewW, viewH };
    AdjustWindowRectEx(&windowRect, style, FALSE, exStyle);

    int winW = windowRect.right - windowRect.left;
    int winH = windowRect.bottom - windowRect.top;

    // Convert title to wide string
    std::wstring wTitle;
    if (title) {
        int len = MultiByteToWideChar(CP_UTF8, 0, title, -1, nullptr, 0);
        wTitle.resize(len);
        MultiByteToWideChar(CP_UTF8, 0, title, -1, &wTitle[0], len);
    } else {
        wTitle = L"VST3 Plugin";
    }

    HWND hwnd = CreateWindowExW(
        exStyle,
        VST3_WND_CLASS,
        wTitle.c_str(),
        style,
        CW_USEDEFAULT, CW_USEDEFAULT,
        winW, winH,
        nullptr, nullptr,
        GetModuleHandleW(nullptr),
        nullptr
    );

    if (!hwnd) {
        fprintf(stderr, "vst3_gui: failed to create HWND\n");
        if (componentCP) { componentCP->disconnect(); controllerCP->disconnect(); }
        if (!isSingleComponent && controller) controller->terminate();
        if (component) component->terminate();
        componentHandler->release();
        return nullptr;
    }

    // 9. Set up plug frame for resize
    auto* plugFrame = new WinPlugFrameImpl(hwnd);
    view->setFrame(plugFrame);

    // 10. Attach view to HWND
    if (view->attached((void*)hwnd, kPlatformTypeHWND) != kResultOk) {
        fprintf(stderr, "vst3_gui: failed to attach view to HWND\n");
        view->setFrame(nullptr);
        plugFrame->release();
        if (componentCP) { componentCP->disconnect(); controllerCP->disconnect(); }
        if (!isSingleComponent && controller) controller->terminate();
        if (component) component->terminate();
        componentHandler->release();
        DestroyWindow(hwnd);
        return nullptr;
    }

    // 11. Show window
    ShowWindow(hwnd, SW_SHOW);
    UpdateWindow(hwnd);

    // 12. Build handle
    auto* handle = new Vst3GuiHandle();
    handle->module = module;
    handle->component = component;
    handle->controller = controller;
    handle->view = view;
    handle->window = (void*)hwnd;
    handle->containerView = nullptr;  // Not needed on Windows
    handle->plugFrame = plugFrame;
    handle->componentHandler = componentHandler;
    handle->isSingleComponent = isSingleComponent;
    handle->componentCP = componentCP;
    handle->controllerCP = controllerCP;

    fprintf(stderr, "vst3_gui: opened GUI for '%s' (%dx%d)\n",
            title ? title : "?", viewW, viewH);

    return handle;

    } catch (const std::exception& e) {
        fprintf(stderr, "vst3_gui_open: C++ exception: %s\n", e.what());
        return nullptr;
    } catch (...) {
        fprintf(stderr, "vst3_gui_open: unknown C++ exception\n");
        return nullptr;
    }
}

void vst3_gui_close(Vst3GuiHandle* handle) {
    if (!handle) return;
    platform_close_window(handle);
    fprintf(stderr, "vst3_gui_close: window hidden\n");
}

void vst3_gui_show(Vst3GuiHandle* handle) {
    if (!handle || !handle->window) return;
    HWND hwnd = (HWND)handle->window;
    ShowWindow(hwnd, SW_SHOW);
    SetForegroundWindow(hwnd);
    fprintf(stderr, "vst3_gui_show: window shown\n");
}

int vst3_gui_is_open(Vst3GuiHandle* handle) {
    if (!handle || !handle->window) return 0;
    HWND hwnd = (HWND)handle->window;
    return IsWindowVisible(hwnd) ? 1 : 0;
}

} // extern "C"
