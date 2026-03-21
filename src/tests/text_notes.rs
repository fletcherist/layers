use crate::operations::Operation;
use crate::text_note::TextNoteEditState;
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
fn text_note_cursor_arrow_up_down() {
    let mut app = App::new_headless();
    app.add_text_note();
    let id = match app.selected[0] {
        HitTarget::TextNote(id) => id,
        _ => panic!("Expected TextNote"),
    };

    // Set up multi-line text: "abc\ndef\nghi"
    let text = "abc\ndef\nghi".to_string();
    app.text_notes.get_mut(&id).unwrap().text = text.clone();
    app.editing_text_note = Some(TextNoteEditState {
        note_id: id,
        text: text.clone(),
        before_text: String::new(),
        cursor: 5, // on 'e' in second line (index: a=0,b=1,c=2,\n=3,d=4,e=5)
    });

    // Simulate ArrowUp: cursor at col 1 of line 1 -> should go to col 1 of line 0 = index 1 ('b')
    {
        let edit = app.editing_text_note.as_mut().unwrap();
        let before = &edit.text[..edit.cursor];
        if let Some(cur_line_start) = before.rfind('\n') {
            let col = edit.cursor - cur_line_start - 1;
            let prev_line_start = before[..cur_line_start].rfind('\n')
                .map(|p| p + 1).unwrap_or(0);
            let prev_line_len = cur_line_start - prev_line_start;
            edit.cursor = prev_line_start + col.min(prev_line_len);
        }
    }
    assert_eq!(app.editing_text_note.as_ref().unwrap().cursor, 1); // 'b'

    // Simulate ArrowDown from cursor=1 (col 1 line 0) -> col 1 line 1 = index 5 ('e')
    {
        let edit = app.editing_text_note.as_mut().unwrap();
        let before = &edit.text[..edit.cursor];
        let cur_line_start = before.rfind('\n').map(|p| p + 1).unwrap_or(0);
        let col = edit.cursor - cur_line_start;
        if let Some(next_nl) = edit.text[edit.cursor..].find('\n') {
            let next_line_start = edit.cursor + next_nl + 1;
            let next_line_end = edit.text[next_line_start..].find('\n')
                .map(|p| next_line_start + p)
                .unwrap_or(edit.text.len());
            let next_line_len = next_line_end - next_line_start;
            edit.cursor = next_line_start + col.min(next_line_len);
        }
    }
    assert_eq!(app.editing_text_note.as_ref().unwrap().cursor, 5); // 'e'

    // ArrowDown again from cursor=5 (col 1 line 1) -> col 1 line 2 = index 9 ('h')
    {
        let edit = app.editing_text_note.as_mut().unwrap();
        let before = &edit.text[..edit.cursor];
        let cur_line_start = before.rfind('\n').map(|p| p + 1).unwrap_or(0);
        let col = edit.cursor - cur_line_start;
        if let Some(next_nl) = edit.text[edit.cursor..].find('\n') {
            let next_line_start = edit.cursor + next_nl + 1;
            let next_line_end = edit.text[next_line_start..].find('\n')
                .map(|p| next_line_start + p)
                .unwrap_or(edit.text.len());
            let next_line_len = next_line_end - next_line_start;
            edit.cursor = next_line_start + col.min(next_line_len);
        }
    }
    assert_eq!(app.editing_text_note.as_ref().unwrap().cursor, 9); // 'h'
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
