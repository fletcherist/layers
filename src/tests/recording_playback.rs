use crate::App;

#[test]
fn toggle_recording_headless_no_panic() {
    let mut app = App::new_headless();
    assert!(!app.is_recording());
    app.toggle_recording();
    assert!(!app.is_recording());
}

#[test]
fn toggle_recording_twice_headless_no_panic() {
    let mut app = App::new_headless();
    app.toggle_recording();
    app.toggle_recording();
    assert!(!app.is_recording());
}
