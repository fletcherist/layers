use crate::entity_id::new_id;
use crate::ui::palette::CommandAction;
use crate::{App, CanvasObject, HitTarget};

#[test]
fn create_group_from_selection() {
    let mut app = App::new_headless();

    // Add two objects
    let id1 = new_id();
    let id2 = new_id();
    app.objects.insert(id1, CanvasObject {
        position: [0.0, 0.0],
        size: [100.0, 50.0],
        color: [1.0, 0.0, 0.0, 1.0],
        border_radius: 0.0,
    });
    app.objects.insert(id2, CanvasObject {
        position: [200.0, 0.0],
        size: [100.0, 50.0],
        color: [0.0, 1.0, 0.0, 1.0],
        border_radius: 0.0,
    });

    // Select both
    app.selected.push(HitTarget::Object(id1));
    app.selected.push(HitTarget::Object(id2));

    // Execute CreateGroup
    app.execute_command(CommandAction::CreateGroup);

    // Should have one group
    assert_eq!(app.groups.len(), 1);
    let group = app.groups.values().next().unwrap();
    assert_eq!(group.member_ids.len(), 2);
    assert!(group.member_ids.contains(&id1));
    assert!(group.member_ids.contains(&id2));

    // Selection should now be the group
    assert_eq!(app.selected.len(), 1);
    assert!(matches!(app.selected[0], HitTarget::Group(_)));
}

#[test]
fn ungroup_selected_restores_members() {
    let mut app = App::new_headless();

    // Add two objects
    let id1 = new_id();
    let id2 = new_id();
    app.objects.insert(id1, CanvasObject {
        position: [0.0, 0.0],
        size: [100.0, 50.0],
        color: [1.0, 0.0, 0.0, 1.0],
        border_radius: 0.0,
    });
    app.objects.insert(id2, CanvasObject {
        position: [200.0, 0.0],
        size: [100.0, 50.0],
        color: [0.0, 1.0, 0.0, 1.0],
        border_radius: 0.0,
    });

    // Select both and create group
    app.selected.push(HitTarget::Object(id1));
    app.selected.push(HitTarget::Object(id2));
    app.execute_command(CommandAction::CreateGroup);
    assert_eq!(app.groups.len(), 1);

    // Now ungroup
    app.execute_command(CommandAction::UngroupSelected);

    // Group should be removed
    assert_eq!(app.groups.len(), 0);

    // Selection should contain the former members
    assert_eq!(app.selected.len(), 2);
    assert!(app.selected.contains(&HitTarget::Object(id1)));
    assert!(app.selected.contains(&HitTarget::Object(id2)));
}

#[test]
fn create_group_requires_at_least_two() {
    let mut app = App::new_headless();

    let id1 = new_id();
    app.objects.insert(id1, CanvasObject {
        position: [0.0, 0.0],
        size: [100.0, 50.0],
        color: [1.0, 0.0, 0.0, 1.0],
        border_radius: 0.0,
    });

    // Select only one
    app.selected.push(HitTarget::Object(id1));
    app.execute_command(CommandAction::CreateGroup);

    // No group should be created
    assert_eq!(app.groups.len(), 0);
}

#[test]
fn select_group_opens_right_window() {
    let mut app = App::new_headless();

    // Add two objects and create a group
    let id1 = new_id();
    let id2 = new_id();
    app.objects.insert(id1, CanvasObject {
        position: [0.0, 0.0],
        size: [100.0, 50.0],
        color: [1.0, 0.0, 0.0, 1.0],
        border_radius: 0.0,
    });
    app.objects.insert(id2, CanvasObject {
        position: [200.0, 0.0],
        size: [100.0, 50.0],
        color: [0.0, 1.0, 0.0, 1.0],
        border_radius: 0.0,
    });
    app.selected.push(HitTarget::Object(id1));
    app.selected.push(HitTarget::Object(id2));
    app.execute_command(CommandAction::CreateGroup);
    assert_eq!(app.groups.len(), 1);

    let group_id = app.groups.keys().next().copied().unwrap();

    // Select the group and update right window
    app.selected.clear();
    app.selected.push(HitTarget::Group(group_id));
    app.update_right_window();

    // Right window should be open with Group target
    let rw = app.right_window.as_ref().expect("right window should be open");
    assert!(rw.is_group());
    assert_eq!(rw.target_id(), group_id);
    assert_eq!(rw.group_name, "Group 1");
    assert_eq!(rw.group_member_count, 2);
}

#[test]
fn rename_group_via_browser_inline_edit() {
    let mut app = App::new_headless();

    // Add two objects and create a group
    let id1 = new_id();
    let id2 = new_id();
    app.objects.insert(id1, CanvasObject {
        position: [0.0, 0.0],
        size: [100.0, 50.0],
        color: [1.0, 0.0, 0.0, 1.0],
        border_radius: 0.0,
    });
    app.objects.insert(id2, CanvasObject {
        position: [200.0, 0.0],
        size: [100.0, 50.0],
        color: [0.0, 1.0, 0.0, 1.0],
        border_radius: 0.0,
    });
    app.selected.push(HitTarget::Object(id1));
    app.selected.push(HitTarget::Object(id2));
    app.execute_command(CommandAction::CreateGroup);
    assert_eq!(app.groups.len(), 1);

    let group_id = app.groups.keys().next().copied().unwrap();
    assert_eq!(app.groups[&group_id].name, "Group 1");

    // Simulate inline rename: set editing state then commit
    app.sample_browser.editing_browser_name = Some((
        group_id,
        crate::layers::LayerNodeKind::Group,
        "My Custom Group".to_string(),
    ));

    // Commit by directly applying the same logic as Enter key handler
    let before = app.groups[&group_id].clone();
    app.groups.get_mut(&group_id).unwrap().name = "My Custom Group".to_string();
    let after = app.groups[&group_id].clone();
    app.push_op(crate::operations::Operation::UpdateGroup { id: group_id, before, after });
    app.sample_browser.editing_browser_name = None;

    assert_eq!(app.groups[&group_id].name, "My Custom Group");

    // Undo should revert to original name
    app.undo_op();
    assert_eq!(app.groups[&group_id].name, "Group 1");

    // Redo should restore the new name
    app.redo_op();
    assert_eq!(app.groups[&group_id].name, "My Custom Group");
}
