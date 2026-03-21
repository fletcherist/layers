#import <Cocoa/Cocoa.h>
#include "vst3_gui_internal.h"

// --- PlugFrame implementation for resize support (macOS) ---

class MacPlugFrameImpl : public IPlugFrame {
public:
    MacPlugFrameImpl(NSWindow* window, NSView* container)
        : refCount(1), window_(window), container_(container) {}

    tresult PLUGIN_API resizeView(IPlugView* view, ViewRect* newSize) override {
        if (!newSize || !window_ || !container_) return kResultFalse;

        float w = static_cast<float>(newSize->right - newSize->left);
        float h = static_cast<float>(newSize->bottom - newSize->top);

        NSRect frame = [window_ frame];
        NSRect contentRect = [window_ contentRectForFrameRect:frame];

        float deltaW = w - contentRect.size.width;
        float deltaH = h - contentRect.size.height;

        frame.size.width += deltaW;
        frame.size.height += deltaH;
        frame.origin.y -= deltaH;

        [window_ setFrame:frame display:YES animate:NO];
        [container_ setFrameSize:NSMakeSize(w, h)];

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
    NSWindow* window_;
    NSView* container_;
};

// --- Platform helpers ---

void platform_close_window(Vst3GuiHandle* handle) {
    if (!handle || !handle->window) return;
    NSWindow* window = (__bridge NSWindow*)handle->window;
    if ([window isVisible]) {
        [window orderOut:nil];
    }
}

void platform_destroy_window(Vst3GuiHandle* handle) {
    if (!handle) return;

    // Release plug frame
    if (handle->plugFrame) {
        auto* pf = (MacPlugFrameImpl*)handle->plugFrame;
        pf->release();
        handle->plugFrame = nullptr;
    }

    // Close and release window (transfers ownership back to ARC)
    if (handle->window) {
        NSWindow* window = (__bridge_transfer NSWindow*)handle->window;
        [window setContentView:nil];
        [window close];
        handle->window = nullptr;
    }

    // Release container view
    if (handle->containerView) {
        NSView* view __attribute__((unused)) = (__bridge_transfer NSView*)handle->containerView;
        handle->containerView = nullptr;
    }
}

// --- GUI open (macOS) ---

extern "C" {

Vst3GuiHandle* vst3_gui_open(const char* vst3_path, const char* uid_str, const char* title) {
    if (!vst3_path || !uid_str) return nullptr;

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
    if (view->isPlatformTypeSupported(kPlatformTypeNSView) != kResultOk) {
        fprintf(stderr, "vst3_gui: plugin view does not support NSView\n");
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

    float viewW = static_cast<float>(rect.right - rect.left);
    float viewH = static_cast<float>(rect.bottom - rect.top);
    if (viewW < 100) viewW = 800;
    if (viewH < 100) viewH = 600;

    // 8. Create NSWindow
    NSString* nsTitle = title ? [NSString stringWithUTF8String:title]
                              : @"VST3 Plugin";

    NSRect contentRect = NSMakeRect(200, 200, viewW, viewH);
    NSUInteger styleMask = NSWindowStyleMaskTitled | NSWindowStyleMaskClosable
                         | NSWindowStyleMaskMiniaturizable;

    NSWindow* window = [[NSWindow alloc] initWithContentRect:contentRect
                                                   styleMask:styleMask
                                                     backing:NSBackingStoreBuffered
                                                       defer:NO];
    [window setTitle:nsTitle];
    [window setReleasedWhenClosed:NO];

    // 9. Create container view
    NSView* containerView = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, viewW, viewH)];
    [window setContentView:containerView];

    // 10. Set up plug frame for resize
    auto* plugFrame = new MacPlugFrameImpl(window, containerView);
    view->setFrame(plugFrame);

    // 11. Attach view to NSView
    if (view->attached((__bridge void*)containerView, kPlatformTypeNSView) != kResultOk) {
        fprintf(stderr, "vst3_gui: failed to attach view\n");
        view->setFrame(nullptr);
        plugFrame->release();
        if (componentCP) { componentCP->disconnect(); controllerCP->disconnect(); }
        if (!isSingleComponent && controller) controller->terminate();
        if (component) component->terminate();
        componentHandler->release();
        [window close];
        return nullptr;
    }

    // 12. Show window
    [window center];
    [window makeKeyAndOrderFront:nil];

    // 13. Build handle
    auto* handle = new Vst3GuiHandle();
    handle->module = module;
    handle->component = component;
    handle->controller = controller;
    handle->view = view;
    handle->window = (__bridge_retained void*)window;
    handle->containerView = (__bridge_retained void*)containerView;
    handle->plugFrame = plugFrame;
    handle->componentHandler = componentHandler;
    handle->isSingleComponent = isSingleComponent;
    handle->componentCP = componentCP;
    handle->controllerCP = controllerCP;

    fprintf(stderr, "vst3_gui: opened GUI for '%s' (%gx%g)\n",
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
    NSWindow* window = (__bridge NSWindow*)handle->window;
    [window makeKeyAndOrderFront:nil];
    fprintf(stderr, "vst3_gui_show: window shown\n");
}

int vst3_gui_is_open(Vst3GuiHandle* handle) {
    if (!handle || !handle->window) return 0;
    NSWindow* window = (__bridge NSWindow*)handle->window;
    return [window isVisible] ? 1 : 0;
}

} // extern "C"
