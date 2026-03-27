use crate::App;
use crate::ui::right_window::RightWindowTarget;

#[test]
fn master_default_state() {
    let app = App::new_headless();
    assert!((app.master.volume - 1.0).abs() < f32::EPSILON);
    assert!((app.master.pan - 0.5).abs() < f32::EPSILON);
    assert!(app.master.effect_chain_id.is_none());
}

#[test]
fn master_volume_pan_mutation() {
    let mut app = App::new_headless();
    app.master.volume = 0.5;
    app.master.pan = 0.3;
    assert!((app.master.volume - 0.5).abs() < f32::EPSILON);
    assert!((app.master.pan - 0.3).abs() < f32::EPSILON);
}

#[test]
fn open_right_window_for_master() {
    let mut app = App::new_headless();
    app.open_right_window_for_master();
    let rw = app.right_window.as_ref().expect("right window should be open");
    assert!(matches!(rw.target, RightWindowTarget::Master));
    assert!(rw.is_master());
    assert!(!rw.is_group());
    assert!(!rw.is_waveform());
    assert!(!rw.is_instrument());
    assert!((rw.volume - 1.0).abs() < f32::EPSILON);
    assert!((rw.pan - 0.5).abs() < f32::EPSILON);
    assert_eq!(rw.group_name, "Main");
}

#[test]
fn master_right_window_updates_volume() {
    let mut app = App::new_headless();
    app.master.volume = 0.7;
    app.open_right_window_for_master();
    let rw = app.right_window.as_ref().unwrap();
    assert!((rw.volume - 0.7).abs() < f32::EPSILON);
}
