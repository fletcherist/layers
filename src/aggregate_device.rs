//! CoreAudio Aggregate Device creation for macOS.
//!
//! When input and output audio devices use different hardware clocks (e.g. USB
//! mic + built-in speakers), CoreAudio can unify them into a single aggregate
//! device with OS-level drift compensation.  This eliminates clock-drift
//! artifacts (clicks, ring-buffer under/overruns) that software resampling
//! cannot fully avoid.
//!
//! Approach matches Ardour and other professional macOS DAWs.

#![allow(non_upper_case_globals)]

use std::ffi::c_void;
use std::ptr;

// ── CoreAudio / CoreFoundation FFI ──────────────────────────────────────────

type OSStatus = i32;
type AudioObjectID = u32;
type AudioObjectPropertySelector = u32;
type AudioObjectPropertyScope = u32;
type AudioObjectPropertyElement = u32;

type CFStringRef = *const c_void;
type CFMutableStringRef = *mut c_void;
type CFDictionaryRef = *const c_void;
type CFMutableDictionaryRef = *mut c_void;
type CFMutableArrayRef = *mut c_void;
type CFAllocatorRef = *const c_void;
type CFNumberRef = *const c_void;
type CFTypeRef = *const c_void;
type CFIndex = isize;
type CFStringEncoding = u32;
type CFNumberType = u32;

const kCFStringEncodingUTF8: CFStringEncoding = 0x08000100;
const kCFNumberSInt32Type: CFNumberType = 3;

const kAudioObjectSystemObject: AudioObjectID = 1;
const kAudioObjectPropertyScopeGlobal: AudioObjectPropertyScope = 0x676C6F62; // 'glob'
const kAudioObjectPropertyElementMain: AudioObjectPropertyElement = 0;

const kAudioDevicePropertyDeviceUID: AudioObjectPropertySelector = 0x75696420; // 'uid '
const kAudioDevicePropertyClockDomain: AudioObjectPropertySelector = 0x636C6B64; // 'clkd'
const kAudioHardwarePropertyDevices: AudioObjectPropertySelector = 0x64657623; // 'dev#'
const kAudioObjectPropertyName: AudioObjectPropertySelector = 0x6C6E616D; // 'lnam'
const kAudioAggregateDevicePropertyFullSubDeviceList: AudioObjectPropertySelector = 0x67727570; // 'grup'
const kAudioAggregateDevicePropertyMainSubDevice: AudioObjectPropertySelector = 0x616D7374; // 'amst'
const kAudioObjectPropertyOwnedObjects: AudioObjectPropertySelector = 0x6F776E64; // 'ownd'
const kAudioSubDevicePropertyDriftCompensation: AudioObjectPropertySelector = 0x64726674; // 'drft'
const kAudioSubDeviceClassID: u32 = 0x61737562; // 'asub'
const kAudioDevicePropertyBufferFrameSize: AudioObjectPropertySelector = 0x6673697A; // 'fsiz'

#[repr(C)]
struct AudioObjectPropertyAddress {
    selector: AudioObjectPropertySelector,
    scope: AudioObjectPropertyScope,
    element: AudioObjectPropertyElement,
}

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectGetPropertyDataSize(
        id: AudioObjectID,
        addr: *const AudioObjectPropertyAddress,
        qualifier_data_size: u32,
        qualifier_data: *const c_void,
        out_size: *mut u32,
    ) -> OSStatus;

    fn AudioObjectGetPropertyData(
        id: AudioObjectID,
        addr: *const AudioObjectPropertyAddress,
        qualifier_data_size: u32,
        qualifier_data: *const c_void,
        data_size: *mut u32,
        data: *mut c_void,
    ) -> OSStatus;

    fn AudioObjectSetPropertyData(
        id: AudioObjectID,
        addr: *const AudioObjectPropertyAddress,
        qualifier_data_size: u32,
        qualifier_data: *const c_void,
        data_size: u32,
        data: *const c_void,
    ) -> OSStatus;

    fn AudioHardwareCreateAggregateDevice(
        desc: CFDictionaryRef,
        out_device_id: *mut AudioObjectID,
    ) -> OSStatus;

    fn AudioHardwareDestroyAggregateDevice(device_id: AudioObjectID) -> OSStatus;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFAllocatorDefault: CFAllocatorRef;
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
    static kCFTypeArrayCallBacks: c_void;

    fn CFDictionaryCreateMutable(
        alloc: CFAllocatorRef,
        capacity: CFIndex,
        key_cb: *const c_void,
        val_cb: *const c_void,
    ) -> CFMutableDictionaryRef;
    fn CFDictionarySetValue(dict: CFMutableDictionaryRef, key: *const c_void, val: *const c_void);
    fn CFArrayCreateMutable(
        alloc: CFAllocatorRef,
        capacity: CFIndex,
        cb: *const c_void,
    ) -> CFMutableArrayRef;
    fn CFArrayAppendValue(arr: CFMutableArrayRef, val: *const c_void);
    fn CFStringCreateWithCString(
        alloc: CFAllocatorRef,
        cstr: *const u8,
        encoding: CFStringEncoding,
    ) -> CFStringRef;
    fn CFNumberCreate(
        alloc: CFAllocatorRef,
        the_type: CFNumberType,
        value_ptr: *const c_void,
    ) -> CFNumberRef;
    fn CFRelease(cf: CFTypeRef);
    fn CFStringGetLength(s: CFStringRef) -> CFIndex;
    fn CFStringGetCString(
        s: CFStringRef,
        buffer: *mut u8,
        buffer_size: CFIndex,
        encoding: CFStringEncoding,
    ) -> bool;
    fn CFRunLoopRunInMode(mode: CFStringRef, seconds: f64, return_after_source_handled: bool) -> i32;
    fn CFStringCreateWithBytes(
        alloc: CFAllocatorRef,
        bytes: *const u8,
        len: CFIndex,
        encoding: CFStringEncoding,
        is_external: bool,
    ) -> CFStringRef;
}

// Commonly used CFRunLoop mode
extern "C" {
    static kCFRunLoopDefaultMode: CFStringRef;
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn cfstr(s: &str) -> CFStringRef {
    unsafe {
        CFStringCreateWithBytes(
            kCFAllocatorDefault,
            s.as_ptr(),
            s.len() as CFIndex,
            kCFStringEncodingUTF8,
            false,
        )
    }
}

fn cfnum_i32(v: i32) -> CFNumberRef {
    unsafe {
        CFNumberCreate(
            kCFAllocatorDefault,
            kCFNumberSInt32Type,
            &v as *const i32 as *const c_void,
        )
    }
}

fn cfstring_to_string(cfstr: CFStringRef) -> Option<String> {
    if cfstr.is_null() {
        return None;
    }
    unsafe {
        let len = CFStringGetLength(cfstr);
        let buf_size = (len * 4 + 1) as usize; // UTF-8 worst case
        let mut buf = vec![0u8; buf_size];
        if CFStringGetCString(cfstr, buf.as_mut_ptr(), buf_size as CFIndex, kCFStringEncodingUTF8) {
            let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
            Some(String::from_utf8_lossy(&buf[..end]).into_owned())
        } else {
            None
        }
    }
}

fn global_addr(selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        selector,
        scope: kAudioObjectPropertyScopeGlobal,
        element: kAudioObjectPropertyElementMain,
    }
}

// ── Device enumeration ──────────────────────────────────────────────────────

/// List all audio device IDs on the system.
fn all_device_ids() -> Vec<AudioObjectID> {
    unsafe {
        let addr = global_addr(kAudioHardwarePropertyDevices);
        let mut size: u32 = 0;
        if AudioObjectGetPropertyDataSize(kAudioObjectSystemObject, &addr, 0, ptr::null(), &mut size) != 0 {
            return Vec::new();
        }
        let count = size as usize / std::mem::size_of::<AudioObjectID>();
        let mut ids = vec![0u32; count];
        if AudioObjectGetPropertyData(
            kAudioObjectSystemObject,
            &addr,
            0,
            ptr::null(),
            &mut size,
            ids.as_mut_ptr() as *mut c_void,
        ) != 0
        {
            return Vec::new();
        }
        ids
    }
}

/// Get the UID string of an audio device.
fn device_uid(device_id: AudioObjectID) -> Option<String> {
    unsafe {
        let addr = global_addr(kAudioDevicePropertyDeviceUID);
        let mut uid_ref: CFStringRef = ptr::null();
        let mut size = std::mem::size_of::<CFStringRef>() as u32;
        if AudioObjectGetPropertyData(
            device_id,
            &addr,
            0,
            ptr::null(),
            &mut size,
            &mut uid_ref as *mut _ as *mut c_void,
        ) != 0
        {
            return None;
        }
        let result = cfstring_to_string(uid_ref);
        if !uid_ref.is_null() {
            CFRelease(uid_ref);
        }
        result
    }
}

/// Get the name of an audio device.
fn device_name(device_id: AudioObjectID) -> Option<String> {
    unsafe {
        let addr = global_addr(kAudioObjectPropertyName);
        let mut name_ref: CFStringRef = ptr::null();
        let mut size = std::mem::size_of::<CFStringRef>() as u32;
        if AudioObjectGetPropertyData(
            device_id,
            &addr,
            0,
            ptr::null(),
            &mut size,
            &mut name_ref as *mut _ as *mut c_void,
        ) != 0
        {
            return None;
        }
        let result = cfstring_to_string(name_ref);
        if !name_ref.is_null() {
            CFRelease(name_ref);
        }
        result
    }
}

/// Get the clock domain of an audio device.
fn clock_domain(device_id: AudioObjectID) -> Option<u32> {
    unsafe {
        let addr = global_addr(kAudioDevicePropertyClockDomain);
        let mut domain: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        if AudioObjectGetPropertyData(
            device_id,
            &addr,
            0,
            ptr::null(),
            &mut size,
            &mut domain as *mut _ as *mut c_void,
        ) != 0
        {
            return None;
        }
        Some(domain)
    }
}

/// Find the AudioObjectID of a device by its name (as reported by cpal).
fn find_device_id_by_name(name: &str) -> Option<AudioObjectID> {
    for id in all_device_ids() {
        if device_name(id).as_deref() == Some(name) {
            return Some(id);
        }
    }
    None
}

// ── Aggregate Device ────────────────────────────────────────────────────────

/// A CoreAudio aggregate device that unifies input and output under one clock.
/// Destroyed automatically on drop.
pub struct AggregateDevice {
    device_id: AudioObjectID,
    pub name: String,
}

impl AggregateDevice {
    /// Create an aggregate device combining `input_name` and `output_name`.
    ///
    /// Returns `None` if:
    /// - either device is not found
    /// - both devices share the same clock domain (no aggregate needed)
    /// - CoreAudio refuses to create the aggregate
    pub fn new(input_name: &str, output_name: &str, buffer_frames: u32) -> Option<Self> {
        let input_id = find_device_id_by_name(input_name)?;
        let output_id = find_device_id_by_name(output_name)?;

        // Same physical device → same clock, no aggregate needed
        if input_id == output_id {
            println!("  Aggregate device: input==output, not needed");
            return None;
        }

        let input_uid = device_uid(input_id)?;
        let output_uid = device_uid(output_id)?;

        // Check clock domains — if same, no drift compensation needed
        let in_clock = clock_domain(input_id);
        let out_clock = clock_domain(output_id);
        let same_clock = match (in_clock, out_clock) {
            (Some(a), Some(b)) => a == b,
            _ => false, // unknown → assume different
        };
        if same_clock {
            println!("  Aggregate device: same clock domain, not needed");
            return None;
        }

        println!(
            "  Creating aggregate device: input='{}' (uid={}) + output='{}' (uid={})",
            input_name, input_uid, output_name, output_uid
        );

        unsafe {
            // Build aggregate description dictionary
            let agg_uid = cfstr("com.layers.aggregate");
            let agg_name_str = "Layers Duplex";
            let agg_name_cf = cfstr(agg_name_str);
            let private_val = cfnum_i32(1);
            let stacked_val = cfnum_i32(0);

            let desc = CFDictionaryCreateMutable(
                kCFAllocatorDefault,
                0,
                &kCFTypeDictionaryKeyCallBacks,
                &kCFTypeDictionaryValueCallBacks,
            );
            CFDictionarySetValue(desc, cfstr("uid") as *const c_void, agg_uid as *const c_void);
            CFDictionarySetValue(desc, cfstr("name") as *const c_void, agg_name_cf as *const c_void);
            CFDictionarySetValue(desc, cfstr("private") as *const c_void, private_val as *const c_void);
            CFDictionarySetValue(desc, cfstr("stacked") as *const c_void, stacked_val as *const c_void);

            // Create blank aggregate
            let mut agg_id: AudioObjectID = 0;
            let status = AudioHardwareCreateAggregateDevice(desc as CFDictionaryRef, &mut agg_id);
            CFRelease(desc as CFTypeRef);

            if status != 0 {
                eprintln!("  AudioHardwareCreateAggregateDevice failed: {}", status);
                return None;
            }
            println!("  Aggregate device created: id={}", agg_id);

            // Wait for device to register
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, false);

            // Set sub-device list (input first, then output)
            let sub_list = CFArrayCreateMutable(kCFAllocatorDefault, 0, &kCFTypeArrayCallBacks);
            let in_uid_cf = cfstr(&input_uid);
            let out_uid_cf = cfstr(&output_uid);
            CFArrayAppendValue(sub_list, in_uid_cf as *const c_void);
            CFArrayAppendValue(sub_list, out_uid_cf as *const c_void);

            let addr = global_addr(kAudioAggregateDevicePropertyFullSubDeviceList);
            let status = AudioObjectSetPropertyData(
                agg_id,
                &addr,
                0,
                ptr::null(),
                std::mem::size_of::<CFMutableArrayRef>() as u32,
                &sub_list as *const _ as *const c_void,
            );
            CFRelease(sub_list as CFTypeRef);
            if status != 0 {
                eprintln!("  Failed to set sub-device list: {}", status);
                AudioHardwareDestroyAggregateDevice(agg_id);
                return None;
            }

            // Wait for sub-devices to register
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, false);

            // Set input device as master clock
            let addr = global_addr(kAudioAggregateDevicePropertyMainSubDevice);
            let status = AudioObjectSetPropertyData(
                agg_id,
                &addr,
                0,
                ptr::null(),
                std::mem::size_of::<CFStringRef>() as u32,
                &in_uid_cf as *const _ as *const c_void,
            );
            if status != 0 {
                eprintln!("  Failed to set master sub-device: {}", status);
                // Non-fatal — continue
            }

            // Enable drift compensation on non-master sub-devices
            let addr = global_addr(kAudioObjectPropertyOwnedObjects);
            let qualifier = kAudioSubDeviceClassID;
            let mut size: u32 = 0;
            let status = AudioObjectGetPropertyDataSize(
                agg_id,
                &addr,
                std::mem::size_of::<u32>() as u32,
                &qualifier as *const _ as *const c_void,
                &mut size,
            );
            if status == 0 && size > 0 {
                let count = size as usize / std::mem::size_of::<AudioObjectID>();
                let mut sub_ids = vec![0u32; count];
                let status = AudioObjectGetPropertyData(
                    agg_id,
                    &addr,
                    std::mem::size_of::<u32>() as u32,
                    &qualifier as *const _ as *const c_void,
                    &mut size,
                    sub_ids.as_mut_ptr() as *mut c_void,
                );
                if status == 0 {
                    // Skip first sub-device (the master clock source);
                    // enable drift compensation on all others.
                    for &sub_id in sub_ids.iter().skip(1) {
                        let drift_addr = global_addr(kAudioSubDevicePropertyDriftCompensation);
                        let one: u32 = 1;
                        let _ = AudioObjectSetPropertyData(
                            sub_id,
                            &drift_addr,
                            0,
                            ptr::null(),
                            std::mem::size_of::<u32>() as u32,
                            &one as *const _ as *const c_void,
                        );
                    }
                    println!("  Drift compensation enabled on {} sub-device(s)", sub_ids.len().saturating_sub(1));
                }
            }

            // Set buffer frame size on the aggregate device — ensures
            // hardware-level latency matches our desired buffer size.
            // CPAL's BufferSize::Fixed only sets the AUHAL side; the
            // aggregate device itself may default to 512-1024 frames.
            {
                let addr = global_addr(kAudioDevicePropertyBufferFrameSize);
                let frames = buffer_frames;
                let status = AudioObjectSetPropertyData(
                    agg_id,
                    &addr,
                    0,
                    ptr::null(),
                    std::mem::size_of::<u32>() as u32,
                    &frames as *const _ as *const c_void,
                );
                if status == 0 {
                    println!("  Aggregate device buffer size: {} frames", frames);
                } else {
                    eprintln!("  Failed to set aggregate buffer size to {}: {}", frames, status);
                }
            }

            // Final wait for everything to settle
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, false);

            println!("  Aggregate device ready: '{}'", agg_name_str);
            Some(AggregateDevice {
                device_id: agg_id,
                name: agg_name_str.to_string(),
            })
        }
    }
}

impl Drop for AggregateDevice {
    fn drop(&mut self) {
        println!("  Destroying aggregate device: id={}", self.device_id);
        unsafe {
            AudioHardwareDestroyAggregateDevice(self.device_id);
        }
    }
}
