#ifndef VST3_GUI_INTERNAL_H
#define VST3_GUI_INTERNAL_H

#include "pluginterfaces/base/funknown.h"
#include "pluginterfaces/base/ipluginbase.h"
#include "pluginterfaces/vst/ivstaudioprocessor.h"
#include "pluginterfaces/vst/ivsteditcontroller.h"
#include "pluginterfaces/vst/ivstmessage.h"
#include "pluginterfaces/vst/ivstevents.h"
#include "pluginterfaces/gui/iplugview.h"
#include "pluginterfaces/base/ustring.h"
#include "public.sdk/source/vst/hosting/module.h"
#include "public.sdk/source/vst/hosting/hostclasses.h"
#include "public.sdk/source/vst/hosting/plugprovider.h"
#include "public.sdk/source/vst/hosting/connectionproxy.h"
#include "public.sdk/source/common/memorystream.h"

#include "vst3_gui.h"

#include <string>
#include <cstring>
#include <vector>
#include <mutex>
#include <atomic>

using namespace Steinberg;
using namespace Steinberg::Vst;

// --- Minimal IComponentHandler so plugin can notify host of param changes ---

class ComponentHandlerImpl : public IComponentHandler {
public:
    ComponentHandlerImpl() : refCount(1) {}

    tresult PLUGIN_API beginEdit(ParamID /*id*/) override { return kResultOk; }
    tresult PLUGIN_API performEdit(ParamID /*id*/, ParamValue /*valueNormalized*/) override { return kResultOk; }
    tresult PLUGIN_API endEdit(ParamID /*id*/) override { return kResultOk; }
    tresult PLUGIN_API restartComponent(int32 /*flags*/) override { return kResultOk; }

    tresult PLUGIN_API queryInterface(const TUID _iid, void** obj) override {
        if (FUnknownPrivate::iidEqual(_iid, IComponentHandler::iid) ||
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
};

// --- Minimal IEventList for MIDI events ---

class EventListImpl : public IEventList {
public:
    EventListImpl() : refCount(1) {}

    int32 PLUGIN_API getEventCount() override { return (int32)events.size(); }

    tresult PLUGIN_API getEvent(int32 index, Event& e) override {
        if (index < 0 || index >= (int32)events.size()) return kResultFalse;
        e = events[index];
        return kResultOk;
    }

    tresult PLUGIN_API addEvent(Event& e) override {
        events.push_back(e);
        return kResultOk;
    }

    void clear() { events.clear(); }

    tresult PLUGIN_API queryInterface(const TUID _iid, void** obj) override {
        if (FUnknownPrivate::iidEqual(_iid, IEventList::iid) ||
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

    std::vector<Event> events;
private:
    std::atomic<uint32> refCount;
};

// --- Handle struct (platform-neutral) ---

struct Vst3GuiHandle {
    VST3::Hosting::Module::Ptr module;
    IPtr<IComponent> component;
    IPtr<IEditController> controller;
    IPtr<IPlugView> view;
    void* window = nullptr;         // NSWindow* (bridged) on macOS, HWND on Windows
    void* containerView = nullptr;  // NSView* (bridged) on macOS, unused on Windows
    void* plugFrame = nullptr;      // Platform-specific IPlugFrame* subclass
    ComponentHandlerImpl* componentHandler = nullptr;
    bool isSingleComponent = false;
    IPtr<ConnectionProxy> componentCP;
    IPtr<ConnectionProxy> controllerCP;
    IAudioProcessor* processor = nullptr;
    int inputChannels = 0;
    int outputChannels = 0;
    bool processingSetUp = false;
    std::vector<Event> pendingMidiEvents;
    std::mutex midiMutex;
};

// --- Common helpers ---

Steinberg::Vst::HostApplication& getHostApp();
std::string normalize_uid(const std::string& s);

// Platform helpers (implemented in vst3_gui_mac.mm / vst3_gui_win.cpp)
void platform_close_window(Vst3GuiHandle* handle);
void platform_destroy_window(Vst3GuiHandle* handle);

#endif // VST3_GUI_INTERNAL_H
