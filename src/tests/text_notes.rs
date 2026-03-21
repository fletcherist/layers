use crate::entity_id::new_id;
use crate::operations::Operation;
use crate::text_note::TextNote;
use crate::{App, HitTarget};

#[test]
fn create_text_note() {
    let mut app = App::new_headless();
    assert!(app.text_notes.is_empty());
    app.add_text_note();
    assert_eq!(app.text_notes.len(), 1);
    assert_eq!(app.selected.len(), 1);
    matches!(app.selected[0], HitTarget::TextNote(_));
}

#[test]
fn delete_text_note() {
    let mut app = App::new_headless();
    app.add_text_note();
    assert_eq!(app.text_notes.len(), 1);
    // selected[0] should be the new note
    app.delete_selected();
    assert!(app.text_notes.is_empty());
}

#[test]
fn undo_create_text_note() {
    let mut app = App::new_headless();
    app.add_text_note();
    assert_eq!(app.text_notes.len(), 1);
    app.undo_op();
    assert!(app.text_notes.is_empty());
    app.redo_op();
    assert_eq!(app.text_notes.len(), 1);
}

#[test]
fn update_text_note_via_edit() {
    let mut app = App::new_headless();
    app.add_text_note();
    let id = match app.selected[0] {
        HitTarget::TextNote(id) => id,
        _ => panic!("Expected TextNote"),
    };

    // Enter edit mode
    app.enter_text_note_edit(id);
    assert!(app.editing_text_note.is_some());

    // Modify the text directly (simulating keyboard input)
    if let Some(ref mut edit) = app.editing_text_note {
        edit.text = "Hello world".to_string();
        edit.cursor = 11;
    }
    app.text_notes.get_mut(&id).unwrap().text = "Hello world".to_string();

    // Commit edit
    app.commit_text_note_edit();
    assert!(app.editing_text_note.is_none());
    assert_eq!(app.text_notes[&id].text, "Hello world");

    // Undo should restore empty text
    app.undo_op();  // undo update
    assert_eq!(app.text_notes[&id].text, "");
}

#[test]
fn move_text_note() {
    let mut app = App::new_headless();
    app.add_text_note();
    let id = match app.selected[0] {
        HitTarget::TextNote(id) => id,
        _ => panic!("Expected TextNote"),
    };
    let orig_pos = app.text_notes[&id].position;

    // Move via direct mutation + operation
    let before = app.text_notes[&id].clone();
    app.text_notes.get_mut(&id).unwrap().position[0] += 50.0;
    let after = app.text_notes[&id].clone();
    app.push_op(Operation::UpdateTextNote { id, before, after });

    assert!((app.text_notes[&id].position[0] - orig_pos[0] - 50.0).abs() < 0.01);

    // Undo should restore original position
    app.undo_op();
    assert!((app.text_notes[&id].position[0] - orig_pos[0]).abs() < 0.01);
}
