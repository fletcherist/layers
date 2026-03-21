#include "vst3_gui_internal.h"

Steinberg::Vst::HostApplication& getHostApp() {
    static Steinberg::Vst::HostApplication sHostApp;
    return sHostApp;
}

std::string normalize_uid(const std::string& s) {
    std::string result;
    for (char c : s) {
        if (isxdigit(c)) result += toupper(c);
    }
    return result;
}

extern "C" {

// ---------------------------------------------------------------------------
// Headless open (no window, works on all platforms)
// ---------------------------------------------------------------------------

Vst3GuiHandle* vst3_gui_open_headless(const char* vst3_path, const char* uid_str) {
    if (!vst3_path || !uid_str) return nullptr;

    try {
    std::string errMsg;
    auto module = VST3::Hosting::Module::create(vst3_path, errMsg);
    if (!module) return nullptr;

    auto factory = module->getFactory();
    if (!factory.get()) return nullptr;

    IPtr<IComponent> component;
    IPtr<IEditController> controller;
    bool isSingleComponent = false;

    std::string targetNorm = normalize_uid(std::string(uid_str));

    for (auto& classInfo : factory.classInfos()) {
        if (classInfo.category() != kVstAudioEffectClass) continue;
        if (normalize_uid(classInfo.ID().toString()) != targetNorm) continue;

        component = factory.createInstance<IComponent>(classInfo.ID());
        if (!component) continue;
        if (component->initialize(&getHostApp()) != kResultOk) { component = nullptr; continue; }

        if (component->queryInterface(IEditController::iid, (void**)&controller) == kResultTrue) {
            isSingleComponent = true;
        } else {
            TUID cid;
            if (component->getControllerClassId(cid) == kResultTrue) {
                controller = factory.createInstance<IEditController>(VST3::UID(cid));
                if (controller && controller->initialize(&getHostApp()) != kResultOk) controller = nullptr;
            }
        }
        if (controller) break;
        component->terminate();
        component = nullptr;
    }

    if (!controller) { if (component) component->terminate(); return nullptr; }

    auto* ch = new ComponentHandlerImpl();
    controller->setComponentHandler(ch);

    IPtr<ConnectionProxy> componentCP, controllerCP;
    if (!isSingleComponent) {
        auto compICP = U::cast<IConnectionPoint>(component);
        auto ctrlICP = U::cast<IConnectionPoint>(controller);
        if (compICP && ctrlICP) {
            componentCP = owned(new ConnectionProxy(compICP));
            controllerCP = owned(new ConnectionProxy(ctrlICP));
            componentCP->connect(ctrlICP);
            controllerCP->connect(compICP);
        }
    }

    { MemoryStream s; if (component->getState(&s) == kResultOk) { s.seek(0, IBStream::kIBSeekSet, nullptr); controller->setComponentState(&s); } }

    auto* handle = new Vst3GuiHandle();
    handle->module = module;
    handle->component = component;
    handle->controller = controller;
    handle->view = nullptr;
    handle->window = nullptr;
    handle->containerView = nullptr;
    handle->plugFrame = nullptr;
    handle->componentHandler = ch;
    handle->isSingleComponent = isSingleComponent;
    handle->componentCP = componentCP;
    handle->controllerCP = controllerCP;
    return handle;

    } catch (...) {
        fprintf(stderr, "vst3_gui_open_headless: exception\n");
        return nullptr;
    }
}

// ---------------------------------------------------------------------------
// Destroy (common cleanup, calls platform helpers for window)
// ---------------------------------------------------------------------------

void vst3_gui_destroy(Vst3GuiHandle* handle) {
    if (!handle) return;

    try {
        // 1. Hide window
        platform_close_window(handle);

        // 2. Detach the view from the container
        if (handle->view) {
            handle->view->setFrame(nullptr);
            handle->view->removed();
        }

        // 3. Destroy platform window and plug frame
        platform_destroy_window(handle);

        // 4. Release the view
        handle->view = nullptr;

        // 5. Disconnect connection proxies
        if (handle->componentCP) {
            handle->componentCP->disconnect();
            handle->componentCP = nullptr;
        }
        if (handle->controllerCP) {
            handle->controllerCP->disconnect();
            handle->controllerCP = nullptr;
        }

        // 6. Release component handler
        if (handle->componentHandler) {
            handle->componentHandler->release();
            handle->componentHandler = nullptr;
        }

        // 7. Terminate controller (only if separate from component)
        if (handle->controller) {
            if (!handle->isSingleComponent) {
                handle->controller->terminate();
            }
            handle->controller = nullptr;
        }

        // 8. Terminate component
        if (handle->component) {
            handle->component->terminate();
            handle->component = nullptr;
        }

        // 9. Release the module last
        handle->module = nullptr;

        delete handle;
        fprintf(stderr, "vst3_gui_destroy: handle destroyed\n");
    } catch (const std::exception& e) {
        fprintf(stderr, "vst3_gui_destroy: C++ exception: %s\n", e.what());
        delete handle;
    } catch (...) {
        fprintf(stderr, "vst3_gui_destroy: unknown C++ exception\n");
        delete handle;
    }
}

// ---------------------------------------------------------------------------
// View size
// ---------------------------------------------------------------------------

int vst3_gui_get_size(Vst3GuiHandle* handle, float* width, float* height) {
    if (!handle || !handle->view) return -1;
    ViewRect rect = {};
    if (handle->view->getSize(&rect) != kResultOk) return -1;
    if (width) *width = static_cast<float>(rect.right - rect.left);
    if (height) *height = static_cast<float>(rect.bottom - rect.top);
    return 0;
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

int vst3_gui_get_state(Vst3GuiHandle* handle, unsigned char* data, int capacity) {
    if (!handle || !handle->component) return -1;

    try {
        MemoryStream stream;

        uint32_t size_placeholder = 0;
        stream.write(&size_placeholder, sizeof(size_placeholder), nullptr);

        int64 comp_start = 0;
        stream.tell(&comp_start);

        if (handle->component->getState(&stream) != kResultOk) {
            return -1;
        }

        int64 comp_end = 0;
        stream.tell(&comp_end);
        uint32_t comp_size = static_cast<uint32_t>(comp_end - comp_start);

        stream.seek(0, IBStream::kIBSeekSet, nullptr);
        stream.write(&comp_size, sizeof(comp_size), nullptr);
        stream.seek(comp_end, IBStream::kIBSeekSet, nullptr);

        if (handle->controller && !handle->isSingleComponent) {
            handle->controller->getState(&stream);
        }

        int64 total_size = 0;
        stream.seek(0, IBStream::kIBSeekEnd, &total_size);
        stream.seek(0, IBStream::kIBSeekSet, nullptr);

        if (!data || capacity <= 0) {
            return (int)total_size;
        }

        int bytesToCopy = (int)total_size < capacity ? (int)total_size : capacity;
        int32 bytesRead = 0;
        stream.read(data, bytesToCopy, &bytesRead);
        return bytesRead;
    } catch (...) {
        fprintf(stderr, "vst3_gui_get_state: C++ exception\n");
        return -1;
    }
}

int vst3_gui_set_state(Vst3GuiHandle* handle, const unsigned char* data, int size) {
    if (!handle || !handle->component || !data || size <= (int)sizeof(uint32_t)) return -1;

    try {
        uint32_t comp_size = 0;
        memcpy(&comp_size, data, sizeof(comp_size));

        const unsigned char* comp_data = data + sizeof(uint32_t);
        int comp_data_len = (int)comp_size;
        if ((int)sizeof(uint32_t) + comp_data_len > size) {
            comp_data_len = size - (int)sizeof(uint32_t);
        }

        MemoryStream compStream;
        compStream.write((void*)comp_data, comp_data_len, nullptr);
        compStream.seek(0, IBStream::kIBSeekSet, nullptr);
        tresult result = handle->component->setState(&compStream);
        if (result != kResultOk) {
            fprintf(stderr, "vst3_gui_set_state: component setState failed (%d)\n", result);
            return -1;
        }

        if (handle->controller) {
            compStream.seek(0, IBStream::kIBSeekSet, nullptr);
            handle->controller->setComponentState(&compStream);
        }

        if (handle->controller && !handle->isSingleComponent) {
            int ctrl_offset = (int)sizeof(uint32_t) + (int)comp_size;
            if (ctrl_offset < size) {
                const unsigned char* ctrl_data = data + ctrl_offset;
                int ctrl_len = size - ctrl_offset;
                MemoryStream ctrlStream;
                ctrlStream.write((void*)ctrl_data, ctrl_len, nullptr);
                ctrlStream.seek(0, IBStream::kIBSeekSet, nullptr);
                handle->controller->setState(&ctrlStream);
            }
        }

        fprintf(stderr, "vst3_gui_set_state: restored %d bytes (comp=%u)\n", size, comp_size);
        return 0;
    } catch (...) {
        fprintf(stderr, "vst3_gui_set_state: C++ exception\n");
        return -1;
    }
}

// ---------------------------------------------------------------------------
// Parameters
// ---------------------------------------------------------------------------

int vst3_gui_get_parameter_count(Vst3GuiHandle* handle) {
    if (!handle || !handle->controller) return -1;
    return handle->controller->getParameterCount();
}

int vst3_gui_get_parameter(Vst3GuiHandle* handle, int index, double* value) {
    if (!handle || !handle->controller || !value) return -1;
    int count = handle->controller->getParameterCount();
    if (index < 0 || index >= count) return -1;

    ParameterInfo info;
    if (handle->controller->getParameterInfo(index, info) != kResultOk) return -1;
    *value = handle->controller->getParamNormalized(info.id);
    return 0;
}

int vst3_gui_set_parameter(Vst3GuiHandle* handle, int index, double value) {
    if (!handle || !handle->controller) return -1;
    int count = handle->controller->getParameterCount();
    if (index < 0 || index >= count) return -1;

    ParameterInfo info;
    if (handle->controller->getParameterInfo(index, info) != kResultOk) return -1;
    if (handle->controller->setParamNormalized(info.id, value) != kResultOk) return -1;
    return 0;
}

// ---------------------------------------------------------------------------
// Audio processing
// ---------------------------------------------------------------------------

int vst3_gui_setup_processing(Vst3GuiHandle* handle, double sample_rate, int block_size) {
    if (!handle || !handle->component) return -1;

    try {
        IAudioProcessor* processor = nullptr;
        if (handle->component->queryInterface(IAudioProcessor::iid, (void**)&processor) != kResultOk || !processor) {
            fprintf(stderr, "vst3_gui_setup_processing: no IAudioProcessor\n");
            return -1;
        }
        handle->processor = processor;

        ProcessSetup setup = {};
        setup.processMode = kRealtime;
        setup.symbolicSampleSize = kSample32;
        setup.sampleRate = sample_rate;
        setup.maxSamplesPerBlock = block_size;

        if (processor->setupProcessing(setup) != kResultOk) {
            fprintf(stderr, "vst3_gui_setup_processing: setupProcessing failed\n");
            handle->processor = nullptr;
            processor->release();
            return -1;
        }

        SpeakerArrangement inputArr = 0;
        SpeakerArrangement outputArr = 0;

        int32 numInputBuses = handle->component->getBusCount(kAudio, kInput);
        if (numInputBuses > 0) {
            processor->getBusArrangement(kInput, 0, inputArr);
            handle->component->activateBus(kAudio, kInput, 0, true);
        }
        handle->inputChannels = SpeakerArr::getChannelCount(inputArr);

        int32 numOutputBuses = handle->component->getBusCount(kAudio, kOutput);
        if (numOutputBuses > 0) {
            processor->getBusArrangement(kOutput, 0, outputArr);
            handle->component->activateBus(kAudio, kOutput, 0, true);
        }
        handle->outputChannels = SpeakerArr::getChannelCount(outputArr);

        handle->component->setActive(true);
        processor->setProcessing(true);
        handle->processingSetUp = true;

        fprintf(stderr, "vst3_gui_setup_processing: OK (in=%d, out=%d, sr=%.0f, bs=%d)\n",
                handle->inputChannels, handle->outputChannels, sample_rate, block_size);
        return 0;
    } catch (...) {
        fprintf(stderr, "vst3_gui_setup_processing: C++ exception\n");
        return -1;
    }
}

int vst3_gui_process(Vst3GuiHandle* handle,
                     const float* const* inputs, int num_input_channels,
                     float** outputs, int num_output_channels,
                     int num_frames) {
    if (!handle || !handle->processor || !handle->processingSetUp) return -1;

    AudioBusBuffers inputBus = {};
    inputBus.numChannels = num_input_channels;
    inputBus.channelBuffers32 = const_cast<float**>(inputs);

    AudioBusBuffers outputBus = {};
    outputBus.numChannels = num_output_channels;
    outputBus.channelBuffers32 = outputs;

    EventListImpl eventList;
    {
        std::lock_guard<std::mutex> lock(handle->midiMutex);
        for (auto& ev : handle->pendingMidiEvents) {
            eventList.addEvent(ev);
        }
        handle->pendingMidiEvents.clear();
    }

    ProcessData data = {};
    data.processMode = kRealtime;
    data.symbolicSampleSize = kSample32;
    data.numSamples = num_frames;
    data.numInputs = (num_input_channels > 0) ? 1 : 0;
    data.numOutputs = (num_output_channels > 0) ? 1 : 0;
    data.inputs = (num_input_channels > 0) ? &inputBus : nullptr;
    data.outputs = (num_output_channels > 0) ? &outputBus : nullptr;
    data.inputEvents = &eventList;

    tresult result;
    try {
        result = handle->processor->process(data);
    } catch (...) {
        fprintf(stderr, "vst3_gui_process: C++ exception\n");
        return -1;
    }
    return (result == kResultOk) ? 0 : -1;
}

// ---------------------------------------------------------------------------
// MIDI
// ---------------------------------------------------------------------------

int vst3_gui_send_midi(Vst3GuiHandle* handle,
                       const unsigned char* midi_data, int num_events,
                       const int* sample_offsets) {
    if (!handle || !midi_data || num_events <= 0) return -1;

    std::lock_guard<std::mutex> lock(handle->midiMutex);

    for (int i = 0; i < num_events; i++) {
        const unsigned char* ev = midi_data + i * 3;
        unsigned char status = ev[0] & 0xF0;
        unsigned char channel = ev[0] & 0x0F;
        unsigned char data1 = ev[1];
        unsigned char data2 = ev[2];

        Event event = {};
        event.busIndex = 0;
        event.sampleOffset = sample_offsets ? sample_offsets[i] : 0;
        event.ppqPosition = 0;
        event.flags = Event::kIsLive;

        if (status == 0x90 && data2 > 0) {
            event.type = Event::kNoteOnEvent;
            event.noteOn.channel = (int16)channel;
            event.noteOn.pitch = (int16)data1;
            event.noteOn.velocity = (float)data2 / 127.0f;
            event.noteOn.length = 0;
            event.noteOn.tuning = 0.0f;
            event.noteOn.noteId = -1;
        } else if (status == 0x80 || (status == 0x90 && data2 == 0)) {
            event.type = Event::kNoteOffEvent;
            event.noteOff.channel = (int16)channel;
            event.noteOff.pitch = (int16)data1;
            event.noteOff.velocity = (float)data2 / 127.0f;
            event.noteOff.tuning = 0.0f;
            event.noteOff.noteId = -1;
        } else {
            continue;
        }

        handle->pendingMidiEvents.push_back(event);
    }

    return 0;
}

// ---------------------------------------------------------------------------
// Audio channel queries
// ---------------------------------------------------------------------------

int vst3_gui_get_audio_input_channels(Vst3GuiHandle* handle) {
    if (!handle) return 0;
    return handle->inputChannels;
}

int vst3_gui_get_audio_output_channels(Vst3GuiHandle* handle) {
    if (!handle) return 0;
    return handle->outputChannels;
}

// ---------------------------------------------------------------------------
// Plugin scanning
// ---------------------------------------------------------------------------

struct Vst3ScanEntry {
    std::string name;
    std::string vendor;
    std::string uid;
    std::string path;
    std::string subcategories;
};

struct Vst3ScanResult {
    std::vector<Vst3ScanEntry> entries;
};

static const char* safe_str(const std::vector<Vst3ScanEntry>& entries, int index,
                             const std::string Vst3ScanEntry::*field) {
    if (index < 0 || index >= (int)entries.size()) return "";
    return (entries[index].*field).c_str();
}

Vst3ScanResult* vst3_gui_scan(void) {
    try {
        auto result = new Vst3ScanResult();

        auto paths = VST3::Hosting::Module::getModulePaths();
        fprintf(stderr, "vst3_gui_scan: found %d module paths\n", (int)paths.size());

        for (auto& modulePath : paths) {
            std::string errMsg;
            auto module = VST3::Hosting::Module::create(modulePath, errMsg);
            if (!module) {
                fprintf(stderr, "vst3_gui_scan: skip '%s': %s\n", modulePath.c_str(), errMsg.c_str());
                continue;
            }

            auto factory = module->getFactory();
            if (!factory.get()) continue;

            // Get factory-level vendor as fallback
            std::string factoryVendor;
            auto factoryInfo = factory.info();
            if (!factoryInfo.vendor().empty()) {
                factoryVendor = factoryInfo.vendor();
            }

            for (auto& classInfo : factory.classInfos()) {
                if (classInfo.category() != kVstAudioEffectClass) continue;

                Vst3ScanEntry entry;
                entry.name = classInfo.name();
                entry.uid = classInfo.ID().toString();
                entry.path = modulePath;

                // Vendor: prefer class-level, fall back to factory-level
                std::string vendor = classInfo.vendor();
                entry.vendor = vendor.empty() ? factoryVendor : vendor;

                // Subcategories string (e.g. "Fx|EQ", "Instrument|Synth")
                entry.subcategories = classInfo.subCategoriesString();

                fprintf(stderr, "vst3_gui_scan:   '%s' by '%s' [%s] (%s)\n",
                        entry.name.c_str(), entry.vendor.c_str(),
                        entry.subcategories.c_str(), entry.uid.c_str());

                result->entries.push_back(std::move(entry));
            }
        }

        fprintf(stderr, "vst3_gui_scan: total %d plugins\n", (int)result->entries.size());
        return result;
    } catch (const std::exception& e) {
        fprintf(stderr, "vst3_gui_scan: exception: %s\n", e.what());
        return nullptr;
    } catch (...) {
        fprintf(stderr, "vst3_gui_scan: unknown exception\n");
        return nullptr;
    }
}

int vst3_gui_scan_count(const Vst3ScanResult* result) {
    if (!result) return 0;
    return (int)result->entries.size();
}

const char* vst3_gui_scan_get_name(const Vst3ScanResult* r, int i) {
    return safe_str(r->entries, i, &Vst3ScanEntry::name);
}

const char* vst3_gui_scan_get_vendor(const Vst3ScanResult* r, int i) {
    return safe_str(r->entries, i, &Vst3ScanEntry::vendor);
}

const char* vst3_gui_scan_get_uid(const Vst3ScanResult* r, int i) {
    return safe_str(r->entries, i, &Vst3ScanEntry::uid);
}

const char* vst3_gui_scan_get_path(const Vst3ScanResult* r, int i) {
    return safe_str(r->entries, i, &Vst3ScanEntry::path);
}

const char* vst3_gui_scan_get_subcategories(const Vst3ScanResult* r, int i) {
    return safe_str(r->entries, i, &Vst3ScanEntry::subcategories);
}

void vst3_gui_scan_free(Vst3ScanResult* result) {
    delete result;
}

} // extern "C"
