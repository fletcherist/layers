//! Figma-style groups: contain any mix of entity types.

use crate::entity_id::EntityId;
use crate::{Camera, InstanceRaw, HitTarget};

fn default_volume() -> f32 { 1.0 }
fn default_pan() -> f32 { 0.5 }

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Group {
    pub id: EntityId,
    pub name: String,
    pub position: [f32; 2],
    pub size: [f32; 2],
    /// Entity IDs of group members (any entity type).
    pub member_ids: Vec<EntityId>,
    #[serde(default)]
    pub effect_chain_id: Option<crate::entity_id::EntityId>,
    #[serde(default = "default_volume")]
    pub volume: f32,
    #[serde(default = "default_pan")]
    pub pan: f32,
    #[serde(default)]
    pub disabled: bool,
}

impl Group {
    pub fn new(id: EntityId, name: String, position: [f32; 2], size: [f32; 2], member_ids: Vec<EntityId>) -> Self {
        Self { id, name, position, size, member_ids, effect_chain_id: None, volume: 1.0, pan: 0.5, disabled: false }
    }
}

/// Compute a bounding box from a set of selected HitTargets, querying each entity map.
pub(crate) fn bounding_box_of_selection(
    targets: &[HitTarget],
    waveforms: &indexmap::IndexMap<EntityId, crate::ui::waveform::WaveformView>,
    midi_clips: &indexmap::IndexMap<EntityId, crate::midi::MidiClip>,
    text_notes: &indexmap::IndexMap<EntityId, crate::text_note::TextNote>,
    objects: &indexmap::IndexMap<EntityId, crate::CanvasObject>,
    loop_regions: &indexmap::IndexMap<EntityId, crate::regions::LoopRegion>,
    export_regions: &indexmap::IndexMap<EntityId, crate::regions::ExportRegion>,
    components: &indexmap::IndexMap<EntityId, crate::component::ComponentDef>,
    component_instances: &indexmap::IndexMap<EntityId, crate::component::ComponentInstance>,
    groups: &indexmap::IndexMap<EntityId, Group>,
) -> Option<([f32; 2], [f32; 2])> {
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    let mut found = false;

    for target in targets {
        let pos_size = match target {
            HitTarget::Waveform(id) => waveforms.get(id).map(|w| (w.position, w.size)),
            HitTarget::MidiClip(id) => midi_clips.get(id).map(|m| (m.position, m.size)),
            HitTarget::TextNote(id) => text_notes.get(id).map(|t| (t.position, t.size)),
            HitTarget::Object(id) => objects.get(id).map(|o| (o.position, o.size)),
            HitTarget::LoopRegion(id) => loop_regions.get(id).map(|l| (l.position, l.size)),
            HitTarget::ExportRegion(id) => export_regions.get(id).map(|x| (x.position, x.size)),
            HitTarget::ComponentDef(id) => components.get(id).map(|c| (c.position, c.size)),
            HitTarget::ComponentInstance(id) => {
                component_instances.get(id).and_then(|inst| {
                    components.values().find(|c| c.id == inst.component_id)
                        .map(|def| (inst.position, def.size))
                })
            }
            HitTarget::Group(id) => groups.get(id).map(|g| (g.position, g.size)),
            HitTarget::Instrument(_) => None,
        };
        if let Some((p, s)) = pos_size {
            found = true;
            min_x = min_x.min(p[0]);
            min_y = min_y.min(p[1]);
            max_x = max_x.max(p[0] + s[0]);
            max_y = max_y.max(p[1] + s[1]);
        }
    }

    if !found {
        return None;
    }
    Some(([min_x, min_y], [
        max_x - min_x,
        max_y - min_y,
    ]))
}

// ---------------------------------------------------------------------------
// Nested-group helpers
// ---------------------------------------------------------------------------

/// Find the direct parent group containing `entity_id`, if any.
pub(crate) fn parent_group_of(
    entity_id: EntityId,
    groups: &indexmap::IndexMap<EntityId, Group>,
) -> Option<EntityId> {
    groups.iter()
        .find(|(_, g)| g.member_ids.contains(&entity_id))
        .map(|(gid, _)| *gid)
}

/// Walk up the group hierarchy and return the chain of ancestor group IDs
/// from the immediate parent to the root: `[parent, grandparent, ...]`.
pub(crate) fn ancestor_chain(
    entity_id: EntityId,
    groups: &indexmap::IndexMap<EntityId, Group>,
) -> Vec<EntityId> {
    let mut chain = Vec::new();
    let mut current = entity_id;
    let mut visited = std::collections::HashSet::new();
    loop {
        match parent_group_of(current, groups) {
            Some(pid) => {
                if !visited.insert(pid) { break; } // cycle guard
                chain.push(pid);
                current = pid;
            }
            None => break,
        }
    }
    chain
}

/// Recursively collect all leaf (non-group) members of a group.
pub(crate) fn all_transitive_members(
    group_id: EntityId,
    groups: &indexmap::IndexMap<EntityId, Group>,
) -> Vec<EntityId> {
    let mut result = Vec::new();
    let mut visited = std::collections::HashSet::new();
    fn recurse(
        gid: EntityId,
        groups: &indexmap::IndexMap<EntityId, Group>,
        result: &mut Vec<EntityId>,
        visited: &mut std::collections::HashSet<EntityId>,
    ) {
        if !visited.insert(gid) { return; }
        if let Some(group) = groups.get(&gid) {
            for mid in &group.member_ids {
                if groups.contains_key(mid) {
                    recurse(*mid, groups, result, visited);
                } else {
                    result.push(*mid);
                }
            }
        }
    }
    recurse(group_id, groups, &mut result, &mut visited);
    result
}

/// Returns `true` if adding `candidate_id` as a member of `container_id`
/// would create a cycle (i.e. `container_id` is already a transitive member
/// of `candidate_id`, or they are the same entity).
pub(crate) fn would_create_cycle(
    container_id: EntityId,
    candidate_id: EntityId,
    groups: &indexmap::IndexMap<EntityId, Group>,
) -> bool {
    if container_id == candidate_id { return true; }
    // Check if container_id is a transitive member of candidate_id
    let mut visited = std::collections::HashSet::new();
    fn is_transitive_member(
        target: EntityId,
        current_group: EntityId,
        groups: &indexmap::IndexMap<EntityId, Group>,
        visited: &mut std::collections::HashSet<EntityId>,
    ) -> bool {
        if !visited.insert(current_group) { return false; }
        if let Some(group) = groups.get(&current_group) {
            for mid in &group.member_ids {
                if *mid == target { return true; }
                if groups.contains_key(mid) {
                    if is_transitive_member(target, *mid, groups, visited) {
                        return true;
                    }
                }
            }
        }
        false
    }
    is_transitive_member(container_id, candidate_id, groups, &mut visited)
}

/// Build rendering instances for a group (border + label badge).
pub(crate) fn build_group_instances(
    group: &Group,
    camera: &Camera,
    is_hovered: bool,
    is_selected: bool,
    theme: &crate::theme::RuntimeTheme,
) -> Vec<InstanceRaw> {
    let mut out = Vec::new();

    // Subtle fill
    let mut fill = theme.component_fill_color;
    if is_hovered || is_selected {
        fill[3] = (fill[3] + 0.03).min(1.0);
    }
    out.push(InstanceRaw {
        position: group.position,
        size: group.size,
        color: fill,
        border_radius: 4.0 / camera.zoom,
    });

    // Border — skip when selected since the global selection overlay draws
    // its own thick border + corner handles.
    if !is_selected {
        let bw = 1.0 / camera.zoom;
        let mut bc = theme.component_border_color;
        if is_hovered {
            bc[3] = (bc[3] + 0.15).min(1.0);
        }
        crate::push_border(&mut out, group.position, group.size, bw, bc);
    }

    out
}
