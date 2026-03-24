use crate::entity_id::EntityId;
use crate::ui::hit_testing::rects_overlap;
use indexmap::IndexMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LayerNodeKind {
    Instrument,
    MidiClip,
    Waveform,
    EffectRegion,
    PluginBlock,
    TextNote,
    Group,
}

impl LayerNodeKind {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Instrument => "instrument",
            Self::MidiClip => "midi_clip",
            Self::Waveform => "waveform",
            Self::EffectRegion => "effect_region",
            Self::PluginBlock => "plugin_block",
            Self::TextNote => "text_note",
            Self::Group => "group",
        }
    }

    pub fn from_tag(s: &str) -> Option<Self> {
        match s {
            "instrument" => Some(Self::Instrument),
            "midi_clip" => Some(Self::MidiClip),
            "waveform" => Some(Self::Waveform),
            "effect_region" => Some(Self::EffectRegion),
            "plugin_block" => Some(Self::PluginBlock),
            "text_note" => Some(Self::TextNote),
            "group" => Some(Self::Group),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LayerNode {
    pub entity_id: EntityId,
    pub kind: LayerNodeKind,
    pub expanded: bool,
    pub children: Vec<LayerNode>,
}

/// Flat row produced by flattening the tree for display in the browser.
#[derive(Clone, Debug)]
pub struct FlatLayerRow {
    pub entity_id: EntityId,
    pub kind: LayerNodeKind,
    pub depth: usize,
    pub has_children: bool,
    pub expanded: bool,
    pub label: String,
    pub color: [f32; 4],
}

/// Determine the LayerNodeKind for a member entity by checking which map contains it.
fn member_kind(
    id: EntityId,
    instruments: &IndexMap<EntityId, crate::instruments::Instrument>,
    midi_clips: &IndexMap<EntityId, crate::midi::MidiClip>,
    waveforms: &IndexMap<EntityId, crate::ui::waveform::WaveformView>,
    effect_regions: &IndexMap<EntityId, crate::effects::EffectRegion>,
    plugin_blocks: &IndexMap<EntityId, crate::effects::PluginBlock>,
) -> Option<LayerNodeKind> {
    if instruments.contains_key(&id) { Some(LayerNodeKind::Instrument) }
    else if midi_clips.contains_key(&id) { Some(LayerNodeKind::MidiClip) }
    else if waveforms.contains_key(&id) { Some(LayerNodeKind::Waveform) }
    else if effect_regions.contains_key(&id) { Some(LayerNodeKind::EffectRegion) }
    else if plugin_blocks.contains_key(&id) { Some(LayerNodeKind::PluginBlock) }
    else { None }
}

/// Build the default layer tree from current App entity maps.
/// Instruments (sorted by Y) with MIDI children; waveforms (sorted by Y);
/// effect regions (sorted by Y) with plugin block children.
pub fn build_default_tree(
    instruments: &IndexMap<EntityId, crate::instruments::Instrument>,
    midi_clips: &IndexMap<EntityId, crate::midi::MidiClip>,
    waveforms: &IndexMap<EntityId, crate::ui::waveform::WaveformView>,
    effect_regions: &IndexMap<EntityId, crate::effects::EffectRegion>,
    plugin_blocks: &IndexMap<EntityId, crate::effects::PluginBlock>,
    groups: &IndexMap<EntityId, crate::group::Group>,
) -> Vec<LayerNode> {
    let mut tree = Vec::new();

    // Instruments (lightweight) sorted by insertion order
    for (&inst_id, _) in instruments.iter() {
        let children: Vec<LayerNode> = midi_clips.iter()
            .filter(|(_, mc)| mc.instrument_id == Some(inst_id))
            .map(|(&mc_id, _)| LayerNode {
                entity_id: mc_id,
                kind: LayerNodeKind::MidiClip,
                expanded: false,
                children: Vec::new(),
            })
            .collect();
        tree.push(LayerNode {
            entity_id: inst_id,
            kind: LayerNodeKind::Instrument,
            expanded: true,
            children,
        });
    }

    // Waveforms sorted by Y
    let mut wfs: Vec<(EntityId, f32)> = waveforms.iter()
        .map(|(&id, wf)| (id, wf.position[1]))
        .collect();
    wfs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    for (wf_id, _) in &wfs {
        tree.push(LayerNode {
            entity_id: *wf_id,
            kind: LayerNodeKind::Waveform,
            expanded: false,
            children: Vec::new(),
        });
    }

    // Effect regions sorted by Y with plugin block children
    let mut ers: Vec<(EntityId, f32)> = effect_regions.iter()
        .map(|(&id, er)| (id, er.position[1]))
        .collect();
    ers.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    for (er_id, _) in &ers {
        let er = &effect_regions[er_id];
        let mut children: Vec<(EntityId, f32)> = plugin_blocks.iter()
            .filter(|(_, pb)| !pb.bypass && rects_overlap(er.position, er.size, pb.position, pb.size))
            .map(|(&pb_id, pb)| (pb_id, pb.position[0]))
            .collect();
        children.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        tree.push(LayerNode {
            entity_id: *er_id,
            kind: LayerNodeKind::EffectRegion,
            expanded: true,
            children: children.into_iter().map(|(pb_id, _)| LayerNode {
                entity_id: pb_id,
                kind: LayerNodeKind::PluginBlock,
                expanded: false,
                children: Vec::new(),
            }).collect(),
        });
    }

    // Groups
    for (&group_id, group) in groups.iter() {
        let children: Vec<LayerNode> = group.member_ids.iter().filter_map(|mid| {
            let kind = member_kind(*mid, instruments, midi_clips, waveforms, effect_regions, plugin_blocks);
            kind.map(|k| LayerNode { entity_id: *mid, kind: k, expanded: false, children: Vec::new() })
        }).collect();
        tree.push(LayerNode {
            entity_id: group_id,
            kind: LayerNodeKind::Group,
            expanded: true,
            children,
        });
    }

    tree
}

/// Flatten a tree of LayerNodes into display rows, respecting expanded state.
pub fn flatten_tree(
    tree: &[LayerNode],
    instruments: &IndexMap<EntityId, crate::instruments::Instrument>,
    midi_clips: &IndexMap<EntityId, crate::midi::MidiClip>,
    waveforms: &IndexMap<EntityId, crate::ui::waveform::WaveformView>,
    effect_regions: &IndexMap<EntityId, crate::effects::EffectRegion>,
    plugin_blocks: &IndexMap<EntityId, crate::effects::PluginBlock>,
    groups: &IndexMap<EntityId, crate::group::Group>,
) -> Vec<FlatLayerRow> {
    let mut rows = Vec::new();
    for node in tree {
        flatten_node(node, 0, &mut rows, instruments, midi_clips, waveforms, effect_regions, plugin_blocks, groups);
    }
    rows
}

fn flatten_node(
    node: &LayerNode,
    depth: usize,
    rows: &mut Vec<FlatLayerRow>,
    instruments: &IndexMap<EntityId, crate::instruments::Instrument>,
    midi_clips: &IndexMap<EntityId, crate::midi::MidiClip>,
    waveforms: &IndexMap<EntityId, crate::ui::waveform::WaveformView>,
    effect_regions: &IndexMap<EntityId, crate::effects::EffectRegion>,
    plugin_blocks: &IndexMap<EntityId, crate::effects::PluginBlock>,
    groups: &IndexMap<EntityId, crate::group::Group>,
) {
    let label = match node.kind {
        LayerNodeKind::Instrument => {
            if let Some(inst) = instruments.get(&node.entity_id) {
                if !inst.name.is_empty() && inst.name != "instrument" { inst.name.clone() }
                else if !inst.plugin_name.is_empty() { inst.plugin_name.clone() }
                else { format!("Instrument {}", node.entity_id) }
            } else {
                format!("Instrument {}", node.entity_id)
            }
        }
        LayerNodeKind::MidiClip => {
            midi_clips.get(&node.entity_id).map(|mc| {
                let n = mc.notes.len();
                format!("MIDI ({} note{})", n, if n == 1 { "" } else { "s" })
            }).unwrap_or_else(|| "MIDI".to_string())
        }
        LayerNodeKind::Waveform => {
            waveforms.get(&node.entity_id).map(|wf| {
                if !wf.audio.filename.is_empty() { wf.audio.filename.clone() } else { wf.filename.clone() }
            }).unwrap_or_else(|| "Audio".to_string())
        }
        LayerNodeKind::EffectRegion => {
            effect_regions.get(&node.entity_id).map(|er| er.name.clone())
                .unwrap_or_else(|| "Effect".to_string())
        }
        LayerNodeKind::PluginBlock => {
            plugin_blocks.get(&node.entity_id).map(|pb| pb.plugin_name.clone())
                .unwrap_or_else(|| "Plugin".to_string())
        }
        LayerNodeKind::TextNote => "Text Note".to_string(),
        LayerNodeKind::Group => {
            groups.get(&node.entity_id).map(|g| g.name.clone())
                .unwrap_or_else(|| "Group".to_string())
        }
    };

    let color = match node.kind {
        LayerNodeKind::Waveform => {
            waveforms.get(&node.entity_id).map(|wf| wf.color).unwrap_or([0.5, 0.5, 0.5, 1.0])
        }
        LayerNodeKind::MidiClip => {
            midi_clips.get(&node.entity_id).map(|mc| mc.color).unwrap_or([0.5, 0.5, 0.5, 1.0])
        }
        LayerNodeKind::PluginBlock => {
            plugin_blocks.get(&node.entity_id).map(|pb| pb.color).unwrap_or([0.5, 0.5, 0.5, 1.0])
        }
        LayerNodeKind::Instrument => [0.5, 0.5, 0.5, 1.0],
        LayerNodeKind::EffectRegion => [0.5, 0.5, 0.5, 1.0],
        LayerNodeKind::TextNote => [0.6, 0.6, 0.5, 1.0],
        LayerNodeKind::Group => [0.5, 0.5, 0.5, 1.0],
    };

    rows.push(FlatLayerRow {
        entity_id: node.entity_id,
        kind: node.kind,
        depth,
        has_children: !node.children.is_empty(),
        expanded: node.expanded,
        label,
        color,
    });

    if node.expanded {
        for child in &node.children {
            flatten_node(child, depth + 1, rows, instruments, midi_clips, waveforms, effect_regions, plugin_blocks, groups);
        }
    }
}

/// Ensure the tree contains all current entities and removes stale ones.
/// Preserves existing order and expanded state where possible.
pub fn sync_tree(
    tree: &mut Vec<LayerNode>,
    instruments: &IndexMap<EntityId, crate::instruments::Instrument>,
    midi_clips: &IndexMap<EntityId, crate::midi::MidiClip>,
    waveforms: &IndexMap<EntityId, crate::ui::waveform::WaveformView>,
    effect_regions: &IndexMap<EntityId, crate::effects::EffectRegion>,
    plugin_blocks: &IndexMap<EntityId, crate::effects::PluginBlock>,
    groups: &IndexMap<EntityId, crate::group::Group>,
) {
    let mut seen_ids: std::collections::HashSet<EntityId> = std::collections::HashSet::new();

    // Phase 1: remove stale root nodes
    tree.retain(|node| {
        match node.kind {
            LayerNodeKind::Instrument => instruments.contains_key(&node.entity_id),
            LayerNodeKind::Waveform => waveforms.contains_key(&node.entity_id),
            LayerNodeKind::EffectRegion => effect_regions.contains_key(&node.entity_id),
            LayerNodeKind::MidiClip => midi_clips.contains_key(&node.entity_id),
            LayerNodeKind::PluginBlock => plugin_blocks.contains_key(&node.entity_id),
            LayerNodeKind::TextNote => true, // text notes not tracked in tree yet
            LayerNodeKind::Group => groups.contains_key(&node.entity_id),
        }
    });

    // Phase 2: sync children and track seen IDs
    for node in tree.iter_mut() {
        seen_ids.insert(node.entity_id);

        if node.kind == LayerNodeKind::Instrument {
            node.children.retain(|c| midi_clips.contains_key(&c.entity_id));
            for c in &node.children { seen_ids.insert(c.entity_id); }
            let node_id = node.entity_id;
            let existing: std::collections::HashSet<EntityId> = node.children.iter().map(|c| c.entity_id).collect();
            for (&mc_id, mc) in midi_clips.iter() {
                if mc.instrument_id == Some(node_id) && !existing.contains(&mc_id) {
                    node.children.push(LayerNode {
                        entity_id: mc_id, kind: LayerNodeKind::MidiClip, expanded: false, children: Vec::new(),
                    });
                }
            }
            for c in &node.children { seen_ids.insert(c.entity_id); }
        } else if node.kind == LayerNodeKind::EffectRegion {
            if let Some(er) = effect_regions.get(&node.entity_id) {
                node.children.retain(|c| plugin_blocks.contains_key(&c.entity_id));
                let existing: std::collections::HashSet<EntityId> = node.children.iter().map(|c| c.entity_id).collect();
                for (&pb_id, pb) in plugin_blocks.iter() {
                    if !pb.bypass && rects_overlap(er.position, er.size, pb.position, pb.size) && !existing.contains(&pb_id) {
                        node.children.push(LayerNode {
                            entity_id: pb_id, kind: LayerNodeKind::PluginBlock, expanded: false, children: Vec::new(),
                        });
                    }
                }
                for c in &node.children { seen_ids.insert(c.entity_id); }
            }
        } else if node.kind == LayerNodeKind::Group {
            if let Some(group) = groups.get(&node.entity_id) {
                // Retain only children whose member still exists in some entity map
                node.children.retain(|c| {
                    member_kind(c.entity_id, instruments, midi_clips, waveforms, effect_regions, plugin_blocks).is_some()
                });
                let existing: std::collections::HashSet<EntityId> = node.children.iter().map(|c| c.entity_id).collect();
                for mid in &group.member_ids {
                    if !existing.contains(mid) {
                        if let Some(k) = member_kind(*mid, instruments, midi_clips, waveforms, effect_regions, plugin_blocks) {
                            node.children.push(LayerNode { entity_id: *mid, kind: k, expanded: false, children: Vec::new() });
                        }
                    }
                }
                for c in &node.children { seen_ids.insert(c.entity_id); }
            }
        }
    }

    // Phase 3: add new root-level entities not yet in the tree
    for &id in instruments.keys() {
        if !seen_ids.contains(&id) {
            let mut children = Vec::new();
            for (&mc_id, mc) in midi_clips.iter() {
                if mc.instrument_id == Some(id) && !seen_ids.contains(&mc_id) {
                    children.push(LayerNode { entity_id: mc_id, kind: LayerNodeKind::MidiClip, expanded: false, children: Vec::new() });
                    seen_ids.insert(mc_id);
                }
            }
            tree.push(LayerNode { entity_id: id, kind: LayerNodeKind::Instrument, expanded: true, children });
            seen_ids.insert(id);
        }
    }
    // InstrumentRegion fallback removed — instruments are the sole source
    for &id in waveforms.keys() {
        if !seen_ids.contains(&id) {
            tree.push(LayerNode { entity_id: id, kind: LayerNodeKind::Waveform, expanded: false, children: Vec::new() });
            seen_ids.insert(id);
        }
    }
    for &id in effect_regions.keys() {
        if !seen_ids.contains(&id) {
            let er = &effect_regions[&id];
            let mut children = Vec::new();
            for (&pb_id, pb) in plugin_blocks.iter() {
                if !pb.bypass && rects_overlap(er.position, er.size, pb.position, pb.size) && !seen_ids.contains(&pb_id) {
                    children.push(LayerNode { entity_id: pb_id, kind: LayerNodeKind::PluginBlock, expanded: false, children: Vec::new() });
                    seen_ids.insert(pb_id);
                }
            }
            tree.push(LayerNode { entity_id: id, kind: LayerNodeKind::EffectRegion, expanded: true, children });
            seen_ids.insert(id);
        }
    }
    for (&id, group) in groups.iter() {
        if !seen_ids.contains(&id) {
            let children: Vec<LayerNode> = group.member_ids.iter().filter_map(|mid| {
                if seen_ids.contains(mid) { return None; }
                let k = member_kind(*mid, instruments, midi_clips, waveforms, effect_regions, plugin_blocks)?;
                seen_ids.insert(*mid);
                Some(LayerNode { entity_id: *mid, kind: k, expanded: false, children: Vec::new() })
            }).collect();
            tree.push(LayerNode { entity_id: id, kind: LayerNodeKind::Group, expanded: true, children });
            seen_ids.insert(id);
        }
    }
}

/// Move a root-level node up by one position. Returns true if moved.
pub fn move_node_up(tree: &mut Vec<LayerNode>, entity_id: EntityId) -> bool {
    if let Some(idx) = tree.iter().position(|n| n.entity_id == entity_id) {
        if idx > 0 {
            tree.swap(idx, idx - 1);
            return true;
        }
    }
    // Try children
    for node in tree.iter_mut() {
        if let Some(idx) = node.children.iter().position(|c| c.entity_id == entity_id) {
            if idx > 0 {
                node.children.swap(idx, idx - 1);
                return true;
            }
        }
    }
    false
}

/// Move a root-level node down by one position. Returns true if moved.
pub fn move_node_down(tree: &mut Vec<LayerNode>, entity_id: EntityId) -> bool {
    if let Some(idx) = tree.iter().position(|n| n.entity_id == entity_id) {
        if idx + 1 < tree.len() {
            tree.swap(idx, idx + 1);
            return true;
        }
    }
    for node in tree.iter_mut() {
        if let Some(idx) = node.children.iter().position(|c| c.entity_id == entity_id) {
            if idx + 1 < node.children.len() {
                node.children.swap(idx, idx + 1);
                return true;
            }
        }
    }
    false
}

/// Toggle expanded state for a node.
pub fn toggle_expanded(tree: &mut [LayerNode], entity_id: EntityId) {
    for node in tree.iter_mut() {
        if node.entity_id == entity_id {
            node.expanded = !node.expanded;
            return;
        }
        for child in node.children.iter_mut() {
            if child.entity_id == entity_id {
                child.expanded = !child.expanded;
                return;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Storage conversion
// ---------------------------------------------------------------------------

pub fn tree_to_stored(tree: &[LayerNode]) -> Vec<crate::storage::StoredLayerNode> {
    let mut out = Vec::new();
    for node in tree {
        out.push(crate::storage::StoredLayerNode {
            entity_id: node.entity_id.to_string(),
            kind_tag: node.kind.tag().to_string(),
            parent_entity_id: String::new(),
            expanded: node.expanded,
        });
        for child in &node.children {
            out.push(crate::storage::StoredLayerNode {
                entity_id: child.entity_id.to_string(),
                kind_tag: child.kind.tag().to_string(),
                parent_entity_id: node.entity_id.to_string(),
                expanded: child.expanded,
            });
        }
    }
    out
}

pub fn tree_from_stored(stored: &[crate::storage::StoredLayerNode]) -> Vec<LayerNode> {
    use crate::entity_id;

    let mut roots: Vec<LayerNode> = Vec::new();
    let mut children_map: std::collections::HashMap<EntityId, Vec<LayerNode>> = std::collections::HashMap::new();

    for s in stored {
        let entity_id = match s.entity_id.parse::<EntityId>() {
            Ok(id) => id,
            Err(_) => continue,
        };
        let kind = match LayerNodeKind::from_tag(&s.kind_tag) {
            Some(k) => k,
            None => continue,
        };
        let node = LayerNode { entity_id, kind, expanded: s.expanded, children: Vec::new() };

        if s.parent_entity_id.is_empty() {
            roots.push(node);
        } else {
            let parent_id = s.parent_entity_id.parse::<EntityId>()
                .unwrap_or_else(|_| entity_id::new_id());
            children_map.entry(parent_id).or_default().push(node);
        }
    }

    // Attach children
    for root in &mut roots {
        if let Some(kids) = children_map.remove(&root.entity_id) {
            root.children = kids;
        }
    }

    roots
}
