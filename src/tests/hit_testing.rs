use std::sync::Arc;

use indexmap::IndexMap;

use crate::automation::AutomationData;
use crate::component;
use crate::entity_id::{EntityId, new_id};
use crate::ui::hit_testing::{canonical_rect, hit_test, point_in_rect, rects_overlap, targets_in_rect};
use crate::ui::waveform::{AudioData, WarpMode, WaveformPeaks, WaveformView};
use crate::{Camera, CanvasObject, HitTarget};

fn make_object(x: f32, y: f32) -> CanvasObject {
    CanvasObject {
        position: [x, y],
        size: [100.0, 60.0],
        color: [1.0, 0.0, 0.0, 1.0],
        border_radius: 0.0,
    }
}

fn make_waveform(x: f32, y: f32) -> WaveformView {
    WaveformView {
        audio: Arc::new(AudioData {
            left_samples: Arc::new(Vec::new()),
            right_samples: Arc::new(Vec::new()),
            left_peaks: Arc::new(WaveformPeaks::empty()),
            right_peaks: Arc::new(WaveformPeaks::empty()),
            sample_rate: 48000,
            filename: "test.wav".to_string(),
        }),
        filename: "test.wav".to_string(),
        position: [x, y],
        size: [200.0, 80.0],
        color: [0.0, 1.0, 0.0, 1.0],
        border_radius: 4.0,
        fade_in_px: 0.0,
        fade_out_px: 0.0,
        fade_in_curve: 0.5,
        fade_out_curve: 0.5,
        volume: 1.0,
        pan: 0.5,
        warp_mode: WarpMode::Off,
        sample_bpm: 120.0,
        pitch_semitones: 0.0,
        disabled: false,
        sample_offset_px: 0.0,
        automation: AutomationData::new(),
    }
}

// ---- point_in_rect ----

#[test]
fn test_point_in_rect_inside() {
    assert!(point_in_rect([50.0, 30.0], [0.0, 0.0], [100.0, 60.0]));
}

#[test]
fn test_point_in_rect_outside() {
    assert!(!point_in_rect([150.0, 30.0], [0.0, 0.0], [100.0, 60.0]));
}

#[test]
fn test_point_in_rect_on_boundary() {
    // Boundaries are inclusive (<=)
    assert!(point_in_rect([0.0, 0.0], [0.0, 0.0], [100.0, 60.0]));
    assert!(point_in_rect([100.0, 60.0], [0.0, 0.0], [100.0, 60.0]));
}

// ---- rects_overlap ----

#[test]
fn test_rects_overlap_overlapping() {
    assert!(rects_overlap(
        [0.0, 0.0],
        [100.0, 100.0],
        [50.0, 50.0],
        [100.0, 100.0]
    ));
}

#[test]
fn test_rects_overlap_adjacent() {
    // Touching but not overlapping -- strict < means adjacent = false
    assert!(!rects_overlap(
        [0.0, 0.0],
        [100.0, 100.0],
        [100.0, 0.0],
        [100.0, 100.0]
    ));
}

#[test]
fn test_rects_overlap_contained() {
    assert!(rects_overlap(
        [0.0, 0.0],
        [200.0, 200.0],
        [50.0, 50.0],
        [10.0, 10.0]
    ));
}

// ---- canonical_rect ----

#[test]
fn test_canonical_rect_positive_direction() {
    let (pos, size) = canonical_rect([10.0, 20.0], [110.0, 120.0]);
    assert_eq!(pos, [10.0, 20.0]);
    assert_eq!(size, [100.0, 100.0]);
}

#[test]
fn test_canonical_rect_negative_direction() {
    let (pos, size) = canonical_rect([110.0, 120.0], [10.0, 20.0]);
    assert_eq!(pos, [10.0, 20.0]);
    assert_eq!(size, [100.0, 100.0]);
}

// ---- hit_test priority ----

#[test]
fn test_hit_test_priority_order() {
    // Instance > Waveform > Object > ComponentDef
    let mut objects: IndexMap<EntityId, CanvasObject> = IndexMap::new();
    let obj_id = new_id();
    objects.insert(obj_id, make_object(0.0, 0.0));

    let mut waveforms: IndexMap<EntityId, WaveformView> = IndexMap::new();
    let wf_id = new_id();
    waveforms.insert(wf_id, make_waveform(0.0, 0.0));

    let comp_id = new_id();
    let comp = component::ComponentDef {
        id: comp_id,
        name: "C".to_string(),
        position: [0.0, 0.0],
        size: [200.0, 100.0],
        waveform_ids: vec![], // waveform NOT owned by component
    };
    let mut components: IndexMap<EntityId, component::ComponentDef> = IndexMap::new();
    components.insert(comp_id, comp.clone());

    let inst_id = new_id();
    let inst = component::ComponentInstance {
        component_id: comp_id,
        position: [0.0, 0.0],
    };
    let mut instances: IndexMap<EntityId, component::ComponentInstance> = IndexMap::new();
    instances.insert(inst_id, inst);

    let camera = Camera::new();
    let empty_er = IndexMap::new();
    let empty_pb = IndexMap::new();
    let empty_lr = IndexMap::new();
    let empty_xr = IndexMap::new();
    let empty_mc = IndexMap::new();
    let empty_ir = IndexMap::new();

    // With instance present, should hit instance first
    let result = hit_test(
        &objects,
        &waveforms,
        &empty_er,
        &empty_pb,
        &empty_lr,
        &empty_xr,
        &components,
        &instances,
        &empty_mc,
        &empty_ir,
        &IndexMap::new(),
        None,
        [50.0, 30.0],
        &camera,
    );
    assert_eq!(result, Some(HitTarget::ComponentInstance(inst_id)));

    // Without instance, waveform (not owned by component) wins over object
    let empty_inst: IndexMap<EntityId, component::ComponentInstance> = IndexMap::new();
    let result = hit_test(
        &objects,
        &waveforms,
        &empty_er,
        &empty_pb,
        &empty_lr,
        &empty_xr,
        &components,
        &empty_inst,
        &empty_mc,
        &empty_ir,
        &IndexMap::new(),
        None,
        [50.0, 30.0],
        &camera,
    );
    assert_eq!(result, Some(HitTarget::Waveform(wf_id)));

    // Without waveform, object wins over component def
    let empty_wf: IndexMap<EntityId, WaveformView> = IndexMap::new();
    let result = hit_test(
        &objects,
        &empty_wf,
        &empty_er,
        &empty_pb,
        &empty_lr,
        &empty_xr,
        &components,
        &empty_inst,
        &empty_mc,
        &empty_ir,
        &IndexMap::new(),
        None,
        [50.0, 30.0],
        &camera,
    );
    assert_eq!(result, Some(HitTarget::Object(obj_id)));
}

// ---- targets_in_rect skips component waveforms ----

#[test]
fn test_targets_in_rect_skips_component_waveforms() {
    let mut waveforms: IndexMap<EntityId, WaveformView> = IndexMap::new();
    let wf0_id = new_id();
    let wf1_id = new_id();
    waveforms.insert(wf0_id, make_waveform(0.0, 0.0));
    waveforms.insert(wf1_id, make_waveform(300.0, 0.0));

    let comp_id = new_id();
    let comp = component::ComponentDef {
        id: comp_id,
        name: "C".to_string(),
        position: [0.0, 0.0],
        size: [200.0, 80.0],
        waveform_ids: vec![wf0_id], // waveform 0 is owned
    };
    let mut components: IndexMap<EntityId, component::ComponentDef> = IndexMap::new();
    components.insert(comp_id, comp);

    let empty_obj = IndexMap::new();
    let empty_er = IndexMap::new();
    let empty_pb = IndexMap::new();
    let empty_lr = IndexMap::new();
    let empty_xr = IndexMap::new();
    let empty_inst = IndexMap::new();
    let empty_mc = IndexMap::new();
    let empty_ir = IndexMap::new();

    let targets = targets_in_rect(
        &empty_obj,
        &waveforms,
        &empty_er,
        &empty_pb,
        &empty_lr,
        &empty_xr,
        &components,
        &empty_inst,
        &empty_mc,
        &empty_ir,
        &IndexMap::new(),
        None,
        [0.0, 0.0],
        [600.0, 200.0],
    );

    // Waveform 0 should be skipped (owned by component), waveform 1 should be included
    let has_wf0 = targets.contains(&HitTarget::Waveform(wf0_id));
    let has_wf1 = targets.contains(&HitTarget::Waveform(wf1_id));
    assert!(!has_wf0, "waveform owned by component should be skipped");
    assert!(has_wf1, "free waveform should be selected");
}
