use crate::App;

#[test]
fn toggle_metronome() {
    let mut app = App::new_headless();
    assert!(!app.settings.metronome_enabled);
    app.settings.metronome_enabled = true;
    assert!(app.settings.metronome_enabled);
}
