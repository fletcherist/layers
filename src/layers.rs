use crate::entity_id::EntityId;
use indexmap::IndexMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LayerNodeKind {
    Instrument,
    MidiClip,
    Waveform,
    TextNote,
    Group,
}

impl LayerNodeKind {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Instrument => "instrument",
            Self::MidiClip => "midi_clip",
            Self::Waveform => "waveform",
            Self::TextNote => "text_note",
            Self::Group => "group",
        }
    }

    pub fn from_tag(s: &str) -> Option<Self> {
        match s {
            "instrument" => Some(Self::Instrument),
            "midi_clip" => Some(Self::MidiClip),
            "waveform" => Some(Self::Waveform),
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
    pub is_soloed: bool,
    pub is_muted: bool,
    pub is_monitoring: bool,
}

/// Determine the LayerNodeKind for a member entity by checking which map contains it.
fn member_kind(
    id: EntityId,
    instruments: &IndexMap<EntityId, crate::instruments::Instrument>,
    midi_clips: &IndexMap<EntityId, crate::midi::MidiClip>,
    waveforms: &IndexMap<EntityId, crate::ui::waveform::WaveformView>,
    groups: &IndexMap<EntityId, crate::group::Group>,
) -> Option<LayerNodeKind> {
    if groups.contains_key(&id) { Some(LayerNodeKind::Group) }
    else if instruments.contains_key(&id) { Some(LayerNodeKind::Instrument) }
    else if midi_clips.contains_key(&id) { Some(LayerNodeKind::MidiClip) }
    else if waveforms.contains_key(&id) { Some(LayerNodeKind::Waveform) }
    else { None }
}

/// Build the default layer tree from current App entity maps.
pub fn build_default_tree(
    instruments: &IndexMap<EntityId, crate::instruments::Instrument>,
    midi_clips: &IndexMap<EntityId, crate::midi::MidiClip>,
    waveforms: &IndexMap<EntityId, crate::ui::waveform::WaveformView>,
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

    // Collect all child take IDs so we can skip them from top-level
    let child_take_ids: std::collections::HashSet<EntityId> = waveforms.iter()
        .filter_map(|(_, wf)| wf.take_group.as_ref())
        .flat_map(|tg| tg.take_ids.iter().copied())
        .collect();

    // Waveforms sorted by Y (skip child takes — they appear under their parent)
    let mut wfs: Vec<(EntityId, f32)> = waveforms.iter()
        .filter(|(&id, _)| !child_take_ids.contains(&id))
        .map(|(&id, wf)| (id, wf.position[1]))
        .collect();
    wfs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    for (wf_id, _) in &wfs {
        // If this waveform has takes, add children
        let children = if let Some(tg) = waveforms.get(wf_id).and_then(|wf| wf.take_group.as_ref()) {
            tg.take_ids.iter().filter_map(|&cid| {
                waveforms.get(&cid).map(|_| LayerNode {
                    entity_id: cid,
                    kind: LayerNodeKind::Waveform,
                    expanded: false,
                    children: Vec::new(),
                })
            }).collect()
        } else {
            Vec::new()
        };
        let has_takes = !children.is_empty();
        tree.push(LayerNode {
            entity_id: *wf_id,
            kind: LayerNodeKind::Waveform,
            expanded: has_takes, // auto-expand if has takes
            children,
        });
    }

    // Groups — only root-level groups (not members of another group)
    let child_group_ids: std::collections::HashSet<EntityId> = groups.values()
        .flat_map(|g| g.member_ids.iter().copied())
        .filter(|mid| groups.contains_key(mid))
        .collect();

    for (&group_id, _) in groups.iter() {
        if child_group_ids.contains(&group_id) { continue; }
        let mut visited = std::collections::HashSet::new();
        tree.push(build_group_subtree(group_id, groups, instruments, midi_clips, waveforms, &mut visited));
    }

    tree
}

/// Recursively build a LayerNode for a group and all its children (including nested groups).
fn build_group_subtree(
    group_id: EntityId,
    groups: &IndexMap<EntityId, crate::group::Group>,
    instruments: &IndexMap<EntityId, crate::instruments::Instrument>,
    midi_clips: &IndexMap<EntityId, crate::midi::MidiClip>,
    waveforms: &IndexMap<EntityId, crate::ui::waveform::WaveformView>,
    visited: &mut std::collections::HashSet<EntityId>,
) -> LayerNode {
    visited.insert(group_id);
    let children = if let Some(group) = groups.get(&group_id) {
        let member_set: std::collections::HashSet<EntityId> = group.member_ids.iter().copied().collect();
        group.member_ids.iter().filter_map(|mid| {
            // Nested group — recurse
            if groups.contains_key(mid) {
                if visited.contains(mid) { return None; } // cycle guard
                return Some(build_group_subtree(*mid, groups, instruments, midi_clips, waveforms, visited));
            }
            let kind = member_kind(*mid, instruments, midi_clips, waveforms, groups);
            kind.and_then(|k| {
                // Skip MIDI clips whose parent instrument is also in this group
                if k == LayerNodeKind::MidiClip {
                    if let Some(mc) = midi_clips.get(mid) {
                        if let Some(inst_id) = mc.instrument_id {
                            if member_set.contains(&inst_id) {
                                return None;
                            }
                        }
                    }
                }
                let mut node = LayerNode { entity_id: *mid, kind: k, expanded: false, children: Vec::new() };
                // Instruments inside groups get their MIDI clip sub-children
                if k == LayerNodeKind::Instrument {
                    node.children = midi_clips.iter()
                        .filter(|(_, mc)| mc.instrument_id == Some(*mid))
                        .map(|(&mc_id, _)| LayerNode {
                            entity_id: mc_id, kind: LayerNodeKind::MidiClip, expanded: false, children: Vec::new(),
                        })
                        .collect();
                    if !node.children.is_empty() { node.expanded = true; }
                }
                Some(node)
            })
        }).collect()
    } else {
        Vec::new()
    };
    LayerNode {
        entity_id: group_id,
        kind: LayerNodeKind::Group,
        expanded: true,
        children,
    }
}

/// Flatten a tree of LayerNodes into display rows, respecting expanded state.
pub fn flatten_tree(
    tree: &[LayerNode],
    instruments: &IndexMap<EntityId, crate::instruments::Instrument>,
    midi_clips: &IndexMap<EntityId, crate::midi::MidiClip>,
    waveforms: &IndexMap<EntityId, crate::ui::waveform::WaveformView>,
    groups: &IndexMap<EntityId, crate::group::Group>,
    solo_ids: &std::collections::HashSet<EntityId>,
    monitoring_group_id: Option<EntityId>,
    keyboard_preview_target: Option<EntityId>,
) -> Vec<FlatLayerRow> {
    let mut rows = Vec::new();
    for node in tree {
        flatten_node(node, 0, &mut rows, instruments, midi_clips, waveforms, groups, solo_ids, monitoring_group_id, keyboard_preview_target);
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
    groups: &IndexMap<EntityId, crate::group::Group>,
    solo_ids: &std::collections::HashSet<EntityId>,
    monitoring_group_id: Option<EntityId>,
    keyboard_preview_target: Option<EntityId>,
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
            // Check if this waveform is a child take — label as "Take N"
            let take_label = waveforms.iter().find_map(|(_, pw)| {
                pw.take_group.as_ref().and_then(|tg| {
                    tg.take_ids.iter().position(|id| *id == node.entity_id)
                        .map(|pos| format!("Take {}", pos + 1))
                })
            });
            take_label.unwrap_or_else(|| {
                waveforms.get(&node.entity_id).map(|wf| {
                    if !wf.audio.filename.is_empty() { wf.audio.filename.clone() } else { wf.filename.clone() }
                }).unwrap_or_else(|| "Audio".to_string())
            })
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
        LayerNodeKind::Instrument => [0.5, 0.5, 0.5, 1.0],
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
        is_soloed: solo_ids.contains(&node.entity_id),
        is_muted: {
            let id = node.entity_id;
            waveforms.get(&id).map_or(false, |wf| wf.disabled)
                || instruments.get(&id).map_or(false, |inst| inst.disabled)
                || groups.get(&id).map_or(false, |g| g.disabled)
        },
        is_monitoring: (node.kind == LayerNodeKind::Group && monitoring_group_id == Some(node.entity_id))
            || (node.kind == LayerNodeKind::Instrument && keyboard_preview_target == Some(node.entity_id)),
    });

    if node.expanded {
        for child in &node.children {
            flatten_node(child, depth + 1, rows, instruments, midi_clips, waveforms, groups, solo_ids, monitoring_group_id, keyboard_preview_target);
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
    groups: &IndexMap<EntityId, crate::group::Group>,
) {
    let mut seen_ids: std::collections::HashSet<EntityId> = std::collections::HashSet::new();

    // Collect all entity IDs that belong to a group — these should not appear as root nodes
    let grouped_ids: std::collections::HashSet<EntityId> = groups.values()
        .flat_map(|g| g.member_ids.iter().copied())
        .collect();

    // Child group IDs — groups that are members of another group
    let child_group_ids: std::collections::HashSet<EntityId> = groups.values()
        .flat_map(|g| g.member_ids.iter().copied())
        .filter(|mid| groups.contains_key(mid))
        .collect();

    // Collect all child take IDs — these should not appear as root nodes
    let child_take_ids: std::collections::HashSet<EntityId> = waveforms.iter()
        .filter_map(|(_, wf)| wf.take_group.as_ref())
        .flat_map(|tg| tg.take_ids.iter().copied())
        .collect();

    // Phase 1: remove stale root nodes + remove root entries that belong to a group or are child takes
    tree.retain(|node| {
        if node.kind != LayerNodeKind::Group && grouped_ids.contains(&node.entity_id) {
            return false;
        }
        // Groups that are children of another group should not be at root level
        if node.kind == LayerNodeKind::Group && child_group_ids.contains(&node.entity_id) {
            return false;
        }
        if node.kind == LayerNodeKind::Waveform && child_take_ids.contains(&node.entity_id) {
            return false;
        }
        match node.kind {
            LayerNodeKind::Instrument => instruments.contains_key(&node.entity_id),
            LayerNodeKind::Waveform => waveforms.contains_key(&node.entity_id),
            LayerNodeKind::MidiClip => midi_clips.contains_key(&node.entity_id),
            LayerNodeKind::TextNote => true,
            LayerNodeKind::Group => groups.contains_key(&node.entity_id),
        }
    });

    // Phase 2: sync children and track seen IDs (recursive for groups)
    for node in tree.iter_mut() {
        seen_ids.insert(node.entity_id);
        sync_node_children(node, instruments, midi_clips, waveforms, groups, &mut seen_ids);
    }

    // Phase 3: add new root-level entities not yet in the tree
    for &id in instruments.keys() {
        if !seen_ids.contains(&id) && !grouped_ids.contains(&id) {
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
    for &id in waveforms.keys() {
        if !seen_ids.contains(&id) && !grouped_ids.contains(&id) && !child_take_ids.contains(&id) {
            let children = if let Some(tg) = waveforms.get(&id).and_then(|wf| wf.take_group.as_ref()) {
                tg.take_ids.iter().filter_map(|&cid| {
                    if waveforms.contains_key(&cid) {
                        seen_ids.insert(cid);
                        Some(LayerNode { entity_id: cid, kind: LayerNodeKind::Waveform, expanded: false, children: Vec::new() })
                    } else { None }
                }).collect()
            } else {
                Vec::new()
            };
            let has_takes = !children.is_empty();
            tree.push(LayerNode { entity_id: id, kind: LayerNodeKind::Waveform, expanded: has_takes, children });
            seen_ids.insert(id);
        }
    }
    // Only add root-level groups (not child groups)
    for (&id, _) in groups.iter() {
        if !seen_ids.contains(&id) && !child_group_ids.contains(&id) {
            let mut visited = std::collections::HashSet::new();
            let node = build_group_subtree(id, groups, instruments, midi_clips, waveforms, &mut visited);
            // Mark all nodes in subtree as seen
            collect_ids_recursive(&node, &mut seen_ids);
            tree.push(node);
        }
    }
}

/// Collect all entity IDs in a node subtree.
fn collect_ids_recursive(node: &LayerNode, seen: &mut std::collections::HashSet<EntityId>) {
    seen.insert(node.entity_id);
    for child in &node.children {
        collect_ids_recursive(child, seen);
    }
}

/// Recursively sync children for a single node (used by sync_tree).
fn sync_node_children(
    node: &mut LayerNode,
    instruments: &IndexMap<EntityId, crate::instruments::Instrument>,
    midi_clips: &IndexMap<EntityId, crate::midi::MidiClip>,
    waveforms: &IndexMap<EntityId, crate::ui::waveform::WaveformView>,
    groups: &IndexMap<EntityId, crate::group::Group>,
    seen_ids: &mut std::collections::HashSet<EntityId>,
) {
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
    } else if node.kind == LayerNodeKind::Waveform {
        if let Some(wf) = waveforms.get(&node.entity_id) {
            if let Some(tg) = &wf.take_group {
                let take_set: std::collections::HashSet<EntityId> = tg.take_ids.iter().copied().collect();
                node.children.retain(|c| take_set.contains(&c.entity_id));
                let existing: std::collections::HashSet<EntityId> = node.children.iter().map(|c| c.entity_id).collect();
                for &cid in &tg.take_ids {
                    if !existing.contains(&cid) && waveforms.contains_key(&cid) {
                        node.children.push(LayerNode {
                            entity_id: cid, kind: LayerNodeKind::Waveform, expanded: false, children: Vec::new(),
                        });
                    }
                }
                if !node.children.is_empty() && !node.expanded {
                    node.expanded = true;
                }
                for c in &node.children { seen_ids.insert(c.entity_id); }
            } else {
                node.children.clear();
            }
        }
    } else if node.kind == LayerNodeKind::Group {
        if let Some(group) = groups.get(&node.entity_id) {
            let member_set: std::collections::HashSet<EntityId> = group.member_ids.iter().copied().collect();
            // Retain only valid children
            node.children.retain(|c| {
                let k = member_kind(c.entity_id, instruments, midi_clips, waveforms, groups);
                if k.is_none() { return false; }
                if !member_set.contains(&c.entity_id) { return false; }
                if k == Some(LayerNodeKind::MidiClip) {
                    if let Some(mc) = midi_clips.get(&c.entity_id) {
                        if let Some(inst_id) = mc.instrument_id {
                            if member_set.contains(&inst_id) { return false; }
                        }
                    }
                }
                true
            });
            // Add missing members
            let existing: std::collections::HashSet<EntityId> = node.children.iter().map(|c| c.entity_id).collect();
            for mid in &group.member_ids {
                if !existing.contains(mid) {
                    if groups.contains_key(mid) {
                        // Nested group — build subtree
                        let mut visited = std::collections::HashSet::new();
                        let child_node = build_group_subtree(*mid, groups, instruments, midi_clips, waveforms, &mut visited);
                        node.children.push(child_node);
                    } else if let Some(k) = member_kind(*mid, instruments, midi_clips, waveforms, groups) {
                        if k == LayerNodeKind::MidiClip {
                            if let Some(mc) = midi_clips.get(mid) {
                                if let Some(inst_id) = mc.instrument_id {
                                    if member_set.contains(&inst_id) { continue; }
                                }
                            }
                        }
                        node.children.push(LayerNode { entity_id: *mid, kind: k, expanded: false, children: Vec::new() });
                    }
                }
            }
            // Recursively sync children (including nested group children)
            for child in &mut node.children {
                seen_ids.insert(child.entity_id);
                sync_node_children(child, instruments, midi_clips, waveforms, groups, seen_ids);
            }
        }
    }
}

/// Move a node up by one position within its parent. Returns true if moved.
pub fn move_node_up(tree: &mut Vec<LayerNode>, entity_id: EntityId) -> bool {
    if let Some(idx) = tree.iter().position(|n| n.entity_id == entity_id) {
        if idx > 0 {
            tree.swap(idx, idx - 1);
            return true;
        }
    }
    for node in tree.iter_mut() {
        if let Some(idx) = node.children.iter().position(|c| c.entity_id == entity_id) {
            if idx > 0 {
                node.children.swap(idx, idx - 1);
                return true;
            }
            return false;
        }
        if move_node_up(&mut node.children, entity_id) {
            return true;
        }
    }
    false
}

/// Move a node down by one position within its parent. Returns true if moved.
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
            return false;
        }
        if move_node_down(&mut node.children, entity_id) {
            return true;
        }
    }
    false
}

/// Toggle expanded state for a node at any depth.
pub fn toggle_expanded(tree: &mut [LayerNode], entity_id: EntityId) {
    for node in tree.iter_mut() {
        if node.entity_id == entity_id {
            node.expanded = !node.expanded;
            return;
        }
        toggle_expanded(&mut node.children, entity_id);
    }
}

// ---------------------------------------------------------------------------
// Drag-to-reorder in layers panel
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DropTarget {
    /// Insert before the root node at this tree index.
    BeforeRoot(usize),
    /// Append after all root nodes.
    AfterLastRoot,
    /// Insert inside a group at a child position.
    InsideGroup { group_id: EntityId, child_index: usize },
}

/// Given the flat row list, the index the mouse is over, and the Y fraction
/// within that row (0.0 = top, 1.0 = bottom), compute where a drop should land.
/// Returns `None` when dropping onto self or an invalid position.
pub fn compute_drop_target(
    flat_rows: &[FlatLayerRow],
    tree: &[LayerNode],
    mouse_flat_index: usize,
    y_fraction: f32,
    dragged_id: EntityId,
) -> Option<DropTarget> {
    if mouse_flat_index >= flat_rows.len() {
        return Some(DropTarget::AfterLastRoot);
    }

    let row = &flat_rows[mouse_flat_index];

    // Cannot drop onto self
    if row.entity_id == dragged_id {
        return None;
    }

    // Middle 40% of a Group row → drop inside group
    if row.kind == LayerNodeKind::Group && y_fraction >= 0.3 && y_fraction <= 0.7 {
        // Don't allow dropping a group into itself
        if row.entity_id == dragged_id {
            return None;
        }
        let child_count = find_node(tree, row.entity_id)
            .map(|n| n.children.len())
            .unwrap_or(0);
        return Some(DropTarget::InsideGroup {
            group_id: row.entity_id,
            child_index: child_count,
        });
    }

    // If this row is a child of a group (depth > 0), compute insertion within that group
    if row.depth > 0 {
        // Find the parent group
        if let Some(parent_group_id) = find_parent_group(tree, row.entity_id) {
            if let Some(group_node) = find_node(tree, parent_group_id) {
                if let Some(child_idx) = group_node.children.iter().position(|c| c.entity_id == row.entity_id) {
                    if y_fraction < 0.5 {
                        return Some(DropTarget::InsideGroup {
                            group_id: parent_group_id,
                            child_index: child_idx,
                        });
                    } else {
                        return Some(DropTarget::InsideGroup {
                            group_id: parent_group_id,
                            child_index: child_idx + 1,
                        });
                    }
                }
            }
        }
    }

    // Root-level row: top half = before, bottom half = after
    if let Some(root_idx) = tree.iter().position(|n| n.entity_id == row.entity_id) {
        if y_fraction < 0.5 {
            Some(DropTarget::BeforeRoot(root_idx))
        } else {
            if root_idx + 1 >= tree.len() {
                Some(DropTarget::AfterLastRoot)
            } else {
                Some(DropTarget::BeforeRoot(root_idx + 1))
            }
        }
    } else {
        None
    }
}

/// Find a node by entity_id at any depth (read-only).
fn find_node(tree: &[LayerNode], entity_id: EntityId) -> Option<&LayerNode> {
    for node in tree {
        if node.entity_id == entity_id { return Some(node); }
        if let Some(found) = find_node(&node.children, entity_id) {
            return Some(found);
        }
    }
    None
}

/// Find a node by entity_id at any depth (mutable).
fn find_node_mut(tree: &mut [LayerNode], entity_id: EntityId) -> Option<&mut LayerNode> {
    for node in tree.iter_mut() {
        if node.entity_id == entity_id { return Some(node); }
        if let Some(found) = find_node_mut(&mut node.children, entity_id) {
            return Some(found);
        }
    }
    None
}

/// Find which group (if any) contains the given entity as a direct child (recursive).
pub fn find_parent_group(tree: &[LayerNode], entity_id: EntityId) -> Option<EntityId> {
    for node in tree {
        if node.children.iter().any(|c| c.entity_id == entity_id) {
            return Some(node.entity_id);
        }
        if let Some(found) = find_parent_group(&node.children, entity_id) {
            return Some(found);
        }
    }
    None
}

/// Remove a node from the tree at any depth and return it.
fn remove_node(tree: &mut Vec<LayerNode>, entity_id: EntityId) -> Option<LayerNode> {
    if let Some(idx) = tree.iter().position(|n| n.entity_id == entity_id) {
        return Some(tree.remove(idx));
    }
    for node in tree.iter_mut() {
        if let Some(found) = remove_node(&mut node.children, entity_id) {
            return Some(found);
        }
    }
    None
}

/// Execute a drop: move a node to a new position in the tree. Returns true if moved.
pub fn execute_drop(tree: &mut Vec<LayerNode>, target: &DropTarget, dragged_id: EntityId) -> bool {
    let node = match remove_node(tree, dragged_id) {
        Some(n) => n,
        None => return false,
    };

    match target {
        DropTarget::BeforeRoot(idx) => {
            // After removal, the index may have shifted down by 1 if it was before idx
            let insert_idx = (*idx).min(tree.len());
            tree.insert(insert_idx, node);
        }
        DropTarget::AfterLastRoot => {
            tree.push(node);
        }
        DropTarget::InsideGroup { group_id, child_index } => {
            if let Some(group_node) = find_node_mut(tree, *group_id) {
                let insert_idx = (*child_index).min(group_node.children.len());
                group_node.children.insert(insert_idx, node);
            } else {
                // Group not found, put it back at root
                tree.push(node);
                return false;
            }
        }
    }
    true
}

/// Compute the visual indicator position for a drop target.
/// Returns (flat_row_index_for_y, depth, is_inside_group).
pub fn drop_target_indicator(
    target: &DropTarget,
    tree: &[LayerNode],
    flat_rows: &[FlatLayerRow],
) -> Option<(usize, usize, bool)> {
    match target {
        DropTarget::InsideGroup { group_id, .. } => {
            // Highlight the group row itself
            flat_rows.iter().position(|r| r.entity_id == *group_id)
                .map(|idx| (idx, 0, true))
        }
        DropTarget::BeforeRoot(root_idx) => {
            if let Some(node) = tree.get(*root_idx) {
                flat_rows.iter().position(|r| r.entity_id == node.entity_id)
                    .map(|idx| (idx, 0, false))
            } else {
                Some((flat_rows.len(), 0, false))
            }
        }
        DropTarget::AfterLastRoot => {
            Some((flat_rows.len(), 0, false))
        }
    }
}

// ---------------------------------------------------------------------------
// Storage conversion
// ---------------------------------------------------------------------------

pub fn tree_to_stored(tree: &[LayerNode]) -> Vec<crate::storage::StoredLayerNode> {
    let mut out = Vec::new();
    fn store_recursive(node: &LayerNode, parent_id: &str, out: &mut Vec<crate::storage::StoredLayerNode>) {
        out.push(crate::storage::StoredLayerNode {
            entity_id: node.entity_id.to_string(),
            kind_tag: node.kind.tag().to_string(),
            parent_entity_id: parent_id.to_string(),
            expanded: node.expanded,
        });
        let pid = node.entity_id.to_string();
        for child in &node.children {
            store_recursive(child, &pid, out);
        }
    }
    for node in tree {
        store_recursive(node, "", &mut out);
    }
    out
}

pub fn tree_from_stored(stored: &[crate::storage::StoredLayerNode]) -> Vec<LayerNode> {
    use crate::entity_id;

    let mut nodes_map: std::collections::HashMap<EntityId, LayerNode> = std::collections::HashMap::new();
    let mut children_map: std::collections::HashMap<EntityId, Vec<EntityId>> = std::collections::HashMap::new();
    let mut root_ids: Vec<EntityId> = Vec::new();

    for s in stored {
        let eid = match s.entity_id.parse::<EntityId>() {
            Ok(id) => id,
            Err(_) => continue,
        };
        let kind = match LayerNodeKind::from_tag(&s.kind_tag) {
            Some(k) => k,
            None => continue,
        };
        let node = LayerNode { entity_id: eid, kind, expanded: s.expanded, children: Vec::new() };
        nodes_map.insert(eid, node);

        if s.parent_entity_id.is_empty() {
            root_ids.push(eid);
        } else {
            let parent_id = s.parent_entity_id.parse::<EntityId>()
                .unwrap_or_else(|_| entity_id::new_id());
            children_map.entry(parent_id).or_default().push(eid);
        }
    }

    // Recursively attach children
    fn attach_children(
        node_id: EntityId,
        nodes_map: &mut std::collections::HashMap<EntityId, LayerNode>,
        children_map: &std::collections::HashMap<EntityId, Vec<EntityId>>,
    ) -> Option<LayerNode> {
        let mut node = nodes_map.remove(&node_id)?;
        if let Some(child_ids) = children_map.get(&node_id) {
            for &cid in child_ids {
                if let Some(child) = attach_children(cid, nodes_map, children_map) {
                    node.children.push(child);
                }
            }
        }
        Some(node)
    }

    let mut roots = Vec::new();
    for rid in root_ids {
        if let Some(root) = attach_children(rid, &mut nodes_map, &children_map) {
            roots.push(root);
        }
    }
    roots
}
