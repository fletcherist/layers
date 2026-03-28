use crate::ui::palette::{CommandPalette, PaletteRow, PluginPickerEntry};

fn make_palette_with_plugins() -> CommandPalette {
    let mut p = CommandPalette::new(false);
    p.plugin_entries = vec![
        PluginPickerEntry {
            name: "Serum".to_string(),
            manufacturer: "Xfer Records".to_string(),
            unique_id: "com.xfer.serum".to_string(),
            is_instrument: true,
        },
        PluginPickerEntry {
            name: "Serum FX".to_string(),
            manufacturer: "Xfer Records".to_string(),
            unique_id: "com.xfer.serumfx".to_string(),
            is_instrument: false,
        },
        PluginPickerEntry {
            name: "Pro-Q 3".to_string(),
            manufacturer: "FabFilter".to_string(),
            unique_id: "com.fabfilter.proq3".to_string(),
            is_instrument: false,
        },
    ];
    p
}

#[test]
fn test_palette_plugin_search_shows_matching_plugins() {
    let mut p = make_palette_with_plugins();
    p.search_input.text ="ser".to_string();
    p.update_filter(false);

    // Should have command matches + a "Plugins" section + 2 Serum entries
    let plugin_rows: Vec<_> = p.rows.iter().filter(|r| matches!(r, PaletteRow::Plugin(_))).collect();
    assert_eq!(plugin_rows.len(), 2, "Should find both Serum and Serum FX");
}

#[test]
fn test_palette_plugin_search_by_manufacturer() {
    let mut p = make_palette_with_plugins();
    p.search_input.text ="fabfilter".to_string();
    p.update_filter(false);

    let plugin_rows: Vec<_> = p.rows.iter().filter(|r| matches!(r, PaletteRow::Plugin(_))).collect();
    assert_eq!(plugin_rows.len(), 1, "Should find Pro-Q 3 by manufacturer");
}

#[test]
fn test_palette_search_by_label_instrument() {
    let mut p = make_palette_with_plugins();
    p.search_input.text ="instrument".to_string();
    p.update_filter(false);

    let plugin_rows: Vec<_> = p.rows.iter().filter(|r| matches!(r, PaletteRow::Plugin(_))).collect();
    assert_eq!(plugin_rows.len(), 1, "Should find only instruments when searching 'instrument'");
}

#[test]
fn test_palette_search_by_label_effect() {
    let mut p = make_palette_with_plugins();
    p.search_input.text ="effect".to_string();
    p.update_filter(false);

    let plugin_rows: Vec<_> = p.rows.iter().filter(|r| matches!(r, PaletteRow::Plugin(_))).collect();
    assert_eq!(plugin_rows.len(), 2, "Should find both effects when searching 'effect'");
}

#[test]
fn test_palette_empty_search_shows_all_plugins() {
    let mut p = make_palette_with_plugins();
    p.search_input.clear();
    p.update_filter(false);

    let plugin_rows: Vec<_> = p.rows.iter().filter(|r| matches!(r, PaletteRow::Plugin(_))).collect();
    assert_eq!(plugin_rows.len(), 3, "All plugins should show when search is empty");
}

#[test]
fn test_palette_selected_inline_plugin() {
    let mut p = make_palette_with_plugins();
    p.search_input.text ="serum".to_string();
    p.update_filter(false);

    // Find the index of the first Plugin row
    let first_plugin_index = {
        let mut cmd_i = 0;
        let mut found = None;
        for row in &p.rows {
            match row {
                PaletteRow::Command(_) | PaletteRow::Plugin(_) => {
                    if matches!(row, PaletteRow::Plugin(_)) && found.is_none() {
                        found = Some(cmd_i);
                    }
                    cmd_i += 1;
                }
                _ => {}
            }
        }
        found.unwrap()
    };

    p.selected_index = first_plugin_index;
    let entry = p.selected_inline_plugin();
    assert!(entry.is_some(), "Should return plugin entry");
    let entry = entry.unwrap();
    assert_eq!(entry.name, "Serum");
    assert!(entry.is_instrument);
}

#[test]
fn test_palette_selected_action_none_for_plugin_row() {
    let mut p = make_palette_with_plugins();
    p.search_input.text ="serum".to_string();
    p.update_filter(false);

    // Navigate to a Plugin row
    let first_plugin_index = {
        let mut cmd_i = 0;
        let mut found = None;
        for row in &p.rows {
            match row {
                PaletteRow::Command(_) | PaletteRow::Plugin(_) => {
                    if matches!(row, PaletteRow::Plugin(_)) && found.is_none() {
                        found = Some(cmd_i);
                    }
                    cmd_i += 1;
                }
                _ => {}
            }
        }
        found.unwrap()
    };

    p.selected_index = first_plugin_index;
    assert!(p.selected_action().is_none(), "Plugin rows should not return a CommandAction");
}
