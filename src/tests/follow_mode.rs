use crate::entity_id::new_id;
use crate::App;

fn add_remote_user(app: &mut App, name: &str) -> crate::user::UserId {
    let uid = new_id();
    let idx = app.remote_users.len();
    app.remote_users.insert(uid, crate::user::RemoteUserState {
        user: crate::user::User {
            id: uid,
            name: name.to_string(),
            color: crate::user::color_for_user_index(idx + 1),
        },
        cursor_world: Some([100.0, 200.0]),
        drag_preview: None,
        online: true,
        viewport: Some(crate::user::RemoteViewport {
            position: [500.0, 300.0],
            zoom: 2.0,
        }),
        playback: None,
    });
    uid
}

#[test]
fn follow_unfollow_toggle() {
    let mut app = App::new_headless();
    let uid = add_remote_user(&mut app, "Alice");

    assert!(app.following_user.is_none());
    app.following_user = Some(uid);
    assert_eq!(app.following_user, Some(uid));
    app.following_user = None;
    assert!(app.following_user.is_none());
}

#[test]
fn camera_sync_from_followed_user() {
    let mut app = App::new_headless();
    let uid = add_remote_user(&mut app, "Bob");

    app.following_user = Some(uid);

    // Simulate camera sync (same logic as about_to_wait)
    if let Some(remote) = app.remote_users.get(&uid) {
        if let Some(viewport) = &remote.viewport {
            app.camera.position = viewport.position;
            app.camera.zoom = viewport.zoom;
        }
    }

    assert_eq!(app.camera.position, [500.0, 300.0]);
    assert_eq!(app.camera.zoom, 2.0);
}

#[test]
fn unfollow_on_user_leave() {
    let mut app = App::new_headless();
    let uid = add_remote_user(&mut app, "Carol");

    app.following_user = Some(uid);
    assert_eq!(app.following_user, Some(uid));

    // Simulate UserLeft
    app.remote_users.remove(&uid);
    if app.following_user == Some(uid) {
        app.following_user = None;
    }

    assert!(app.following_user.is_none());
}

#[test]
fn unfollow_on_offline() {
    let mut app = App::new_headless();
    let uid = add_remote_user(&mut app, "Dave");

    app.following_user = Some(uid);

    // Mark user offline
    if let Some(state) = app.remote_users.get_mut(&uid) {
        state.online = false;
    }

    // Simulate the follow-mode check in about_to_wait
    if let Some(followed_id) = app.following_user {
        if let Some(remote) = app.remote_users.get(&followed_id) {
            if !remote.online {
                app.following_user = None;
            }
        } else {
            app.following_user = None;
        }
    }

    assert!(app.following_user.is_none());
}

#[test]
fn camera_sync_updates_when_viewport_changes() {
    let mut app = App::new_headless();
    let uid = add_remote_user(&mut app, "Eve");

    app.following_user = Some(uid);

    // First sync
    if let Some(remote) = app.remote_users.get(&uid) {
        if let Some(vp) = &remote.viewport {
            app.camera.position = vp.position;
            app.camera.zoom = vp.zoom;
        }
    }
    assert_eq!(app.camera.position, [500.0, 300.0]);

    // Update remote viewport
    if let Some(state) = app.remote_users.get_mut(&uid) {
        state.viewport = Some(crate::user::RemoteViewport {
            position: [1000.0, 600.0],
            zoom: 0.5,
        });
    }

    // Second sync
    if let Some(remote) = app.remote_users.get(&uid) {
        if let Some(vp) = &remote.viewport {
            app.camera.position = vp.position;
            app.camera.zoom = vp.zoom;
        }
    }
    assert_eq!(app.camera.position, [1000.0, 600.0]);
    assert_eq!(app.camera.zoom, 0.5);
}

#[test]
fn hit_test_avatar_circles_finds_user() {
    let mut app = App::new_headless();
    let uid = add_remote_user(&mut app, "Frank");

    // The avatar layout: right-to-left from screen edge
    // With screen_info returning (1280, 800, 1.0) in headless mode:
    // First avatar center: x = 1280 - 12 - 14 = 1254, y = 12 + 14 + 28 = 54
    app.mouse_pos = [1254.0, 54.0];
    assert_eq!(app.hit_test_avatar_circles(), Some(uid));

    // Click outside any avatar
    app.mouse_pos = [0.0, 0.0];
    assert_eq!(app.hit_test_avatar_circles(), None);
}

#[test]
fn hit_test_avatar_no_online_users() {
    let mut app = App::new_headless();
    let uid = add_remote_user(&mut app, "Grace");

    // Mark offline
    if let Some(state) = app.remote_users.get_mut(&uid) {
        state.online = false;
    }

    app.mouse_pos = [1254.0, 54.0];
    assert_eq!(app.hit_test_avatar_circles(), None);
}
