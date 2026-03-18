use crate::entity_id::EntityId;
use crate::ui::hit_testing::point_in_rect;
use crate::Camera;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct ExportRegion {
    pub(crate) position: [f32; 2],
    pub(crate) size: [f32; 2],
}

impl ExportRegion {
    pub fn hit_test_border(&self, world_pos: [f32; 2], camera: &Camera) -> bool {
        let border_thickness = 6.0 / camera.zoom;
        let p = self.position;
        let s = self.size;
        if !point_in_rect(
            world_pos,
            [p[0] - border_thickness, p[1] - border_thickness],
            [s[0] + border_thickness * 2.0, s[1] + border_thickness * 2.0],
        ) {
            return false;
        }
        if point_in_rect(world_pos, [p[0], p[1] - border_thickness], [s[0], border_thickness * 2.0]) {
            return true;
        }
        if point_in_rect(world_pos, [p[0], p[1] + s[1] - border_thickness], [s[0], border_thickness * 2.0]) {
            return true;
        }
        if point_in_rect(world_pos, [p[0] - border_thickness, p[1]], [border_thickness * 2.0, s[1]]) {
            return true;
        }
        if point_in_rect(world_pos, [p[0] + s[0] - border_thickness, p[1]], [border_thickness * 2.0, s[1]]) {
            return true;
        }
        let pill_w = EXPORT_RENDER_PILL_W / camera.zoom;
        let pill_h = EXPORT_RENDER_PILL_H / camera.zoom;
        if point_in_rect(
            world_pos,
            [p[0] + 4.0 / camera.zoom, p[1] + 4.0 / camera.zoom],
            [pill_w, pill_h],
        ) {
            return true;
        }
        false
    }
}

pub(crate) struct SelectArea {
    pub(crate) position: [f32; 2],
    pub(crate) size: [f32; 2],
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ExportHover {
    None,
    RenderPill(EntityId),
    CornerNW(EntityId),
    CornerNE(EntityId),
    CornerSW(EntityId),
    CornerSE(EntityId),
}

pub(crate) const EXPORT_REGION_DEFAULT_WIDTH: f32 = 800.0;
pub(crate) const EXPORT_REGION_DEFAULT_HEIGHT: f32 = 300.0;
pub(crate) use crate::theme::{EXPORT_FILL_COLOR, EXPORT_BORDER_COLOR, EXPORT_RENDER_PILL_COLOR};
pub(crate) const EXPORT_RENDER_PILL_W: f32 = 110.0;
pub(crate) const EXPORT_RENDER_PILL_H: f32 = 22.0;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct LoopRegion {
    pub(crate) position: [f32; 2],
    pub(crate) size: [f32; 2],
    pub(crate) enabled: bool,
}

impl LoopRegion {
    pub fn hit_test_border(&self, world_pos: [f32; 2], camera: &Camera) -> bool {
        let border_thickness = 6.0 / camera.zoom;
        let p = self.position;
        let s = self.size;
        if !point_in_rect(
            world_pos,
            [p[0] - border_thickness, p[1] - border_thickness],
            [s[0] + border_thickness * 2.0, s[1] + border_thickness * 2.0],
        ) {
            return false;
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
        // LOOP badge area
        let badge_w = LOOP_BADGE_W / camera.zoom;
        let badge_h = LOOP_BADGE_H / camera.zoom;
        if point_in_rect(
            world_pos,
            [p[0] + 4.0 / camera.zoom, p[1] + 4.0 / camera.zoom],
            [badge_w, badge_h],
        ) {
            return true;
        }
        false
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum LoopHover {
    None,
    CornerNW(EntityId),
    CornerNE(EntityId),
    CornerSW(EntityId),
    CornerSE(EntityId),
}

pub(crate) const LOOP_REGION_DEFAULT_WIDTH: f32 = 800.0;
pub(crate) const LOOP_REGION_DEFAULT_HEIGHT: f32 = 250.0;
pub(crate) use crate::theme::{LOOP_FILL_COLOR, LOOP_BORDER_COLOR, LOOP_BADGE_COLOR};
pub(crate) const LOOP_BADGE_W: f32 = 70.0;
pub(crate) const LOOP_BADGE_H: f32 = 22.0;
