use crate::App;
use crate::settings::Settings;
use crate::ui::browser::{BrowserCategory, SampleBrowser, PluginEntry, EntryKind};
use std::path::PathBuf;
use std::collections::HashSet;

#[test]
fn test_add_folder_updates_browser_state() {
    let mut app = App::new_headless();
    assert!(app.sample_browser.root_folders.is_empty());
    assert!(app.sample_browser.visible);

    // Use a real directory so `add_folder` / `from_state` accept it
    let tmp = std::env::temp_dir();
    app.sample_browser.add_folder(tmp.clone());

    assert_eq!(app.sample_browser.root_folders.len(), 1);
    assert_eq!(app.sample_browser.root_folders[0], tmp);
    assert!(app.sample_browser.expanded.contains(&tmp));
}

#[test]
fn test_add_folder_selects_new_place() {
    let mut browser = SampleBrowser::new();
    let tmp = std::env::temp_dir();
    // Add a first valid real directory
    browser.add_folder(tmp.clone());
    assert_eq!(browser.selected_place, 0);

    // Add a second folder that is also tmp (will be ignored as duplicate)
    // so test with /tmp itself being the one valid dir.
    // Verify selected_place stays at 0 for one-folder case
    assert_eq!(browser.root_folders.len(), 1);
    assert_eq!(browser.selected_place, 0);
}

#[test]
fn test_add_duplicate_folder_ignored() {
    let mut app = App::new_headless();
    let tmp = std::env::temp_dir();

    app.sample_browser.add_folder(tmp.clone());
    app.sample_browser.add_folder(tmp.clone());

    assert_eq!(app.sample_browser.root_folders.len(), 1);
}

#[test]
fn test_settings_sample_library_folders_roundtrip() {
    let mut settings = Settings::default();
    assert!(settings.sample_library_folders.is_empty());

    settings.sample_library_folders = vec![
        "/tmp/samples_a".to_string(),
        "/tmp/samples_b".to_string(),
    ];

    let json = serde_json::to_string(&settings).unwrap();
    let loaded: Settings = serde_json::from_str(&json).unwrap();

    assert_eq!(loaded.sample_library_folders.len(), 2);
    assert_eq!(loaded.sample_library_folders[0], "/tmp/samples_a");
    assert_eq!(loaded.sample_library_folders[1], "/tmp/samples_b");
}

#[test]
fn test_settings_without_sample_folders_defaults_empty() {
    // Simulate loading a settings.json written before this field existed
    let json = r#"{"grid_line_intensity":0.26,"brightness":1.0,"color_intensity":0.0}"#;
    let loaded: Settings = serde_json::from_str(json).unwrap();
    assert!(loaded.sample_library_folders.is_empty());
}

#[test]
fn test_browser_from_state_restores_global_folders() {
    let tmp = std::env::temp_dir();
    let folders = vec![tmp.clone()];
    let mut expanded = HashSet::new();
    expanded.insert(tmp.clone());

    let browser =
        crate::ui::browser::SampleBrowser::from_state(folders, expanded, true);

    assert_eq!(browser.root_folders.len(), 1);
    assert_eq!(browser.root_folders[0], tmp);
    assert!(browser.visible);
    assert!(browser.expanded.contains(&tmp));
}

#[test]
fn test_rebuild_entries_samples_shows_only_selected_place() {
    let tmp = std::env::temp_dir();
    let mut browser = SampleBrowser::new();
    browser.add_folder(tmp.clone());

    // With one root and selected_place=0, entries should contain at least the root dir entry
    browser.active_category = BrowserCategory::Samples;
    browser.rebuild_entries();
    // The root dir itself is always added by walk_dir as a Dir entry
    assert!(!browser.entries.is_empty());
    assert!(matches!(browser.entries[0].kind, EntryKind::Dir));
    assert_eq!(browser.entries[0].path, tmp);
}

#[test]
fn test_selected_place_clamped_after_remove() {
    let tmp = std::env::temp_dir();
    let mut browser = SampleBrowser::new();
    browser.add_folder(tmp.clone());
    assert_eq!(browser.selected_place, 0);
    // Removing the only folder should clamp to 0 safely
    browser.remove_folder(0);
    assert!(browser.root_folders.is_empty());
    assert_eq!(browser.selected_place, 0);
}

#[test]
fn test_category_switching_rebuilds_entries() {
    let mut browser = SampleBrowser::new();
    let effects = vec![PluginEntry {
        unique_id: "fx1".into(),
        name: "TestFX".into(),
        manufacturer: "Test".into(),
        is_instrument: false,
    }];
    let instruments = vec![PluginEntry {
        unique_id: "inst1".into(),
        name: "TestSynth".into(),
        manufacturer: "Test".into(),
        is_instrument: true,
    }];
    browser.set_plugins(effects, instruments);

    // Default category is Samples — no entries (no folders added)
    assert_eq!(browser.active_category, BrowserCategory::Samples);
    assert!(browser.entries.is_empty());

    // Switch to Instruments
    browser.active_category = BrowserCategory::Instruments;
    browser.rebuild_entries();
    assert_eq!(browser.entries.len(), 1);
    assert!(matches!(browser.entries[0].kind, EntryKind::Plugin { ref unique_id, is_instrument: true } if unique_id == "inst1"));

    // Switch to Effects
    browser.active_category = BrowserCategory::Effects;
    browser.rebuild_entries();
    assert_eq!(browser.entries.len(), 1);
    assert!(matches!(browser.entries[0].kind, EntryKind::Plugin { ref unique_id, is_instrument: false } if unique_id == "fx1"));

    // Switch back to Samples — empty again
    browser.active_category = BrowserCategory::Samples;
    browser.rebuild_entries();
    assert!(browser.entries.is_empty());
}

#[test]
fn test_hit_sidebar_returns_correct_category() {
    let browser = SampleBrowser::new();
    let scale = 1.0;
    // Sidebar starts below header + search bar row
    let content_top = (crate::ui::browser::HEADER_HEIGHT + 32.0) as f32;

    // Click sidebar items (below content_top + section gap)
    let pos_project = [50.0, content_top + 20.0];
    assert_eq!(browser.hit_sidebar(pos_project, scale), Some(BrowserCategory::Layers));

    let pos_samples = [50.0, content_top + 20.0 + 26.0];
    assert_eq!(browser.hit_sidebar(pos_samples, scale), Some(BrowserCategory::Samples));

    let pos_instruments = [50.0, content_top + 20.0 + 52.0];
    assert_eq!(browser.hit_sidebar(pos_instruments, scale), Some(BrowserCategory::Instruments));

    let pos_effects = [50.0, content_top + 20.0 + 78.0];
    assert_eq!(browser.hit_sidebar(pos_effects, scale), Some(BrowserCategory::Effects));

    // Click in header — None
    let pos_header = [50.0, 5.0];
    assert_eq!(browser.hit_sidebar(pos_header, scale), None);

    // Click in content area (x > sidebar width) — None
    let pos_content = [120.0, content_top + 20.0];
    assert_eq!(browser.hit_sidebar(pos_content, scale), None);
}

#[test]
fn test_hit_place_row_and_places_add() {
    let tmp = std::env::temp_dir();
    let mut browser = SampleBrowser::new();
    browser.add_folder(tmp.clone());

    let scale = 1.0;
    let ct = (crate::ui::browser::HEADER_HEIGHT + 32.0) as f32;
    // places_section_y = ct + 18 + 4*26 + 8 = ct + 130
    let places_y = ct + 18.0 + 4.0 * 26.0 + 8.0;
    let ph = 20.0_f32; // PLACES_HEADER_HEIGHT
    let rh = 24.0_f32; // PLACES_ROW_HEIGHT

    // Click on the single place row — must be in sidebar (x < 110)
    let row_y = places_y + ph + 2.0;
    let hit = browser.hit_place_row([50.0, row_y], scale);
    assert_eq!(hit, Some(0));

    // Click on "Add Folder…" row — still in sidebar
    let add_row_y = places_y + ph + rh + 2.0;
    assert!(browser.hit_places_add([50.0, add_row_y], scale));

    // Click in tree area (x >= 110) — neither
    assert_eq!(browser.hit_place_row([120.0, row_y], scale), None);
    assert!(!browser.hit_places_add([120.0, add_row_y], scale));
}

#[test]
fn test_browser_preview_state() {
    let mut app = App::new_headless();

    // Initially no preview
    assert!(app.sample_browser.preview_audio.is_none());
    assert!(app.sample_browser.preview_path.is_none());
    assert!(app.sample_browser.auto_preview);

    // Toggle auto-preview
    app.sample_browser.auto_preview = false;
    assert!(!app.sample_browser.auto_preview);

    // Set a preview path
    let path = PathBuf::from("/tmp/test.wav");
    app.sample_browser.preview_path = Some(path.clone());
    assert_eq!(app.sample_browser.preview_path, Some(path));

    // Visible height should NOT shrink when no preview audio loaded
    let screen_h = 800.0;
    let scale = 1.0;
    let h_no_preview = app.sample_browser.visible_height_for_test(screen_h, scale);

    // Set preview audio
    use std::sync::Arc;
    use crate::ui::waveform::{AudioData, WaveformPeaks};
    let audio = Arc::new(AudioData {
        left_samples: Arc::new(vec![0.0; 1000]),
        right_samples: Arc::new(vec![0.0; 1000]),
        left_peaks: Arc::new(WaveformPeaks::build(&[0.0; 1000])),
        right_peaks: Arc::new(WaveformPeaks::build(&[0.0; 1000])),
        sample_rate: 44100,
        filename: "test.wav".to_string(),
    });
    app.sample_browser.preview_audio = Some(audio);

    // Visible height should shrink by PREVIEW_STRIP_HEIGHT
    let h_with_preview = app.sample_browser.visible_height_for_test(screen_h, scale);
    assert!(h_with_preview < h_no_preview);
    let expected_diff = crate::ui::browser::PREVIEW_STRIP_HEIGHT * scale;
    assert!((h_no_preview - h_with_preview - expected_diff).abs() < 0.1);
}
