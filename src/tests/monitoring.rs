use crate::App;
use crate::entity_id::new_id;
use crate::effects;

#[test]
fn monitoring_group_id_defaults_none() {
    let app = App::new_headless();
    assert!(app.monitoring_group_id.is_none());
    assert!(app.recording_group_id.is_none());
}

#[test]
fn toggle_group_monitoring_enables() {
    let mut app = App::new_headless();
    let group_id = new_id();
    app.groups.insert(group_id, crate::group::Group::new(
        group_id, "Test".into(), [0.0, 0.0], [100.0, 50.0], vec![],
    ));
    // No audio engine in headless, but data model should work
    app.monitoring_group_id = Some(group_id);
    assert_eq!(app.monitoring_group_id, Some(group_id));
}

#[test]
fn toggle_group_monitoring_switches() {
    let mut app = App::new_headless();
    let g1 = new_id();
    let g2 = new_id();
    app.groups.insert(g1, crate::group::Group::new(g1, "G1".into(), [0.0, 0.0], [100.0, 50.0], vec![]));
    app.groups.insert(g2, crate::group::Group::new(g2, "G2".into(), [0.0, 100.0], [100.0, 50.0], vec![]));

    app.monitoring_group_id = Some(g1);
    assert_eq!(app.monitoring_group_id, Some(g1));

    // Switching to another group
    app.monitoring_group_id = Some(g2);
    assert_eq!(app.monitoring_group_id, Some(g2));

    // Toggling off
    app.monitoring_group_id = None;
    assert!(app.monitoring_group_id.is_none());
}

#[test]
fn group_inspector_shows_monitoring_state() {
    let mut app = App::new_headless();
    let group_id = new_id();
    app.groups.insert(group_id, crate::group::Group::new(
        group_id, "Vocals".into(), [0.0, 0.0], [100.0, 50.0], vec![],
    ));

    // Not monitoring — select the group and update right window
    app.monitoring_group_id = None;
    app.selected = vec![crate::HitTarget::Group(group_id)];
    app.update_right_window();
    assert!(!app.right_window.as_ref().unwrap().is_monitoring);

    // Monitoring this group
    app.monitoring_group_id = Some(group_id);
    app.update_right_window();
    assert!(app.right_window.as_ref().unwrap().is_monitoring);
}

#[test]
fn group_with_effect_chain_for_monitoring() {
    let mut app = App::new_headless();
    let group_id = new_id();
    let chain_id = new_id();
    app.effect_chains.insert(chain_id, effects::EffectChain::new());
    let mut group = crate::group::Group::new(
        group_id, "Vocals".into(), [0.0, 0.0], [100.0, 50.0], vec![],
    );
    group.effect_chain_id = Some(chain_id);
    app.groups.insert(group_id, group);

    app.monitoring_group_id = Some(group_id);
    assert_eq!(app.groups.get(&group_id).unwrap().effect_chain_id, Some(chain_id));
}
