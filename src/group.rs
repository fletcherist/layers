//! Figma-style groups: contain any mix of entity types.

use crate::entity_id::EntityId;
use crate::{Camera, InstanceRaw, HitTarget};

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
}

impl Group {
    pub fn new(id: EntityId, name: String, position: [f32; 2], size: [f32; 2], member_ids: Vec<EntityId>) -> Self {
        Self { id, name, position, size, member_ids, effect_chain_id: None }
    }
}

/// Compute a bounding box from a set of selected HitTargets, querying each entity map.
pub(crate) fn bounding_box_of_selection(
    targets: &[HitTarget],
    waveforms: &indexmap::IndexMap<EntityId, crate::ui::waveform::WaveformView>,
    midi_clips: &indexmap::IndexMap<EntityId, crate::midi::MidiClip>,
    effect_regions: &indexmap::IndexMap<EntityId, crate::effects::EffectRegion>,
    text_notes: &indexmap::IndexMap<EntityId, crate::text_note::TextNote>,
    objects: &indexmap::IndexMap<EntityId, crate::CanvasObject>,
    loop_regions: &indexmap::IndexMap<EntityId, crate::regions::LoopRegion>,
    export_regions: &indexmap::IndexMap<EntityId, crate::regions::ExportRegion>,
    components: &indexmap::IndexMap<EntityId, crate::component::ComponentDef>,
    component_instances: &indexmap::IndexMap<EntityId, crate::component::ComponentInstance>,
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
            HitTarget::EffectRegion(id) => effect_regions.get(id).map(|e| (e.position, e.size)),
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
            HitTarget::PluginBlock(_) | HitTarget::Group(_) => None,
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

    // Border
    let bw = if is_selected { 2.0 } else { 1.0 } / camera.zoom;
    let mut bc = theme.component_border_color;
    if is_hovered && !is_selected {
        bc[3] = (bc[3] + 0.15).min(1.0);
    }
    crate::push_border(&mut out, group.position, group.size, bw, bc);

    // No badge — group name is rendered as an inline text label (like waveform filenames)

    // Resize handles when selected
    if is_selected {
        let handle_sz = 8.0 / camera.zoom;
        let handle_color = theme.component_border_color;
        for &hx in &[
            group.position[0] - handle_sz * 0.5,
            group.position[0] + group.size[0] - handle_sz * 0.5,
        ] {
            for &hy in &[
                group.position[1] - handle_sz * 0.5,
                group.position[1] + group.size[1] - handle_sz * 0.5,
            ] {
                out.push(InstanceRaw {
                    position: [hx, hy],
                    size: [handle_sz, handle_sz],
                    color: handle_color,
                    border_radius: 2.0 / camera.zoom,
                });
            }
        }
    }

    out
}
