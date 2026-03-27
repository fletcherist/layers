use crate::App;

#[test]
fn reset_theme_to_defaults_restores_factory_values() {
    let mut app = App::new_headless();

    // Modify theme settings away from defaults
    app.settings.primary_hue = 90.0;
    app.settings.brightness = 1.5;
    app.settings.color_intensity = 0.5;
    app.settings.grid_line_intensity = 0.8;
    app.settings.theme_preset = "Light".to_string();
    app.settings.theme = crate::theme::RuntimeTheme::from_preset_light(app.settings.primary_hue);

    // Reset
    app.settings.reset_theme_to_defaults();

    assert_eq!(app.settings.theme_preset, "Dark");
    assert_eq!(app.settings.primary_hue, 216.0);
    assert_eq!(app.settings.brightness, 1.0);
    assert_eq!(app.settings.color_intensity, 1.0);
    assert_eq!(app.settings.grid_line_intensity, 0.26);

    // Theme should match the default
    let default_theme = crate::theme::RuntimeTheme::from_hue(216.0);
    assert_eq!(app.settings.theme.bg_base, default_theme.bg_base);
    assert_eq!(app.settings.theme.accent, default_theme.accent);
}
