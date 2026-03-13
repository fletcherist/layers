use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rack::plugin_info::{PluginInfo, PluginType};
use rack::traits::{PluginInstance, PluginScanner};
use rack::vst3::Vst3Scanner;
use serde::{Deserialize, Serialize};

use crate::{point_in_rect, push_border, Camera, InstanceRaw};

#[derive(Serialize, Deserialize)]
struct CachedPluginInfo {
    name: String,
    manufacturer: String,
    version: u32,
    plugin_type: String,
    path: String,
    unique_id: String,
}

#[derive(Serialize, Deserialize)]
struct PluginCache {
    version: u32,
    plugins: Vec<CachedPluginInfo>,
}

fn plugin_type_to_str(pt: &PluginType) -> &'static str {
    match pt {
        PluginType::Effect => "Effect",
        PluginType::Instrument => "Instrument",
        PluginType::Mixer => "Mixer",
        PluginType::FormatConverter => "FormatConverter",
        PluginType::Analyzer => "Analyzer",
        PluginType::Spatial => "Spatial",
        PluginType::Other => "Other",
    }
}

fn str_to_plugin_type(s: &str) -> PluginType {
    match s {
        "Effect" => PluginType::Effect,
        "Instrument" => PluginType::Instrument,
        "Mixer" => PluginType::Mixer,
        "FormatConverter" => PluginType::FormatConverter,
        "Analyzer" => PluginType::Analyzer,
        "Spatial" => PluginType::Spatial,
        _ => PluginType::Other,
    }
}

fn cache_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".layers")
        .join("vst_plugin_cache.json")
}

fn load_cache() -> Option<Vec<PluginInfo>> {
    let data = std::fs::read_to_string(cache_path()).ok()?;
    let cache: PluginCache = serde_json::from_str(&data).ok()?;
    if cache.version != 1 {
        return None;
    }
    Some(
        cache
            .plugins
            .into_iter()
            .map(|c| PluginInfo::new(
                c.name,
                c.manufacturer,
                c.version,
                str_to_plugin_type(&c.plugin_type),
                PathBuf::from(c.path),
                c.unique_id,
            ))
            .collect(),
    )
}

fn save_cache(plugins: &[PluginInfo]) {
    let cache = PluginCache {
        version: 1,
        plugins: plugins
            .iter()
            .map(|p| CachedPluginInfo {
                name: p.name.clone(),
                manufacturer: p.manufacturer.clone(),
                version: p.version,
                plugin_type: plugin_type_to_str(&p.plugin_type).to_string(),
                path: p.path.to_string_lossy().to_string(),
                unique_id: p.unique_id.clone(),
            })
            .collect(),
    };
    if let Some(parent) = cache_path().parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(&cache) {
        Ok(json) => {
            if let Err(e) = std::fs::write(cache_path(), json) {
                println!("  VST3 cache write error: {}", e);
            }
        }
        Err(e) => println!("  VST3 cache serialize error: {}", e),
    }
}

pub const EFFECT_REGION_DEFAULT_WIDTH: f32 = 600.0;
pub const EFFECT_REGION_DEFAULT_HEIGHT: f32 = 250.0;
const EFFECT_BORDER_COLOR: [f32; 4] = [0.60, 0.30, 0.90, 0.50];
const EFFECT_ACTIVE_BORDER: [f32; 4] = [0.70, 0.40, 1.00, 0.70];

#[derive(Clone)]
pub struct EffectRegion {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub chain: Vec<PluginSlot>,
    pub name: String,
}

impl EffectRegion {
    pub fn new(position: [f32; 2], size: [f32; 2]) -> Self {
        Self {
            position,
            size,
            chain: Vec::new(),
            name: "effects".to_string(),
        }
    }

    pub fn hit_test_border(&self, world_pos: [f32; 2], camera: &Camera) -> bool {
        let border_thickness = 6.0 / camera.zoom;
        let name_area_h = 20.0 / camera.zoom;
        let p = self.position;
        let s = self.size;
        if !point_in_rect(world_pos, [p[0] - border_thickness, p[1] - border_thickness - name_area_h],
            [s[0] + border_thickness * 2.0, s[1] + border_thickness * 2.0 + name_area_h]) {
            return false;
        }
        // Name label area above the region
        if point_in_rect(world_pos, [p[0], p[1] - name_area_h], [s[0], name_area_h]) {
            return true;
        }
        // Top edge
        if point_in_rect(world_pos, [p[0], p[1] - border_thickness], [s[0], border_thickness * 2.0]) {
            return true;
        }
        // Bottom edge
        if point_in_rect(world_pos, [p[0], p[1] + s[1] - border_thickness], [s[0], border_thickness * 2.0]) {
            return true;
        }
        // Left edge
        if point_in_rect(world_pos, [p[0] - border_thickness, p[1]], [border_thickness * 2.0, s[1]]) {
            return true;
        }
        // Right edge
        if point_in_rect(world_pos, [p[0] + s[0] - border_thickness, p[1]], [border_thickness * 2.0, s[1]]) {
            return true;
        }
        // FX badge area
        let badge_w = 28.0 / camera.zoom;
        let badge_h = 16.0 / camera.zoom;
        if point_in_rect(world_pos,
            [p[0] + 4.0 / camera.zoom, p[1] + 4.0 / camera.zoom],
            [badge_w + 100.0 / camera.zoom, badge_h]) {
            return true;
        }
        false
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct PluginSlot {
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_path: PathBuf,
    pub bypass: bool,
    pub instance: Arc<Mutex<Option<Box<dyn PluginInstance>>>>,
    pub gui: Arc<Mutex<Option<vst3_gui::Vst3Gui>>>,
}

pub struct PluginRegistryEntry {
    pub info: PluginInfo,
}

pub struct PluginRegistry {
    pub plugins: Vec<PluginRegistryEntry>,
    scanner: Option<Vst3Scanner>,
    scanned: bool,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
            scanner: None,
            scanned: false,
        }
    }

    pub fn is_scanned(&self) -> bool {
        self.scanned
    }

    pub fn ensure_scanned(&mut self) {
        if !self.scanned {
            self.scan();
        }
    }

    pub fn scan(&mut self) {
        self.scanned = true;

        if let Some(cached) = load_cache() {
            let effects: Vec<_> = cached
                .into_iter()
                .filter(|p| matches!(p.plugin_type, PluginType::Effect | PluginType::Other))
                .collect();
            println!("  VST3 (cached): found {} effect plugins", effects.len());
            for p in &effects {
                println!("    - {} ({})", p.name, p.manufacturer);
            }
            self.plugins = effects
                .into_iter()
                .map(|info| PluginRegistryEntry { info })
                .collect();
            if let Ok(scanner) = Vst3Scanner::new() {
                self.scanner = Some(scanner);
            }
            return;
        }

        match Vst3Scanner::new() {
            Ok(scanner) => {
                match scanner.scan() {
                    Ok(plugins) => {
                        save_cache(&plugins);
                        let effects: Vec<_> = plugins
                            .into_iter()
                            .filter(|p| {
                                matches!(p.plugin_type, PluginType::Effect | PluginType::Other)
                            })
                            .collect();
                        println!("  VST3: found {} effect plugins", effects.len());
                        for p in &effects {
                            println!("    - {} ({})", p.name, p.manufacturer);
                        }
                        self.plugins = effects
                            .into_iter()
                            .map(|info| PluginRegistryEntry { info })
                            .collect();
                    }
                    Err(e) => {
                        println!("  VST3 scan error: {}", e);
                    }
                }
                self.scanner = Some(scanner);
            }
            Err(e) => {
                println!("  VST3 scanner init error: {}", e);
            }
        }
    }

    pub fn rescan(&mut self) {
        let _ = std::fs::remove_file(cache_path());
        self.scanned = false;
        self.plugins.clear();
        self.scanner = None;
        self.scan();
    }

    pub fn load_plugin(
        &self,
        plugin_id: &str,
        sample_rate: f64,
        block_size: usize,
    ) -> Option<Box<dyn PluginInstance>> {
        let scanner = self.scanner.as_ref()?;
        let entry = self
            .plugins
            .iter()
            .find(|e| e.info.unique_id == plugin_id)?;
        match scanner.load(&entry.info) {
            Ok(mut plugin) => {
                if let Err(e) = plugin.initialize(sample_rate, block_size) {
                    println!("  VST3 plugin init error: {}", e);
                    return None;
                }
                println!("  VST3 plugin loaded: {}", entry.info.name);
                Some(Box::new(plugin))
            }
            Err(e) => {
                println!("  VST3 plugin load error: {}", e);
                None
            }
        }
    }
}

pub const PLUGIN_LABEL_COLOR: [f32; 4] = [0.55, 0.28, 0.85, 0.55];
pub const PLUGIN_LABEL_W: f32 = 80.0;
pub const PLUGIN_LABEL_H: f32 = 16.0;
pub const PLUGIN_LABEL_GAP: f32 = 4.0;
pub const PLUGIN_CLOSE_SIZE: f32 = 14.0;

#[allow(dead_code)]
pub struct PluginLabelRect {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub close_position: [f32; 2],
    pub close_size: [f32; 2],
    pub region_idx: usize,
    pub slot_idx: usize,
}

pub fn plugin_label_rects(region: &EffectRegion, camera: &Camera) -> Vec<PluginLabelRect> {
    let badge_h = PLUGIN_LABEL_H / camera.zoom;
    let start_x = region.position[0] + 36.0 / camera.zoom;
    let y = region.position[1] + 4.0 / camera.zoom;
    let pill_w = PLUGIN_LABEL_W / camera.zoom;
    let gap = PLUGIN_LABEL_GAP / camera.zoom;
    let close_sz = PLUGIN_CLOSE_SIZE / camera.zoom;

    region
        .chain
        .iter()
        .enumerate()
        .map(|(i, _slot)| {
            let x = start_x + i as f32 * (pill_w + gap);
            let close_x = x + pill_w - close_sz - 1.0 / camera.zoom;
            let close_y = y + (badge_h - close_sz) * 0.5;
            PluginLabelRect {
                position: [x, y],
                size: [pill_w, badge_h],
                close_position: [close_x, close_y],
                close_size: [close_sz, close_sz],
                region_idx: 0,
                slot_idx: i,
            }
        })
        .collect()
}

pub fn build_effect_region_instances(
    region: &EffectRegion,
    camera: &Camera,
    is_hovered: bool,
    is_selected: bool,
    is_active: bool,
) -> Vec<InstanceRaw> {
    let mut out = Vec::new();

    let border_color = if is_active {
        EFFECT_ACTIVE_BORDER
    } else {
        EFFECT_BORDER_COLOR
    };

    let bw = if is_selected { 2.5 } else { 1.5 } / camera.zoom;
    let mut bc = border_color;
    if is_hovered && !is_selected {
        bc[3] = (bc[3] + 0.15).min(1.0);
    }
    push_border(&mut out, region.position, region.size, bw, bc);

    // Dashed top indicator
    let dash_h = 3.0 / camera.zoom;
    let dash_w = 20.0 / camera.zoom;
    let gap = 10.0 / camera.zoom;
    let y = region.position[1] - dash_h - 2.0 / camera.zoom;
    let mut x = region.position[0];
    while x < region.position[0] + region.size[0] {
        let w = dash_w.min(region.position[0] + region.size[0] - x);
        out.push(InstanceRaw {
            position: [x, y],
            size: [w, dash_h],
            color: [0.60, 0.30, 0.90, 0.40],
            border_radius: 1.0 / camera.zoom,
        });
        x += dash_w + gap;
    }

    // "FX" badge at top-left
    let badge_w = 28.0 / camera.zoom;
    let badge_h = 16.0 / camera.zoom;
    out.push(InstanceRaw {
        position: [
            region.position[0] + 4.0 / camera.zoom,
            region.position[1] + 4.0 / camera.zoom,
        ],
        size: [badge_w, badge_h],
        color: [0.55, 0.28, 0.85, 0.70],
        border_radius: 3.0 / camera.zoom,
    });

    // Plugin name label pills (one per plugin in chain)
    let labels = plugin_label_rects(region, camera);
    for rect in &labels {
        out.push(InstanceRaw {
            position: rect.position,
            size: rect.size,
            color: PLUGIN_LABEL_COLOR,
            border_radius: rect.size[1] * 0.5,
        });
        // Close button circle on the right edge
        out.push(InstanceRaw {
            position: rect.close_position,
            size: rect.close_size,
            color: [0.45, 0.20, 0.75, 0.60],
            border_radius: rect.close_size[1] * 0.5,
        });
    }

    if is_selected {
        let handle_sz = 8.0 / camera.zoom;
        let handle_color = [0.55, 0.28, 0.85, 0.90];
        for &hx in &[region.position[0] - handle_sz * 0.5, region.position[0] + region.size[0] - handle_sz * 0.5] {
            for &hy in &[region.position[1] - handle_sz * 0.5, region.position[1] + region.size[1] - handle_sz * 0.5] {
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
