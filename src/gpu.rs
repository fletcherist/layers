use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glyphon::{
    Attrs, Buffer as TextBuffer, Color as TextColor, Family, FontSystem, Metrics, Resolution,
    Shaping, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::grid::PIXELS_PER_SECOND;
use crate::ui::browser;
use crate::ui::right_window;
use crate::effects;
use crate::instruments;
use crate::midi;
use crate::settings::Settings;
use crate::ui::settings_window::SettingsWindow;
use crate::ui::context_menu::ContextMenu;
use crate::ui::palette::CommandPalette;
use crate::ui::plugin_editor;
use crate::ui::toast;
use crate::ui::tooltip;
use crate::ui::waveform;
use crate::ui::waveform::WaveformVertex;
use crate::{
    ExportRegion, TransportPanel, EXPORT_RENDER_PILL_H,
    EXPORT_RENDER_PILL_W,
};

// ---------------------------------------------------------------------------
// Shader (WGSL)
// ---------------------------------------------------------------------------

const SHADER_SRC: &str = r#"
struct Camera {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> camera: Camera;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) local_pos: vec2<f32>,
    @location(2) rect_size: vec2<f32>,
    @location(3) border_radius: f32,
}

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) obj_pos: vec2<f32>,
    @location(2) obj_size: vec2<f32>,
    @location(3) obj_color: vec4<f32>,
    @location(4) radius: f32,
) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = obj_pos + position * obj_size;
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 0.0, 1.0);
    out.color = obj_color;
    out.local_pos = position * obj_size;
    out.rect_size = obj_size;
    out.border_radius = radius;
    return out;
}

fn rounded_box_sdf(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let r = min(in.border_radius, min(in.rect_size.x, in.rect_size.y) * 0.5);
    let center = in.rect_size * 0.5;
    let p = in.local_pos - center;
    let d = rounded_box_sdf(p, center, r);
    let fw = fwidth(d);
    if (r < 0.01) {
        return in.color;
    }
    let alpha = 1.0 - smoothstep(0.0, fw, d);
    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
"#;

const WAVEFORM_SHADER_SRC: &str = r#"
struct Camera {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> camera: Camera;

struct WfOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) edge_dist: f32,
}

@vertex
fn wf_vs(
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) edge: f32,
) -> WfOut {
    var out: WfOut;
    out.clip_position = camera.view_proj * vec4<f32>(pos, 0.0, 1.0);
    out.color = color;
    out.edge_dist = edge;
    return out;
}

@fragment
fn wf_fs(in: WfOut) -> @location(0) vec4<f32> {
    let aa = 1.0 - smoothstep(0.0, 1.0, abs(in.edge_dist));
    return vec4<f32>(in.color.rgb, in.color.a * aa);
}
"#;

// ---------------------------------------------------------------------------
// GPU data types
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
}

const QUAD_VERTICES: &[Vertex] = &[
    Vertex {
        position: [0.0, 0.0],
    },
    Vertex {
        position: [1.0, 0.0],
    },
    Vertex {
        position: [1.0, 1.0],
    },
    Vertex {
        position: [0.0, 1.0],
    },
];

const QUAD_INDICES: &[u16] = &[0, 1, 2, 0, 2, 3];

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct InstanceRaw {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub border_radius: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
}

const MAX_INSTANCES: usize = 16384;

// ---------------------------------------------------------------------------
// Camera
// ---------------------------------------------------------------------------

pub(crate) struct Camera {
    pub(crate) position: [f32; 2],
    pub(crate) zoom: f32,
}

impl Camera {
    pub(crate) fn new() -> Self {
        Self {
            position: [-100.0, -50.0],
            zoom: 1.0,
        }
    }

    pub(crate) fn view_proj(&self, width: f32, height: f32) -> [[f32; 4]; 4] {
        let z = self.zoom;
        let cx = self.position[0];
        let cy = self.position[1];
        [
            [2.0 * z / width, 0.0, 0.0, 0.0],
            [0.0, -2.0 * z / height, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [
                -2.0 * z * cx / width - 1.0,
                2.0 * z * cy / height + 1.0,
                0.0,
                1.0,
            ],
        ]
    }

    pub(crate) fn screen_to_world(&self, screen: [f32; 2]) -> [f32; 2] {
        [
            screen[0] / self.zoom + self.position[0],
            screen[1] / self.zoom + self.position[1],
        ]
    }

    pub(crate) fn world_to_screen(&self, world: [f32; 2]) -> [f32; 2] {
        [
            (world[0] - self.position[0]) * self.zoom,
            (world[1] - self.position[1]) * self.zoom,
        ]
    }

    pub(crate) fn zoom_at(&mut self, screen_pos: [f32; 2], factor: f32) {
        let world = self.screen_to_world(screen_pos);
        self.zoom = (self.zoom * factor).clamp(0.05, 200.0);
        self.position[0] = world[0] - screen_pos[0] / self.zoom;
        self.position[1] = world[1] - screen_pos[1] / self.zoom;
    }
}

pub(crate) fn screen_ortho(width: f32, height: f32) -> [[f32; 4]; 4] {
    [
        [2.0 / width, 0.0, 0.0, 0.0],
        [0.0, -2.0 / height, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [-1.0, 1.0, 0.0, 1.0],
    ]
}

pub(crate) fn push_border(
    out: &mut Vec<InstanceRaw>,
    pos: [f32; 2],
    size: [f32; 2],
    bw: f32,
    color: [f32; 4],
) {
    out.push(InstanceRaw {
        position: pos,
        size: [size[0], bw],
        color,
        border_radius: 0.0,
    });
    out.push(InstanceRaw {
        position: [pos[0], pos[1] + size[1] - bw],
        size: [size[0], bw],
        color,
        border_radius: 0.0,
    });
    out.push(InstanceRaw {
        position: pos,
        size: [bw, size[1]],
        color,
        border_radius: 0.0,
    });
    out.push(InstanceRaw {
        position: [pos[0] + size[0] - bw, pos[1]],
        size: [bw, size[1]],
        color,
        border_radius: 0.0,
    });
}

// ---------------------------------------------------------------------------
// GPU state
// ---------------------------------------------------------------------------

const MAX_WAVEFORM_VERTICES: usize = 131072;

#[derive(PartialEq, Eq)]
struct TextLabelCacheKey {
    text: String,
    max_width_q: i32,
    font_size_q: i32,
}

pub(crate) struct Gpu {
    pub(crate) window: Arc<Window>,
    pub(crate) surface: wgpu::Surface<'static>,
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) config: wgpu::SurfaceConfiguration,
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) waveform_pipeline: wgpu::RenderPipeline,
    pub(crate) waveform_vertex_buffer: wgpu::Buffer,
    pub(crate) camera_buffer: wgpu::Buffer,
    pub(crate) camera_bind_group: wgpu::BindGroup,
    pub(crate) screen_camera_buffer: wgpu::Buffer,
    pub(crate) screen_camera_bind_group: wgpu::BindGroup,
    pub(crate) vertex_buffer: wgpu::Buffer,
    pub(crate) index_buffer: wgpu::Buffer,
    pub(crate) instance_buffer: wgpu::Buffer,

    pub(crate) font_system: FontSystem,
    pub(crate) swash_cache: SwashCache,
    pub(crate) text_atlas: TextAtlas,
    pub(crate) text_renderer: TextRenderer,
    pub(crate) overlay_text_atlas: TextAtlas,
    pub(crate) overlay_text_renderer: TextRenderer,
    pub(crate) viewport: Viewport,
    pub(crate) scale_factor: f32,

    pub(crate) browser_text_buffers: Vec<TextBuffer>,
    pub(crate) browser_text_generation: u64,

    cached_wf_label_bufs: Vec<(TextLabelCacheKey, TextBuffer)>,
    cached_text_note_bufs: Vec<(TextLabelCacheKey, TextBuffer)>,
    cached_group_label_bufs: Vec<(TextLabelCacheKey, TextBuffer)>,
    cached_auto_dot_bufs: Vec<(TextLabelCacheKey, TextBuffer)>,
    cached_auto_lane_bufs: Vec<(TextLabelCacheKey, TextBuffer)>,
    cached_midi_note_label_bufs: Vec<(TextLabelCacheKey, TextBuffer)>,
    cached_midi_per_note_bufs: Vec<(TextLabelCacheKey, TextBuffer)>,
    cached_inst_label_bufs: Vec<(TextLabelCacheKey, TextBuffer)>,
    pub(crate) auto_lane_close_rects: Vec<(crate::entity_id::EntityId, [f32; 4])>,
}

pub(crate) struct TextEntry {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub line_height: f32,
    pub max_width: f32,
    pub color: [u8; 4],
    pub weight: u16,
    pub bounds: Option<[f32; 4]>, // [left, top, right, bottom] clip rect; None = full screen
    pub center: bool,
}

fn shape_text_entry(font_system: &mut FontSystem, entry: &TextEntry) -> TextBuffer {
    let mut buf = TextBuffer::new(font_system, Metrics::new(entry.font_size, entry.line_height));
    buf.set_size(font_system, Some(entry.max_width), Some(entry.line_height * 2.0));
    let attrs = Attrs::new()
        .family(Family::SansSerif)
        .weight(glyphon::Weight(entry.weight));
    buf.set_text(font_system, &entry.text, attrs, Shaping::Advanced);
    if entry.center {
        for line in buf.lines.iter_mut() {
            line.set_align(Some(glyphon::cosmic_text::Align::Center));
        }
    }
    buf.shape_until_scroll(font_system, false);
    buf
}

pub(crate) struct IconEntry {
    pub codepoint: &'static str,
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub color: [u8; 4],
}

fn shape_icon_entry(font_system: &mut FontSystem, entry: &IconEntry) -> TextBuffer {
    let mut buf = TextBuffer::new(font_system, Metrics::new(entry.size, entry.size));
    buf.set_size(font_system, Some(entry.size * 2.0), Some(entry.size * 2.0));
    let attrs = Attrs::new().family(Family::Name("Material Icons"));
    buf.set_text(font_system, entry.codepoint, attrs, Shaping::Advanced);
    buf.shape_until_scroll(font_system, false);
    buf
}

impl Gpu {
    pub(crate) async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let scale_factor = window.scale_factor() as f32;

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("No suitable GPU adapter found");

        log::info!("GPU adapter: {:?}", adapter.get_info());

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("canvas shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
        });

        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera uniform"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let screen_camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("screen camera uniform"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera bind group layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let screen_camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("screen camera bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: screen_camera_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x2,
            }],
        };

        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceRaw>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 32,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[vertex_layout, instance_layout],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // --- waveform pipeline ---
        let wf_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("waveform shader"),
            source: wgpu::ShaderSource::Wgsl(WAVEFORM_SHADER_SRC.into()),
        });

        let wf_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<WaveformVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 24,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        };

        let waveform_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("waveform pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &wf_shader,
                entry_point: Some("wf_vs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wf_vertex_layout],
            },
            fragment: Some(wgpu::FragmentState {
                module: &wf_shader,
                entry_point: Some("wf_fs"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let waveform_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("waveform vertex buffer"),
            size: (MAX_WAVEFORM_VERTICES * std::mem::size_of::<WaveformVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad vertices"),
            contents: bytemuck::cast_slice(QUAD_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad indices"),
            contents: bytemuck::cast_slice(QUAD_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instance buffer"),
            size: (MAX_INSTANCES * std::mem::size_of::<InstanceRaw>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // --- glyphon text rendering ---
        #[cfg(target_arch = "wasm32")]
        let mut font_system = {
            let font_data = include_bytes!("../assets/Inter-Regular.ttf");
            FontSystem::new_with_fonts(std::iter::once(glyphon::fontdb::Source::Binary(
                std::sync::Arc::new(font_data.as_slice()),
            )))
        };
        #[cfg(not(target_arch = "wasm32"))]
        let mut font_system = FontSystem::new();
        // Load icon fonts on all platforms
        {
            let icons_data = include_bytes!("../assets/MaterialIcons-Regular.ttf");
            font_system.db_mut().load_font_data(icons_data.to_vec());
        }
        let font_system = font_system;
        let swash_cache = SwashCache::new();
        let cache = glyphon::Cache::new(&device);
        let mut text_atlas = TextAtlas::new(&device, &queue, &cache, surface_format);
        let text_renderer = TextRenderer::new(
            &mut text_atlas,
            &device,
            wgpu::MultisampleState::default(),
            None,
        );
        let mut overlay_text_atlas = TextAtlas::new(&device, &queue, &cache, surface_format);
        let overlay_text_renderer = TextRenderer::new(
            &mut overlay_text_atlas,
            &device,
            wgpu::MultisampleState::default(),
            None,
        );
        let viewport = Viewport::new(&device, &cache);

        Self {
            window,
            surface,
            device,
            queue,
            config,
            pipeline,
            waveform_pipeline,
            waveform_vertex_buffer,
            camera_buffer,
            camera_bind_group,
            screen_camera_buffer,
            screen_camera_bind_group,
            vertex_buffer,
            index_buffer,
            instance_buffer,
            font_system,
            swash_cache,
            text_atlas,
            text_renderer,
            overlay_text_atlas,
            overlay_text_renderer,
            viewport,
            scale_factor,
            browser_text_buffers: Vec::new(),
            browser_text_generation: 0,
            cached_wf_label_bufs: Vec::new(),
            cached_text_note_bufs: Vec::new(),
            cached_group_label_bufs: Vec::new(),
            cached_auto_dot_bufs: Vec::new(),
            cached_auto_lane_bufs: Vec::new(),
            cached_midi_note_label_bufs: Vec::new(),
            cached_midi_per_note_bufs: Vec::new(),
            cached_inst_label_bufs: Vec::new(),
            auto_lane_close_rects: Vec::new(),
        }
    }

    pub(crate) fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    pub(crate) fn render(
        &mut self,
        camera: &Camera,
        world_instances: &[InstanceRaw],
        waveform_vertices: &[WaveformVertex],
        preview_waveform_vertices: &[WaveformVertex],
        command_palette: Option<&CommandPalette>,
        context_menu: Option<&ContextMenu>,
        sample_browser: Option<&browser::SampleBrowser>,
        browser_drag_ghost: Option<(&str, [f32; 2])>,
        is_playing: bool,
        is_recording: bool,
        computer_keyboard_armed: bool,
        playback_position: f64,
        export_regions: &indexmap::IndexMap<crate::entity_id::EntityId, ExportRegion>,
        loop_regions: &indexmap::IndexMap<crate::entity_id::EntityId, crate::regions::LoopRegion>,
        _editing_effect_name: Option<(crate::entity_id::EntityId, &str)>,
        waveforms: &indexmap::IndexMap<crate::entity_id::EntityId, waveform::WaveformView>,
        editing_waveform_name: Option<(crate::entity_id::EntityId, &str)>,
        plugin_editor: Option<&plugin_editor::PluginEditorWindow>,
        export_window: Option<&crate::ui::export_window::ExportWindow>,
        settings_window: Option<&SettingsWindow>,
        settings: &Settings,
        toast_manager: &toast::ToastManager,
        tooltip: &tooltip::TooltipState,
        bpm: f32,
        editing_bpm: Option<&str>,
        automation_mode: bool,
        active_automation_param: crate::automation::AutomationParam,
        midi_clips: &indexmap::IndexMap<crate::entity_id::EntityId, midi::MidiClip>,
        instruments: &indexmap::IndexMap<crate::entity_id::EntityId, instruments::Instrument>,
        hovered_midi_clip: Option<crate::entity_id::EntityId>,
        editing_midi_clip: Option<crate::entity_id::EntityId>,
        mouse_world: [f32; 2],
        cmd_velocity_hover_note: Option<(crate::entity_id::EntityId, usize)>,
        has_remote_storage: bool,
        right_window: Option<&right_window::RightWindow>,
        right_window_effect_chain: Option<(&effects::EffectChain, crate::entity_id::EntityId, usize)>,
        effect_chain_drag: Option<(crate::entity_id::EntityId, usize, f32, Option<usize>)>,
        input_monitoring: bool,
        text_notes: &indexmap::IndexMap<crate::entity_id::EntityId, crate::text_note::TextNote>,
        editing_text_note: Option<(crate::entity_id::EntityId, usize)>,
        selected_ids: &std::collections::HashSet<crate::entity_id::EntityId>,
        groups: &indexmap::IndexMap<crate::entity_id::EntityId, crate::group::Group>,
        hidden_take_children: &std::collections::HashSet<crate::entity_id::EntityId>,
    ) {
        let w = self.config.width as f32;
        let h = self.config.height as f32;
        if w < 1.0 || h < 1.0 {
            return;
        }

        let cam_uniform = CameraUniform {
            view_proj: camera.view_proj(w, h),
        };
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[cam_uniform]));

        let screen_cam = CameraUniform {
            view_proj: screen_ortho(w, h),
        };
        self.queue.write_buffer(
            &self.screen_camera_buffer,
            0,
            bytemuck::cast_slice(&[screen_cam]),
        );

        let world_count = world_instances.len().min(MAX_INSTANCES);
        self.queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&world_instances[..world_count]),
        );

        let wf_vert_count = waveform_vertices.len().min(MAX_WAVEFORM_VERTICES);
        let preview_wf_count = preview_waveform_vertices.len().min(MAX_WAVEFORM_VERTICES.saturating_sub(wf_vert_count));
        if wf_vert_count > 0 || preview_wf_count > 0 {
            if wf_vert_count > 0 {
                self.queue.write_buffer(
                    &self.waveform_vertex_buffer,
                    0,
                    bytemuck::cast_slice(&waveform_vertices[..wf_vert_count]),
                );
            }
            if preview_wf_count > 0 {
                let offset = (wf_vert_count * std::mem::size_of::<WaveformVertex>()) as u64;
                self.queue.write_buffer(
                    &self.waveform_vertex_buffer,
                    offset,
                    bytemuck::cast_slice(&preview_waveform_vertices[..preview_wf_count]),
                );
            }
        }

        // Build overlay instances: browser panel + drag ghost + command palette
        let mut overlay_instances: Vec<InstanceRaw> = Vec::new();

        if let Some(br) = sample_browser {
            overlay_instances.extend(br.build_instances(settings, w, h, self.scale_factor, selected_ids));
        }

        if let Some(rw) = right_window {
            overlay_instances.extend(rw.build_instances(settings, w, h, self.scale_factor));
            let (chain, chain_id, ref_count) = match &right_window_effect_chain {
                Some((c, id, rc)) => (Some(*c), Some(*id), *rc),
                None => (None, None, 0),
            };
            let (drag_idx, drag_offset, hover_idx) = match effect_chain_drag {
                Some((dc_id, slot_idx, offset, hover)) if Some(dc_id) == chain_id =>
                    (Some(slot_idx), offset, hover),
                _ => (None, 0.0, None),
            };
            if !rw.is_multi() {
                overlay_instances.extend(rw.build_effect_chain_instances(
                    chain, chain_id, ref_count, settings, w, h, self.scale_factor,
                    drag_idx, drag_offset, hover_idx,
                ));
            }
        }

        if let Some((_, pos)) = browser_drag_ghost {
            overlay_instances.push(InstanceRaw {
                position: [pos[0] - 4.0, pos[1] - 4.0],
                size: [160.0 * self.scale_factor, 24.0 * self.scale_factor],
                color: settings.theme.tooltip_bg,
                border_radius: 4.0 * self.scale_factor,
            });
        }

        overlay_instances.extend(TransportPanel::build_instances(
            settings,
            w,
            h,
            self.scale_factor,
            is_playing,
            is_recording,
            settings.metronome_enabled,
            computer_keyboard_armed,
            input_monitoring,
        ));

        if let Some(p) = command_palette {
            overlay_instances.extend(p.build_instances(settings, w, h, self.scale_factor));
        }

        // Context menu is rendered in a separate top layer (after text) — see top_instances below

        if let Some(sw) = settings_window {
            overlay_instances.extend(sw.build_instances(settings, w, h, self.scale_factor));
        }

        if let Some(pe) = plugin_editor {
            overlay_instances.extend(pe.build_instances(settings, w, h, self.scale_factor));
        }

        if let Some(ew) = export_window {
            overlay_instances.extend(ew.build_instances(settings, w, h, self.scale_factor));
        }

        overlay_instances.extend(toast_manager.build_instances(w, h, self.scale_factor));
        overlay_instances.extend(tooltip.build_instances(self.scale_factor, &settings.theme));

        // Velocity tooltip background pill
        if let Some((mc_idx, note_idx)) = cmd_velocity_hover_note {
            if let Some(mc) = midi_clips.get(&mc_idx) {
                if note_idx < mc.notes.len() {
                let s = self.scale_factor;
                let vel_text = format!("{}", mc.notes[note_idx].velocity);
                let vel_font = 11.0 * s;
                let vel_line = 14.0 * s;
                let text_w = vel_font * vel_text.len() as f32 * 0.6;
                let pad_x = 5.0 * s;
                let pad_y = 3.0 * s;
                let pill_w = text_w + vel_font + pad_x * 2.0;
                let pill_h = vel_line + pad_y * 2.0;
                let mouse_sx = (mouse_world[0] - camera.position[0]) * camera.zoom;
                let mouse_sy = (mouse_world[1] - camera.position[1]) * camera.zoom;
                let pill_x = mouse_sx + 12.0 * s - pad_x;
                let pill_y = mouse_sy - vel_line - 4.0 * s - pad_y;
                overlay_instances.push(InstanceRaw {
                    position: [pill_x, pill_y],
                    size: [pill_w, pill_h],
                    color: settings.theme.tooltip_bg,
                    border_radius: 4.0 * s,
                });
            }
            }
        }

        // Top overlay: context menu renders above all other overlays and text
        let mut top_instances: Vec<InstanceRaw> = Vec::new();
        if let Some(cm) = context_menu {
            top_instances.extend(cm.build_instances(settings, w, h, self.scale_factor));
        }

        // --- prepare text ---
        let scale = self.scale_factor;
        let mut text_buffers: Vec<TextBuffer> = Vec::new();
        let mut text_meta: Vec<(f32, f32, TextColor, TextBounds)> = Vec::new();

        let full_bounds = TextBounds {
            left: 0,
            top: 0,
            right: w as i32,
            bottom: h as i32,
        };

        // Browser text: shape ALL entries once, positions computed each frame
        if let Some(br) = sample_browser {
            if br.visible && br.text_generation != self.browser_text_generation {
                self.browser_text_buffers.clear();
                for te in &br.cached_text {
                    let buf = shape_text_entry(&mut self.font_system, te);
                    self.browser_text_buffers.push(buf);
                }
                self.browser_text_generation = br.text_generation;
            } else if !br.visible && !self.browser_text_buffers.is_empty() {
                self.browser_text_buffers.clear();
                self.browser_text_generation = 0;
            }
        } else if !self.browser_text_buffers.is_empty() {
            self.browser_text_buffers.clear();
            self.browser_text_generation = 0;
        }

        // Drag ghost text
        if let Some((label, pos)) = browser_drag_ghost {
            let font_sz = 12.0 * scale;
            let line_h = 16.0 * scale;
            let mut buf = TextBuffer::new(&mut self.font_system, Metrics::new(font_sz, line_h));
            buf.set_size(&mut self.font_system, Some(150.0 * scale), Some(line_h));
            buf.set_text(
                &mut self.font_system,
                label,
                Attrs::new().family(Family::SansSerif),
                Shaping::Advanced,
            );
            buf.shape_until_scroll(&mut self.font_system, false);
            text_buffers.push(buf);
            let tp = settings.theme.text_primary;
            text_meta.push((
                pos[0] + 4.0 * scale,
                pos[1] - 4.0 + (24.0 * scale - line_h) * 0.5,
                TextColor::rgba((tp[0] * 255.0) as u8, (tp[1] * 255.0) as u8, (tp[2] * 255.0) as u8, 255),
                full_bounds,
            ));
        }

        // Right window text
        if let Some(rw) = right_window {
            for te in rw.get_text_entries(&settings.theme, w, h, scale) {
                let bounds = match te.bounds {
                    Some([l, t, r, b]) => TextBounds {
                        left: l as i32, top: t as i32, right: r as i32, bottom: b as i32,
                    },
                    None => full_bounds,
                };
                let buf = shape_text_entry(&mut self.font_system, &te);
                text_buffers.push(buf);
                text_meta.push((
                    te.x,
                    te.y,
                    TextColor::rgba(te.color[0], te.color[1], te.color[2], te.color[3]),
                    bounds,
                ));
            }
            // Effect chain text entries
            let (chain, chain_id, ref_count) = match &right_window_effect_chain {
                Some((c, id, rc)) => (Some(*c), Some(*id), *rc),
                None => (None, None, 0),
            };
            let (text_drag_idx, text_drag_offset) = match effect_chain_drag {
                Some((dc_id, slot_idx, offset, _)) if Some(dc_id) == chain_id =>
                    (Some(slot_idx), offset),
                _ => (None, 0.0),
            };
            let effect_text_entries = if rw.is_multi() { Vec::new() } else { rw.get_effect_chain_text_entries(&settings.theme, chain, chain_id, ref_count, w, h, scale, text_drag_idx, text_drag_offset) };
            for te in effect_text_entries {
                let bounds = match te.bounds {
                    Some([l, t, r, b]) => TextBounds {
                        left: l as i32, top: t as i32, right: r as i32, bottom: b as i32,
                    },
                    None => full_bounds,
                };
                let buf = shape_text_entry(&mut self.font_system, &te);
                text_buffers.push(buf);
                text_meta.push((
                    te.x,
                    te.y,
                    TextColor::rgba(te.color[0], te.color[1], te.color[2], te.color[3]),
                    bounds,
                ));
            }
            // Effect chain icon entries (delete icons)
            let chain_for_icons = match &right_window_effect_chain {
                Some((c, _, _)) => Some(*c),
                None => None,
            };
            let (icon_drag_idx, icon_drag_offset) = match effect_chain_drag {
                Some((dc_id, slot_idx, offset, _)) if Some(dc_id) == chain_id =>
                    (Some(slot_idx), offset),
                _ => (None, 0.0),
            };
            for ie in rw.get_effect_chain_icon_entries(chain_for_icons, w, h, scale, icon_drag_idx, icon_drag_offset, settings) {
                let buf = shape_icon_entry(&mut self.font_system, &ie);
                text_buffers.push(buf);
                text_meta.push((
                    ie.x,
                    ie.y,
                    TextColor::rgba(ie.color[0], ie.color[1], ie.color[2], ie.color[3]),
                    full_bounds,
                ));
            }
        }

        // Browser toggle icon (≡ when visible, › when collapsed) and search clear icon
        if let Some(br) = sample_browser {
            let ie = br.get_toggle_icon_entry(&settings.theme, scale, h);
            let buf = shape_icon_entry(&mut self.font_system, &ie);
            text_buffers.push(buf);
            text_meta.push((
                ie.x,
                ie.y,
                TextColor::rgba(ie.color[0], ie.color[1], ie.color[2], ie.color[3]),
                full_bounds,
            ));
            if let Some(ie) = br.get_search_clear_icon_entry(&settings.theme, scale) {
                let buf = shape_icon_entry(&mut self.font_system, &ie);
                text_buffers.push(buf);
                text_meta.push((
                    ie.x,
                    ie.y,
                    TextColor::rgba(ie.color[0], ie.color[1], ie.color[2], ie.color[3]),
                    full_bounds,
                ));
            }
            for ie in br.get_places_icon_entries(&settings.theme, scale) {
                let buf = shape_icon_entry(&mut self.font_system, &ie);
                text_buffers.push(buf);
                text_meta.push((
                    ie.x,
                    ie.y,
                    TextColor::rgba(ie.color[0], ie.color[1], ie.color[2], ie.color[3]),
                    full_bounds,
                ));
            }
        }

        // Transport panel text and icons
        for te in TransportPanel::get_text_entries(&settings.theme, w, h, scale, playback_position, bpm, editing_bpm) {
            let buf = shape_text_entry(&mut self.font_system, &te);
            text_buffers.push(buf);
            text_meta.push((
                te.x,
                te.y,
                TextColor::rgba(te.color[0], te.color[1], te.color[2], te.color[3]),
                full_bounds,
            ));
        }
        for ie in TransportPanel::get_icon_entries(
            settings, w, h, scale,
            is_playing, is_recording,
            settings.metronome_enabled, computer_keyboard_armed, input_monitoring,
        ) {
            let buf = shape_icon_entry(&mut self.font_system, &ie);
            text_buffers.push(buf);
            text_meta.push((
                ie.x,
                ie.y,
                TextColor::rgba(ie.color[0], ie.color[1], ie.color[2], ie.color[3]),
                full_bounds,
            ));
        }

        if let Some(palette) = command_palette {
            for te in palette.get_text_entries(&settings.theme, w, h, scale) {
                let buf = shape_text_entry(&mut self.font_system, &te);
                text_buffers.push(buf);
                text_meta.push((
                    te.x,
                    te.y,
                    TextColor::rgba(te.color[0], te.color[1], te.color[2], te.color[3]),
                    full_bounds,
                ));
            }
        }

        // Context menu text is rendered in the top overlay layer — see top_text_buffers below

        // Plugin editor text
        if let Some(pe) = plugin_editor {
            for te in pe.get_text_entries(settings, w, h, scale) {
                let buf = shape_text_entry(&mut self.font_system, &te);
                text_buffers.push(buf);
                text_meta.push((
                    te.x,
                    te.y,
                    TextColor::rgba(te.color[0], te.color[1], te.color[2], te.color[3]),
                    full_bounds,
                ));
            }
        }

        // Export window text
        if let Some(ew) = export_window {
            let popup_rect = ew.open_dropdown_popup_rect(w, h, scale);
            for te in ew.get_text_entries(settings, w, h, scale) {
                let is_popup_entry = te.bounds.is_some();
                let mut bounds = full_bounds;
                if !is_popup_entry {
                    if let Some((pp, ps)) = popup_rect {
                        let overlaps_v = te.y + te.line_height > pp[1]
                            && te.y < pp[1] + ps[1];
                        if overlaps_v {
                            if te.x >= pp[0] && te.x < pp[0] + ps[0] {
                                continue;
                            }
                            if te.x < pp[0] && te.x + te.max_width > pp[0] {
                                bounds.right = bounds.right.min(pp[0] as i32);
                            }
                        }
                    }
                }
                let buf = shape_text_entry(&mut self.font_system, &te);
                text_buffers.push(buf);
                text_meta.push((
                    te.x,
                    te.y,
                    TextColor::rgba(te.color[0], te.color[1], te.color[2], te.color[3]),
                    bounds,
                ));
            }
        }

        // Settings window text
        if let Some(sw) = settings_window {
            let popup_rect = sw.open_dropdown_popup_rect(w, h, scale);
            for te in sw.get_text_entries(settings, w, h, scale) {
                let is_popup_entry = te.bounds.is_some();
                // Clip non-popup text against the open dropdown popup
                let mut bounds = full_bounds;
                if !is_popup_entry {
                    if let Some((pp, ps)) = popup_rect {
                        let overlaps_v = te.y + te.line_height > pp[1]
                            && te.y < pp[1] + ps[1];
                        if overlaps_v {
                            // Text starts inside popup horizontally → clip right edge
                            if te.x >= pp[0] && te.x < pp[0] + ps[0] {
                                continue; // fully inside popup column
                            }
                            // Text extends into popup from the left → clip right to popup left
                            if te.x < pp[0] && te.x + te.max_width > pp[0] {
                                bounds.right = bounds.right.min(pp[0] as i32);
                            }
                        }
                    }
                }
                let buf = shape_text_entry(&mut self.font_system, &te);
                text_buffers.push(buf);
                text_meta.push((
                    te.x,
                    te.y,
                    TextColor::rgba(te.color[0], te.color[1], te.color[2], te.color[3]),
                    bounds,
                ));
            }
        }

        // Compute context menu rect for text overlap checks
        let ctx_menu_rect: Option<([f32; 2], [f32; 2])> =
            context_menu.map(|cm| cm.menu_rect(w, h, scale));

        // Export region "Render" label with duration (world-space -> screen-space)
        for (_er_id, er) in export_regions {
            if settings_window.is_none() && command_palette.is_none() && export_window.is_none() {
                let pill_world_x = er.position[0] + 4.0 / camera.zoom;
                let pill_world_y = er.position[1] + 4.0 / camera.zoom;
                let pill_screen_x = (pill_world_x - camera.position[0]) * camera.zoom;
                let pill_screen_y = (pill_world_y - camera.position[1]) * camera.zoom;
                let pill_w_screen = EXPORT_RENDER_PILL_W;
                let pill_h_screen = EXPORT_RENDER_PILL_H;

                let overlaps_ctx = if let Some((cm_pos, cm_size)) = ctx_menu_rect {
                    pill_screen_x + pill_w_screen > cm_pos[0]
                        && pill_screen_x < cm_pos[0] + cm_size[0]
                        && pill_screen_y + pill_h_screen > cm_pos[1]
                        && pill_screen_y < cm_pos[1] + cm_size[1]
                } else {
                    false
                };

                if !overlaps_ctx {
                    let duration_secs = er.size[0] as f64 / PIXELS_PER_SECOND as f64;
                    let label_text = if duration_secs < 60.0 {
                        format!("Render  {:.1}s", duration_secs)
                    } else {
                        let mins = (duration_secs / 60.0) as u32;
                        let secs = duration_secs % 60.0;
                        format!("Render  {}:{:04.1}", mins, secs)
                    };

                    let label_font = 11.0 * scale;
                    let label_line = 16.0 * scale;
                    let mut buf = TextBuffer::new(
                        &mut self.font_system,
                        Metrics::new(label_font, label_line),
                    );
                    buf.set_size(
                        &mut self.font_system,
                        Some(pill_w_screen),
                        Some(pill_h_screen),
                    );
                    buf.set_text(
                        &mut self.font_system,
                        &label_text,
                        Attrs::new().family(Family::SansSerif),
                        Shaping::Advanced,
                    );
                    buf.shape_until_scroll(&mut self.font_system, false);
                    text_buffers.push(buf);
                    text_meta.push((
                        pill_screen_x + 8.0,
                        pill_screen_y + (pill_h_screen - label_line) * 0.5,
                        TextColor::rgb(255, 255, 255),
                        full_bounds,
                    ));
                }
            }
        }

        // Loop region labels (audio-sample style, world-space -> screen-space)
        if settings_window.is_none() && command_palette.is_none() && export_window.is_none() {
            for (idx, (_lr_id, lr)) in loop_regions.iter().enumerate() {
                if !lr.enabled {
                    continue;
                }
                let pad = 6.0 / camera.zoom;
                let label_world_x = lr.position[0] + pad;
                let label_world_y = camera.position[1] + pad;
                let label_screen_x = (label_world_x - camera.position[0]) * camera.zoom;
                let label_screen_y = (label_world_y - camera.position[1]) * camera.zoom;

                let clip_screen_w = lr.size[0] * camera.zoom;
                if clip_screen_w < 30.0 {
                    continue;
                }
                let name_font = 10.0 * scale;
                let name_line = 14.0 * scale;
                let max_text_w = (clip_screen_w - 12.0 * scale).max(20.0);

                let overlaps_ctx = if let Some((cm_pos, cm_size)) = ctx_menu_rect {
                    label_screen_x + max_text_w > cm_pos[0]
                        && label_screen_x < cm_pos[0] + cm_size[0]
                        && label_screen_y + name_line > cm_pos[1]
                        && label_screen_y < cm_pos[1] + cm_size[1]
                } else {
                    false
                };

                if !overlaps_ctx {
                    let label_text = if idx == 0 {
                        "Loop".to_string()
                    } else {
                        format!("Loop {}", idx + 1)
                    };

                    let mut buf = TextBuffer::new(
                        &mut self.font_system,
                        Metrics::new(name_font, name_line),
                    );
                    buf.set_size(&mut self.font_system, Some(max_text_w), Some(name_line));
                    buf.set_text(
                        &mut self.font_system,
                        &label_text,
                        Attrs::new()
                            .family(Family::Name(".AppleSystemUIFont"))
                            .weight(glyphon::Weight(500)),
                        Shaping::Advanced,
                    );
                    buf.shape_until_scroll(&mut self.font_system, false);
                    text_buffers.push(buf);
                    text_meta.push((
                        label_screen_x,
                        label_screen_y,
                        TextColor::rgba(255, 255, 255, 180),
                        full_bounds,
                    ));
                }
            }
        }

        let world_left = camera.position[0];
        let world_right = world_left + w / camera.zoom;
        let world_top = camera.position[1];
        let world_bottom = world_top + h / camera.zoom;

        // Waveform sample name labels (cached shaping, positions recomputed each frame)
        let browser_right = sample_browser.map_or(0.0, |b| b.panel_width(scale));
        let right_panel_left = right_window.map_or(w, |_| w - right_window::RIGHT_WINDOW_WIDTH * scale);
        let wf_label_bounds = TextBounds {
            left: browser_right as i32,
            top: 0,
            right: right_panel_left as i32,
            bottom: h as i32,
        };
        let mut old_wf_cache = std::mem::take(&mut self.cached_wf_label_bufs);
        let mut new_wf_cache: Vec<(TextLabelCacheKey, TextBuffer)> = Vec::new();
        let mut wf_label_meta: Vec<(f32, f32, TextColor, TextBounds)> = Vec::new();
        if settings_window.is_none() && command_palette.is_none() && export_window.is_none()
        { for (wf_idx, wf) in waveforms.iter() {
            if hidden_take_children.contains(wf_idx) { continue; }
            let wf_right = wf.position[0] + wf.size[0];
            let wf_bottom = wf.position[1] + wf.size[1];
            if wf_right < world_left
                || wf.position[0] > world_right
                || wf_bottom < world_top
                || wf.position[1] > world_bottom
            {
                continue;
            }
            let clip_screen_w = wf.size[0] * camera.zoom;
            if clip_screen_w < 30.0 {
                continue;
            }

            let pad = 6.0 / camera.zoom;
            let name_x_world = wf.position[0] + pad;
            let name_y_world = wf.position[1] + pad;
            let name_screen_x = (name_x_world - camera.position[0]) * camera.zoom;
            let name_screen_y = (name_y_world - camera.position[1]) * camera.zoom;

            if settings_window.is_some() || command_palette.is_some() {
                continue;
            }
            if let Some((cm_pos, cm_size)) = ctx_menu_rect {
                let max_text_w = (clip_screen_w - 12.0 * scale).max(20.0);
                let name_line = 14.0 * scale;
                if name_screen_x + max_text_w > cm_pos[0]
                    && name_screen_x < cm_pos[0] + cm_size[0]
                    && name_screen_y + name_line > cm_pos[1]
                    && name_screen_y < cm_pos[1] + cm_size[1]
                {
                    continue;
                }
            }

            let display_name = if let Some((idx, ref text)) = editing_waveform_name {
                if idx == *wf_idx {
                    format!("{}|", text)
                } else if !wf.audio.filename.is_empty() {
                    wf.audio.filename.clone()
                } else {
                    wf.filename.clone()
                }
            } else {
                // Child takes get "Take 1", "Take 2", etc. Parent keeps its filename.
                let take_label: Option<String> = waveforms.iter().find_map(|(_, pw)| {
                    pw.take_group.as_ref().and_then(|tg| {
                        tg.take_ids.iter().position(|id| id == wf_idx)
                            .map(|pos| format!("Take {}", pos + 1))
                    })
                });
                if let Some(label) = take_label {
                    label
                } else {
                    let base = if !wf.audio.filename.is_empty() {
                        wf.audio.filename.clone()
                    } else {
                        wf.filename.clone()
                    };
                    if wf.disabled && has_remote_storage {
                        format!("{} (uploading...)", base)
                    } else {
                        base
                    }
                }
            };

            let name_font = 10.0 * scale;
            let name_line = 14.0 * scale;
            let max_text_w = (clip_screen_w - 12.0 * scale).max(20.0);

            let key = TextLabelCacheKey {
                text: display_name.clone(),
                max_width_q: (max_text_w * 2.0) as i32,
                font_size_q: (name_font * 2.0) as i32,
            };
            if let Some(pos) = old_wf_cache.iter().position(|(k, _)| *k == key) {
                new_wf_cache.push(old_wf_cache.swap_remove(pos));
            } else {
                let mut buf =
                    TextBuffer::new(&mut self.font_system, Metrics::new(name_font, name_line));
                buf.set_size(&mut self.font_system, Some(max_text_w), Some(name_line));
                let attrs = Attrs::new()
                    .family(Family::Name(".AppleSystemUIFont"))
                    .weight(glyphon::Weight(500));
                buf.set_text(
                    &mut self.font_system,
                    &display_name,
                    attrs,
                    Shaping::Advanced,
                );
                buf.shape_until_scroll(&mut self.font_system, false);
                new_wf_cache.push((key, buf));
            }

            let is_editing = editing_waveform_name.map_or(false, |(idx, _)| idx == *wf_idx);
            let alpha = if is_editing { 255 } else { 180 };
            wf_label_meta.push((
                name_screen_x,
                name_screen_y,
                TextColor::rgba(255, 255, 255, alpha),
                wf_label_bounds,
            ));
        }
        }
        self.cached_wf_label_bufs = new_wf_cache;

        // Instrument name labels on MIDI clips (same style as waveform filename labels)
        let mut old_inst_cache = std::mem::take(&mut self.cached_inst_label_bufs);
        let mut new_inst_cache: Vec<(TextLabelCacheKey, TextBuffer)> = Vec::new();
        let mut inst_label_meta: Vec<(f32, f32, TextColor, TextBounds)> = Vec::new();
        if settings_window.is_none() && command_palette.is_none() && export_window.is_none()
        { for (_mc_idx, mc) in midi_clips.iter() {
            if mc.disabled { continue; }
            let mc_right = mc.position[0] + mc.size[0];
            let mc_bottom = mc.position[1] + mc.size[1];
            if mc_right < world_left
                || mc.position[0] > world_right
                || mc_bottom < world_top
                || mc.position[1] > world_bottom
            {
                continue;
            }
            let clip_screen_w = mc.size[0] * camera.zoom;
            if clip_screen_w < 30.0 {
                continue;
            }

            let display_name = mc.instrument_id
                .and_then(|iid| instruments.get(&iid))
                .and_then(|inst| {
                    if !inst.name.is_empty() && inst.name != "instrument" {
                        Some(inst.name.clone())
                    } else if !inst.plugin_name.is_empty() {
                        Some(inst.plugin_name.clone())
                    } else {
                        None
                    }
                });
            let display_name = match display_name {
                Some(n) => n,
                None => continue,
            };

            let pad = 6.0 / camera.zoom;
            let name_x_world = mc.position[0] + pad;
            let name_y_world = mc.position[1] + pad;
            let name_screen_x = (name_x_world - camera.position[0]) * camera.zoom;
            let name_screen_y = (name_y_world - camera.position[1]) * camera.zoom;

            if let Some((cm_pos, cm_size)) = ctx_menu_rect {
                let max_text_w = (clip_screen_w - 12.0 * scale).max(20.0);
                let name_line = 14.0 * scale;
                if name_screen_x + max_text_w > cm_pos[0]
                    && name_screen_x < cm_pos[0] + cm_size[0]
                    && name_screen_y + name_line > cm_pos[1]
                    && name_screen_y < cm_pos[1] + cm_size[1]
                {
                    continue;
                }
            }

            let name_font = 10.0 * scale;
            let name_line = 14.0 * scale;
            let max_text_w = (clip_screen_w - 12.0 * scale).max(20.0);

            let key = TextLabelCacheKey {
                text: display_name.clone(),
                max_width_q: (max_text_w * 2.0) as i32,
                font_size_q: (name_font * 2.0) as i32,
            };
            if let Some(pos) = old_inst_cache.iter().position(|(k, _)| *k == key) {
                new_inst_cache.push(old_inst_cache.swap_remove(pos));
            } else {
                let mut buf =
                    TextBuffer::new(&mut self.font_system, Metrics::new(name_font, name_line));
                buf.set_size(&mut self.font_system, Some(max_text_w), Some(name_line));
                let attrs = Attrs::new()
                    .family(Family::Name(".AppleSystemUIFont"))
                    .weight(glyphon::Weight(500));
                buf.set_text(
                    &mut self.font_system,
                    &display_name,
                    attrs,
                    Shaping::Advanced,
                );
                buf.shape_until_scroll(&mut self.font_system, false);
                new_inst_cache.push((key, buf));
            }

            inst_label_meta.push((
                name_screen_x,
                name_screen_y,
                TextColor::rgba(255, 255, 255, 180),
                wf_label_bounds,
            ));
        }
        }
        self.cached_inst_label_bufs = new_inst_cache;

        // Text note content labels (cached shaping, positions recomputed each frame)
        let mut old_tn_cache = std::mem::take(&mut self.cached_text_note_bufs);
        let mut new_tn_cache: Vec<(TextLabelCacheKey, TextBuffer)> = Vec::new();
        let mut tn_label_meta: Vec<(f32, f32, TextColor, TextBounds)> = Vec::new();
        if settings_window.is_none() && command_palette.is_none() && export_window.is_none() {
            for (_tn_id, tn) in text_notes.iter() {
                let tn_right = tn.position[0] + tn.size[0];
                let tn_bottom = tn.position[1] + tn.size[1];
                if tn_right < world_left
                    || tn.position[0] > world_right
                    || tn_bottom < world_top
                    || tn.position[1] > world_bottom
                {
                    continue;
                }
                let note_screen_w = tn.size[0] * camera.zoom;
                let note_screen_h = tn.size[1] * camera.zoom;
                if note_screen_w < 20.0 {
                    continue;
                }

                let pad = 8.0 / camera.zoom;
                let text_x_world = tn.position[0] + pad;
                let text_y_world = tn.position[1] + pad;
                let text_screen_x = (text_x_world - camera.position[0]) * camera.zoom;
                let text_screen_y = (text_y_world - camera.position[1]) * camera.zoom;

                let text_content = if tn.text.is_empty() && editing_text_note.map(|(id, _)| id) != Some(*_tn_id) {
                    "Text note".to_string()
                } else {
                    tn.text.clone()
                };

                let font_size = tn.font_size * scale;
                let line_height = (tn.font_size * 1.4) * scale;
                let max_text_w = (note_screen_w - 16.0 * scale).max(20.0);
                let max_text_h = (note_screen_h - 16.0 * scale).max(20.0);

                let key = TextLabelCacheKey {
                    text: text_content.clone(),
                    max_width_q: (max_text_w * 2.0) as i32,
                    font_size_q: (font_size * 2.0) as i32,
                };
                if let Some(pos) = old_tn_cache.iter().position(|(k, _)| *k == key) {
                    new_tn_cache.push(old_tn_cache.swap_remove(pos));
                } else {
                    let mut buf =
                        TextBuffer::new(&mut self.font_system, Metrics::new(font_size, line_height));
                    buf.set_size(&mut self.font_system, Some(max_text_w), Some(max_text_h));
                    let attrs = Attrs::new()
                        .family(Family::Name(".AppleSystemUIFont"))
                        .weight(glyphon::Weight(400));
                    buf.set_text(
                        &mut self.font_system,
                        &text_content,
                        attrs,
                        Shaping::Advanced,
                    );
                    buf.shape_until_scroll(&mut self.font_system, false);
                    new_tn_cache.push((key, buf));
                }

                // Text note cursor using Glyphon layout
                if let Some((edit_id, cursor_idx)) = editing_text_note {
                    if *_tn_id == edit_id {
                        let buf = &new_tn_cache.last().unwrap().1;
                        let sf = self.scale_factor;
                        let cursor_w = 1.5 * sf;
                        let tc = tn.text_color;

                        // Convert full-text cursor byte index to (line_index, offset_in_line)
                        let text = &tn.text;
                        let mut target_line = 0usize;
                        let mut line_start = 0usize;
                        for (i, ch) in text.char_indices() {
                            if i >= cursor_idx { break; }
                            if ch == '\n' {
                                target_line += 1;
                                line_start = i + 1;
                            }
                        }
                        let col_byte = cursor_idx - line_start;

                        // Walk layout runs to find cursor pixel position
                        let mut cursor_x = text_screen_x;
                        let mut cursor_y = text_screen_y;
                        let mut found = false;
                        let mut matched_line = false;
                        let mut last_run_bottom = text_screen_y;
                        for run in buf.layout_runs() {
                            let run_bottom = text_screen_y + run.line_top + run.line_height;
                            if run_bottom > last_run_bottom {
                                last_run_bottom = run_bottom;
                            }
                            if run.line_i != target_line { continue; }
                            matched_line = true;
                            cursor_y = text_screen_y + run.line_top;
                            for glyph in run.glyphs {
                                if col_byte >= glyph.start && col_byte < glyph.end {
                                    cursor_x = text_screen_x + glyph.x;
                                    found = true;
                                    break;
                                }
                            }
                            if found { break; }
                            // Cursor past this run's glyphs — update x to end of run
                            if let Some(last) = run.glyphs.last() {
                                if col_byte >= last.end {
                                    cursor_x = text_screen_x + last.x + last.w;
                                }
                            }
                            // Don't break — text wrapping creates multiple runs
                            // with the same line_i
                        }
                        // No run for target line (empty line after Enter)
                        if !matched_line {
                            cursor_y = last_run_bottom;
                            cursor_x = text_screen_x;
                        }

                        overlay_instances.push(InstanceRaw {
                            position: [cursor_x, cursor_y],
                            size: [cursor_w, line_height],
                            color: [tc[0], tc[1], tc[2], 0.9],
                            border_radius: 0.0,
                        });
                    }
                }

                let tc = tn.text_color;
                let alpha = if tn.text.is_empty() && editing_text_note.map(|(id, _)| id) != Some(*_tn_id) { 100 } else { 230 };
                let text_color = TextColor::rgba(
                    (tc[0] * 255.0) as u8,
                    (tc[1] * 255.0) as u8,
                    (tc[2] * 255.0) as u8,
                    alpha,
                );

                // Clip to note bounds
                let clip_left = ((tn.position[0] - camera.position[0]) * camera.zoom) as i32;
                let clip_top = ((tn.position[1] - camera.position[1]) * camera.zoom) as i32;
                let clip_right = clip_left + note_screen_w as i32;
                let clip_bottom = clip_top + note_screen_h as i32;
                let bounds = TextBounds {
                    left: clip_left.max(browser_right as i32),
                    top: clip_top.max(0),
                    right: clip_right.min(right_panel_left as i32),
                    bottom: clip_bottom.min(h as i32),
                };

                tn_label_meta.push((text_screen_x, text_screen_y, text_color, bounds));
            }
        }
        self.cached_text_note_bufs = new_tn_cache;

        // Write overlay instances to GPU (after text note cursor is added)
        let overlay_count = overlay_instances.len().min(MAX_INSTANCES - world_count);
        if overlay_count > 0 {
            let offset = (world_count * std::mem::size_of::<InstanceRaw>()) as u64;
            self.queue.write_buffer(
                &self.instance_buffer,
                offset,
                bytemuck::cast_slice(&overlay_instances[..overlay_count]),
            );
        }

        // Write top overlay instances (context menu) after regular overlays
        let top_count = top_instances.len().min(MAX_INSTANCES - world_count - overlay_count);
        if top_count > 0 {
            let top_offset = ((world_count + overlay_count) * std::mem::size_of::<InstanceRaw>()) as u64;
            self.queue.write_buffer(
                &self.instance_buffer,
                top_offset,
                bytemuck::cast_slice(&top_instances[..top_count]),
            );
        }

        let pb_label_meta: Vec<(f32, f32, TextColor, TextBounds)> = Vec::new();

        // Group name labels (cached shaping, positions recomputed each frame)
        let mut old_grp_cache = std::mem::take(&mut self.cached_group_label_bufs);
        let mut new_grp_cache: Vec<(TextLabelCacheKey, TextBuffer)> = Vec::new();
        let mut grp_label_meta: Vec<(f32, f32, TextColor, TextBounds)> = Vec::new();
        if settings_window.is_none() && command_palette.is_none() && export_window.is_none() {
            for (_grp_id, grp) in groups.iter() {
                let grp_right = grp.position[0] + grp.size[0];
                let grp_bottom = grp.position[1] + grp.size[1];
                if grp_right < world_left
                    || grp.position[0] > world_right
                    || grp_bottom < world_top
                    || grp.position[1] > world_bottom
                {
                    continue;
                }
                let clip_screen_w = grp.size[0] * camera.zoom;
                if clip_screen_w < 30.0 {
                    continue;
                }

                let pad = 6.0 / camera.zoom;
                let label_h = 14.0 / camera.zoom;
                let name_x_world = grp.position[0] + pad;
                let name_y_world = grp.position[1] - label_h - 12.0 / camera.zoom;
                let name_screen_x = (name_x_world - camera.position[0]) * camera.zoom;
                let name_screen_y = (name_y_world - camera.position[1]) * camera.zoom;

                let name_font = 10.0 * scale;
                let name_line = 14.0 * scale;
                let max_text_w = (clip_screen_w - 12.0 * scale).max(20.0);

                let key = TextLabelCacheKey {
                    text: grp.name.clone(),
                    max_width_q: (max_text_w * 2.0) as i32,
                    font_size_q: (name_font * 2.0) as i32,
                };
                if let Some(pos) = old_grp_cache.iter().position(|(k, _)| *k == key) {
                    new_grp_cache.push(old_grp_cache.swap_remove(pos));
                } else {
                    let mut buf =
                        TextBuffer::new(&mut self.font_system, Metrics::new(name_font, name_line));
                    buf.set_size(&mut self.font_system, Some(max_text_w), Some(name_line));
                    let attrs = Attrs::new()
                        .family(Family::Name(".AppleSystemUIFont"))
                        .weight(glyphon::Weight(500));
                    buf.set_text(
                        &mut self.font_system,
                        &grp.name,
                        attrs,
                        Shaping::Advanced,
                    );
                    buf.shape_until_scroll(&mut self.font_system, false);
                    new_grp_cache.push((key, buf));
                }

                grp_label_meta.push((
                    name_screen_x,
                    name_screen_y,
                    TextColor::rgba(255, 255, 255, 200),
                    wf_label_bounds,
                ));
            }
        }
        self.cached_group_label_bufs = new_grp_cache;

        // Automation dot gain labels
        let mut old_auto_cache = std::mem::take(&mut self.cached_auto_dot_bufs);
        let mut new_auto_cache: Vec<(TextLabelCacheKey, TextBuffer)> = Vec::new();
        let mut auto_label_meta: Vec<(f32, f32, TextColor, TextBounds)> = Vec::new();
        if automation_mode && settings_window.is_none() && command_palette.is_none() && export_window.is_none() {
            for (_wf_id, wf) in waveforms.iter() {
                if hidden_take_children.contains(_wf_id) { continue; }
                let wf_right = wf.position[0] + wf.size[0];
                let wf_bottom = wf.position[1] + wf.size[1];
                if wf_right < world_left
                    || wf.position[0] > world_right
                    || wf_bottom < world_top
                    || wf.position[1] > world_bottom
                {
                    continue;
                }
                let lane = wf.automation.lane_for(active_automation_param);
                let y_top = wf.position[1];
                let y_bot = wf.position[1] + wf.size[1];
                for p in &lane.points {
                    let x = wf.position[0] + p.t * wf.size[0];
                    let y = y_bot + (y_top - y_bot) * p.value;
                    let screen_x = (x - camera.position[0]) * camera.zoom;
                    let screen_y = (y - camera.position[1]) * camera.zoom;

                    let label_text = match active_automation_param {
                        crate::automation::AutomationParam::Volume => {
                            let db = crate::ui::palette::gain_to_db(crate::automation::volume_value_to_gain(p.value));
                            if db <= -60.0 {
                                "-inf dB".to_string()
                            } else {
                                format!("{:.1} dB", db)
                            }
                        }
                        crate::automation::AutomationParam::Pan => {
                            let pct = ((p.value - 0.5) * 200.0) as i32;
                            if pct == 0 {
                                "C".to_string()
                            } else if pct < 0 {
                                format!("L{}", -pct)
                            } else {
                                format!("R{}", pct)
                            }
                        }
                    };

                    let label_font = 9.0 * scale;
                    let label_line = 12.0 * scale;
                    let max_w = 80.0 * scale;

                    let key = TextLabelCacheKey {
                        text: label_text.clone(),
                        max_width_q: (max_w * 2.0) as i32,
                        font_size_q: (label_font * 2.0) as i32,
                    };
                    if let Some(pos) = old_auto_cache.iter().position(|(k, _)| *k == key) {
                        new_auto_cache.push(old_auto_cache.swap_remove(pos));
                    } else {
                        let mut buf = TextBuffer::new(
                            &mut self.font_system,
                            Metrics::new(label_font, label_line),
                        );
                        buf.set_size(&mut self.font_system, Some(max_w), Some(label_line));
                        let attrs = Attrs::new()
                            .family(Family::Name(".AppleSystemUIFont"))
                            .weight(glyphon::Weight(500));
                        buf.set_text(&mut self.font_system, &label_text, attrs, Shaping::Advanced);
                        buf.shape_until_scroll(&mut self.font_system, false);
                        new_auto_cache.push((key, buf));
                    }

                    let dot_screen_sz = (8.0 + camera.zoom * 2.0).min(40.0);
                    auto_label_meta.push((
                        screen_x + dot_screen_sz * 0.5 + 4.0,
                        screen_y - label_line / scale * 0.5,
                        TextColor::rgba(255, 255, 255, 220),
                        full_bounds,
                    ));
                }
            }
        }
        self.cached_auto_dot_bufs = new_auto_cache;

        // Automation lane label (e.g. "Volume" / "Pan") in top-left of each waveform
        let mut old_lane_cache = std::mem::take(&mut self.cached_auto_lane_bufs);
        let mut new_lane_cache: Vec<(TextLabelCacheKey, TextBuffer)> = Vec::new();
        let mut lane_label_meta: Vec<(f32, f32, TextColor, TextBounds)> = Vec::new();
        self.auto_lane_close_rects.clear();
        if automation_mode && settings_window.is_none() && command_palette.is_none() && export_window.is_none() {
            for (wf_id, wf) in waveforms.iter() {
                if hidden_take_children.contains(wf_id) { continue; }
                let wf_right = wf.position[0] + wf.size[0];
                let wf_bottom = wf.position[1] + wf.size[1];
                if wf_right < world_left
                    || wf.position[0] > world_right
                    || wf_bottom < world_top
                    || wf.position[1] > world_bottom
                {
                    continue;
                }
                let clip_screen_w = wf.size[0] * camera.zoom;
                if clip_screen_w < 30.0 {
                    continue;
                }

                let lane_text = match active_automation_param {
                    crate::automation::AutomationParam::Volume => "Volume  \u{00d7}",
                    crate::automation::AutomationParam::Pan => "Pan  \u{00d7}",
                };
                let (lr, lg, lb) = match active_automation_param {
                    crate::automation::AutomationParam::Volume => (255u8, 179u8, 51u8),
                    crate::automation::AutomationParam::Pan => (77u8, 153u8, 255u8),
                };

                let pad = 6.0 / camera.zoom;
                let name_x_world = wf.position[0] + pad;
                let name_y_world = wf.position[1] + pad + 14.0 / camera.zoom;
                let screen_x = (name_x_world - camera.position[0]) * camera.zoom;
                let screen_y = (name_y_world - camera.position[1]) * camera.zoom;

                let label_font = 9.0 * scale;
                let label_line = 12.0 * scale;
                let max_w = (clip_screen_w - 12.0 * scale).max(20.0);

                let key = TextLabelCacheKey {
                    text: lane_text.to_string(),
                    max_width_q: (max_w * 2.0) as i32,
                    font_size_q: (label_font * 2.0) as i32,
                };
                if let Some(pos) = old_lane_cache.iter().position(|(k, _)| *k == key) {
                    new_lane_cache.push(old_lane_cache.swap_remove(pos));
                } else {
                    let mut buf = TextBuffer::new(
                        &mut self.font_system,
                        Metrics::new(label_font, label_line),
                    );
                    buf.set_size(&mut self.font_system, Some(max_w), Some(label_line));
                    let attrs = Attrs::new()
                        .family(Family::Name(".AppleSystemUIFont"))
                        .weight(glyphon::Weight(500));
                    buf.set_text(&mut self.font_system, lane_text, attrs, Shaping::Advanced);
                    buf.shape_until_scroll(&mut self.font_system, false);
                    new_lane_cache.push((key, buf));
                }

                // Compute × close button hit rect
                let base_text_len = match active_automation_param {
                    crate::automation::AutomationParam::Volume => 6, // "Volume"
                    crate::automation::AutomationParam::Pan => 3,    // "Pan"
                };
                let x_icon_offset = (base_text_len as f32 + 2.0) * label_font * 0.55;
                let close_size = 12.0 * scale;
                self.auto_lane_close_rects.push((
                    *wf_id,
                    [screen_x + x_icon_offset, screen_y - close_size * 0.8, close_size, close_size],
                ));

                lane_label_meta.push((
                    screen_x,
                    screen_y,
                    TextColor::rgba(lr, lg, lb, 200),
                    full_bounds,
                ));
            }
        }
        self.cached_auto_lane_bufs = new_lane_cache;

        // MIDI clip note labels (C notes + hovered pitch)
        let mut old_midi_cache = std::mem::take(&mut self.cached_midi_note_label_bufs);
        let mut new_midi_cache: Vec<(TextLabelCacheKey, TextBuffer)> = Vec::new();
        let mut midi_label_meta: Vec<(f32, f32, TextColor, TextBounds)> = Vec::new();
        if settings_window.is_none() && command_palette.is_none() && export_window.is_none() {
            let browser_right_px = sample_browser.map_or(0.0, |b| b.panel_width(scale));

            for (mc_idx, mc) in midi_clips.iter() {
                let mc_right = mc.position[0] + mc.size[0];
                let mc_bottom = mc.position[1] + mc.size[1];
                if mc_right < world_left
                    || mc.position[0] > world_right
                    || mc_bottom < world_top
                    || mc.position[1] > world_bottom
                {
                    continue;
                }

                let is_editing = editing_midi_clip == Some(*mc_idx);
                let nh = mc.note_height_editing(is_editing);
                let row_screen_h = nh * camera.zoom;
                if row_screen_h < 3.0 {
                    continue;
                }

                let label_font = (row_screen_h * 0.7).clamp(6.0, 14.0) * scale;
                let label_line = (label_font * 1.35).min(row_screen_h * scale);
                let max_label_w = (label_font * 4.0).max(24.0 * scale);

                let clip_screen_left = (mc.position[0] - camera.position[0]) * camera.zoom;
                let clip_screen_right = (mc_right - camera.position[0]) * camera.zoom;
                let clip_screen_top = (mc.position[1] - camera.position[1]) * camera.zoom;
                let clip_screen_bottom = (mc_bottom - camera.position[1]) * camera.zoom;

                let bounds_left = clip_screen_left.max(browser_right_px).max(0.0);
                let clip_bounds = TextBounds {
                    left: bounds_left as i32,
                    top: clip_screen_top.max(0.0) as i32,
                    right: (clip_screen_right.min(w)) as i32,
                    bottom: (clip_screen_bottom.min(h)) as i32,
                };

                if clip_bounds.left >= clip_bounds.right || clip_bounds.top >= clip_bounds.bottom {
                    continue;
                }

                let sticky_x_world = mc.position[0].max(camera.position[0]);
                let browser_right_world =
                    camera.position[0] + browser_right_px / camera.zoom;
                let sticky_x_world = sticky_x_world.max(browser_right_world);
                let pad = 3.0 / camera.zoom;
                let label_x_world = sticky_x_world + pad;
                let label_screen_x = (label_x_world - camera.position[0]) * camera.zoom;

                let hovered_pitch = if hovered_midi_clip == Some(*mc_idx) && mc.contains(mouse_world)
                {
                    Some(mc.y_to_pitch_editing(mouse_world[1], is_editing))
                } else {
                    None
                };

                for pitch in mc.pitch_range.0..mc.pitch_range.1 {
                    let is_c = pitch % 12 == 0;
                    let is_hovered = hovered_pitch == Some(pitch);
                    if !is_c && !is_hovered {
                        continue;
                    }

                    let y_world = mc.pitch_to_y_editing(pitch, is_editing) + (nh - label_line / camera.zoom) * 0.5;
                    let screen_y = (y_world - camera.position[1]) * camera.zoom;

                    if screen_y + label_line < clip_screen_top || screen_y > clip_screen_bottom {
                        continue;
                    }

                    let name = midi::note_name(pitch);
                    let key = TextLabelCacheKey {
                        text: name.clone(),
                        max_width_q: (max_label_w * 2.0) as i32,
                        font_size_q: (label_font * 2.0) as i32,
                    };
                    if let Some(pos) = old_midi_cache.iter().position(|(k, _)| *k == key) {
                        new_midi_cache.push(old_midi_cache.swap_remove(pos));
                    } else {
                        let mut buf = TextBuffer::new(
                            &mut self.font_system,
                            Metrics::new(label_font, label_line),
                        );
                        buf.set_size(&mut self.font_system, Some(max_label_w), Some(label_line));
                        let attrs = Attrs::new()
                            .family(Family::Name(".AppleSystemUIFont"))
                            .weight(glyphon::Weight(500));
                        buf.set_text(&mut self.font_system, &name, attrs, Shaping::Advanced);
                        buf.shape_until_scroll(&mut self.font_system, false);
                        new_midi_cache.push((key, buf));
                    }

                    let alpha = if is_hovered { 220 } else { 130 };
                    midi_label_meta.push((
                        label_screen_x,
                        screen_y,
                        TextColor::rgba(255, 255, 255, alpha),
                        clip_bounds,
                    ));
                }
            }
        }
        self.cached_midi_note_label_bufs = new_midi_cache;

        // Per-note name labels (all visible clips; zoom thresholds hide when zoomed out)
        let mut old_pn_cache = std::mem::take(&mut self.cached_midi_per_note_bufs);
        let mut new_pn_cache: Vec<(TextLabelCacheKey, TextBuffer)> = Vec::new();
        let mut pn_label_meta: Vec<(f32, f32, TextColor, TextBounds)> = Vec::new();
        if settings_window.is_none() && command_palette.is_none() && export_window.is_none() {
            for (mc_idx, mc) in midi_clips.iter() {
                let mc_right = mc.position[0] + mc.size[0];
                let mc_bottom = mc.position[1] + mc.size[1];
                if mc_right < world_left
                    || mc.position[0] > world_right
                    || mc_bottom < world_top
                    || mc.position[1] > world_bottom
                {
                    continue;
                }

                let is_editing = editing_midi_clip == Some(*mc_idx);
                let nh = mc.note_height_editing(is_editing);
                let row_screen_h = nh * camera.zoom;
                if row_screen_h >= 6.0 {
                    let pn_font = (row_screen_h * 0.65).clamp(6.0, 12.0) * scale;
                    let pn_line = (pn_font * 1.3).min(row_screen_h * scale);
                    let min_note_screen_w = pn_font * 2.5;

                    let clip_screen_left =
                        (mc.position[0] - camera.position[0]) * camera.zoom;
                    let clip_screen_right = (mc_right - camera.position[0]) * camera.zoom;
                    let clip_screen_top =
                        (mc.position[1] - camera.position[1]) * camera.zoom;
                    let clip_screen_bottom =
                        (mc_bottom - camera.position[1]) * camera.zoom;

                    for note in &mc.notes {
                        let nx = mc.position[0] + note.start_px;
                        let nw = note.duration_px;
                        let note_screen_w = nw * camera.zoom;
                        if note_screen_w < min_note_screen_w {
                            continue;
                        }

                        let ny = mc.pitch_to_y_editing(note.pitch, is_editing);
                        let screen_x =
                            (nx - camera.position[0]) * camera.zoom + 2.0 * scale;
                        let y_world =
                            ny + (nh - pn_line / camera.zoom) * 0.5;
                        let screen_y = (y_world - camera.position[1]) * camera.zoom;

                        if screen_y + pn_line < clip_screen_top
                            || screen_y > clip_screen_bottom
                            || screen_x > clip_screen_right
                        {
                            continue;
                        }

                        let note_screen_right =
                            (nx + nw - camera.position[0]) * camera.zoom;
                        let bounds = TextBounds {
                            left: screen_x.max(clip_screen_left).max(browser_right).max(0.0) as i32,
                            top: screen_y.max(clip_screen_top).max(0.0) as i32,
                            right: note_screen_right
                                .min(clip_screen_right)
                                .min(right_panel_left)
                                .min(w) as i32,
                            bottom: (screen_y + pn_line)
                                .min(clip_screen_bottom)
                                .min(h) as i32,
                        };
                        if bounds.left >= bounds.right || bounds.top >= bounds.bottom {
                            continue;
                        }

                        let max_w = note_screen_w - 4.0 * scale;
                        let name = midi::note_name(note.pitch);
                        let key = TextLabelCacheKey {
                            text: name.clone(),
                            max_width_q: (max_w * 2.0) as i32,
                            font_size_q: (pn_font * 2.0) as i32,
                        };
                        if let Some(pos) =
                            old_pn_cache.iter().position(|(k, _)| *k == key)
                        {
                            new_pn_cache.push(old_pn_cache.swap_remove(pos));
                        } else {
                            let mut buf = TextBuffer::new(
                                &mut self.font_system,
                                Metrics::new(pn_font, pn_line),
                            );
                            buf.set_size(
                                &mut self.font_system,
                                Some(max_w),
                                Some(pn_line),
                            );
                            let attrs = Attrs::new()
                                .family(Family::Name(".AppleSystemUIFont"))
                                .weight(glyphon::Weight(500));
                            buf.set_text(
                                &mut self.font_system,
                                &name,
                                attrs,
                                Shaping::Advanced,
                            );
                            buf.shape_until_scroll(&mut self.font_system, false);
                            new_pn_cache.push((key, buf));
                        }

                        pn_label_meta.push((
                            screen_x,
                            screen_y,
                            TextColor::rgba(255, 255, 255, 180),
                            bounds,
                        ));
                    }
                }
            }
        }
        self.cached_midi_per_note_bufs = new_pn_cache;

        // Velocity label on Cmd+hovered note
        if let Some((mc_idx, note_idx)) = cmd_velocity_hover_note {
            if let Some(mc) = midi_clips.get(&mc_idx) {
                if note_idx < mc.notes.len() {
                let note = &mc.notes[note_idx];

                let vel_font = 11.0 * scale;
                let vel_line = 14.0 * scale;
                let vel_text = format!("{}", note.velocity);

                let mouse_sx = (mouse_world[0] - camera.position[0]) * camera.zoom;
                let mouse_sy = (mouse_world[1] - camera.position[1]) * camera.zoom;
                let text_w = vel_font * vel_text.len() as f32 * 0.6;
                let sx = mouse_sx + 12.0 * scale;
                let sy = mouse_sy - vel_line - 4.0 * scale;

                let mut buf = TextBuffer::new(
                    &mut self.font_system,
                    Metrics::new(vel_font, vel_line),
                );
                buf.set_size(&mut self.font_system, Some(text_w + vel_font), Some(vel_line));
                let attrs = Attrs::new()
                    .family(Family::Name(".AppleSystemUIFont"))
                    .weight(glyphon::Weight(700));
                buf.set_text(&mut self.font_system, &vel_text, attrs, Shaping::Advanced);
                buf.shape_until_scroll(&mut self.font_system, false);

                text_buffers.push(buf);
                text_meta.push((
                    sx,
                    sy,
                    TextColor::rgba(255, 255, 255, 255),
                    full_bounds,
                ));
            }
            }
        }

        // Toast text
        for te in toast_manager.build_text_entries(w, h, scale) {
            let buf = shape_text_entry(&mut self.font_system, &te);
            text_buffers.push(buf);
            text_meta.push((
                te.x,
                te.y,
                TextColor::rgba(te.color[0], te.color[1], te.color[2], te.color[3]),
                full_bounds,
            ));
        }

        // Tooltip text
        for te in tooltip.build_text_entries(scale, &settings.theme) {
            let buf = shape_text_entry(&mut self.font_system, &te);
            text_buffers.push(buf);
            text_meta.push((
                te.x,
                te.y,
                TextColor::rgba(te.color[0], te.color[1], te.color[2], te.color[3]),
                full_bounds,
            ));
        }

        self.viewport.update(
            &self.queue,
            Resolution {
                width: self.config.width,
                height: self.config.height,
            },
        );

        let mut browser_text_areas: Vec<TextArea> = Vec::new();
        if let Some(br) = sample_browser.filter(|b| b.visible) {
            let panel_w = br.panel_width(scale);
            let header_h = browser::HEADER_HEIGHT * scale;
            let content_top_h = br.content_top(scale);
            let content_bottom_h = if br.preview_audio.is_some() {
                h - browser::PREVIEW_STRIP_HEIGHT * scale
            } else {
                h
            };
            // Overlay rects that clip browser text (settings window, command palette)
            let sw_rect = settings_window.map(|sw| sw.win_rect(w, h, scale));
            let cp_rect = command_palette.map(|cp| cp.palette_rect(w, h, scale));
            // Backdrop dim factor: settings=0.5, palette=0.55
            let backdrop_alpha = if sw_rect.is_some() {
                0.5_f32
            } else if cp_rect.is_some() {
                0.55
            } else {
                1.0
            };
            for (idx, te) in br.cached_text.iter().enumerate() {
                if idx >= self.browser_text_buffers.len() {
                    break;
                }
                let is_header = te.bounds.is_some();
                let actual_y = if is_header {
                    te.y
                } else {
                    te.y - br.scroll_offset
                };
                if !is_header && (actual_y + te.line_height < content_top_h || actual_y > content_bottom_h) {
                    continue;
                }
                let clip_top = if !is_header && actual_y < content_top_h {
                    content_top_h
                } else {
                    actual_y
                };
                let mut clip_right = (panel_w - 8.0 * scale) as i32;
                if let Some((cm_pos, cm_size)) = ctx_menu_rect {
                    let overlaps = actual_y + te.line_height > cm_pos[1]
                        && actual_y < cm_pos[1] + cm_size[1]
                        && te.x < cm_pos[0] + cm_size[0];
                    if overlaps {
                        clip_right = clip_right.min(cm_pos[0] as i32);
                    }
                }
                // Clip browser text behind overlay windows
                for overlay_rect in [sw_rect, cp_rect].into_iter().flatten() {
                    let (ov_pos, ov_size) = overlay_rect;
                    let overlaps = actual_y + te.line_height > ov_pos[1]
                        && actual_y < ov_pos[1] + ov_size[1]
                        && te.x < ov_pos[0] + ov_size[0];
                    if overlaps {
                        clip_right = clip_right.min(ov_pos[0] as i32);
                    }
                }
                if clip_right <= te.x as i32 {
                    continue;
                }
                let alpha = ((te.color[3] as f32) * backdrop_alpha) as u8;
                browser_text_areas.push(TextArea {
                    buffer: &self.browser_text_buffers[idx],
                    left: te.x,
                    top: actual_y,
                    scale: 1.0,
                    default_color: TextColor::rgba(
                        te.color[0],
                        te.color[1],
                        te.color[2],
                        alpha,
                    ),
                    bounds: TextBounds {
                        left: 0,
                        top: clip_top as i32,
                        right: clip_right,
                        bottom: if is_header {
                            (actual_y + te.line_height) as i32
                        } else {
                            ((actual_y + te.line_height) as i32).min(content_bottom_h as i32)
                        },
                    },
                    custom_glyphs: &[],
                });
            }
        }

        let other_areas = text_buffers.iter().zip(text_meta.iter()).map(
            |(buffer, &(left, top, color, bounds))| TextArea {
                buffer,
                left,
                top,
                scale: 1.0,
                bounds,
                default_color: color,
                custom_glyphs: &[],
            },
        );

        fn cached_label_area<'a>(
            entry: &'a (TextLabelCacheKey, TextBuffer),
            meta: &(f32, f32, TextColor, TextBounds),
        ) -> TextArea<'a> {
            let &(left, top, color, bounds) = meta;
            TextArea {
                buffer: &entry.1,
                left,
                top,
                scale: 1.0,
                bounds,
                default_color: color,
                custom_glyphs: &[],
            }
        }
        let wf_areas = self
            .cached_wf_label_bufs
            .iter()
            .zip(wf_label_meta.iter())
            .map(|(e, m)| cached_label_area(e, m));
        let inst_label_areas = self
            .cached_inst_label_bufs
            .iter()
            .zip(inst_label_meta.iter())
            .map(|(e, m)| cached_label_area(e, m));
        let group_areas = self
            .cached_group_label_bufs
            .iter()
            .zip(grp_label_meta.iter())
            .map(|(e, m)| cached_label_area(e, m));

        let auto_dot_areas = self
            .cached_auto_dot_bufs
            .iter()
            .zip(auto_label_meta.iter())
            .map(|(e, m)| cached_label_area(e, m));

        let auto_lane_areas = self
            .cached_auto_lane_bufs
            .iter()
            .zip(lane_label_meta.iter())
            .map(|(e, m)| cached_label_area(e, m));

        let midi_note_label_areas = self
            .cached_midi_note_label_bufs
            .iter()
            .zip(midi_label_meta.iter())
            .map(|(e, m)| cached_label_area(e, m));

        let midi_per_note_areas = self
            .cached_midi_per_note_bufs
            .iter()
            .zip(pn_label_meta.iter())
            .map(|(e, m)| cached_label_area(e, m));

        let text_note_areas = self
            .cached_text_note_bufs
            .iter()
            .zip(tn_label_meta.iter())
            .map(|(e, m)| cached_label_area(e, m));

        let text_areas: Vec<TextArea> = browser_text_areas
            .into_iter()
            .chain(other_areas)
            .chain(wf_areas)
            .chain(inst_label_areas)
            .chain(group_areas)
            .chain(auto_dot_areas)
            .chain(auto_lane_areas)
            .chain(midi_note_label_areas)
            .chain(midi_per_note_areas)
            .chain(text_note_areas)
            .collect();

        self.text_renderer
            .prepare(
                &self.device,
                &self.queue,
                &mut self.font_system,
                &mut self.text_atlas,
                &self.viewport,
                text_areas,
                &mut self.swash_cache,
            )
            .unwrap();

        // Prepare top overlay text (context menu) — rendered after top overlay geometry
        let mut top_text_buffers: Vec<TextBuffer> = Vec::new();
        let mut top_text_meta: Vec<(f32, f32, TextColor, TextBounds)> = Vec::new();
        if let Some(cm) = context_menu {
            for te in cm.get_text_entries(&settings.theme, w, h, scale) {
                let buf = shape_text_entry(&mut self.font_system, &te);
                top_text_buffers.push(buf);
                top_text_meta.push((
                    te.x,
                    te.y,
                    TextColor::rgba(te.color[0], te.color[1], te.color[2], te.color[3]),
                    full_bounds,
                ));
            }
        }
        let top_text_areas: Vec<TextArea> = top_text_buffers
            .iter()
            .zip(top_text_meta.iter())
            .map(|(buffer, &(left, top, color, bounds))| TextArea {
                buffer,
                left,
                top,
                scale: 1.0,
                bounds,
                default_color: color,
                custom_glyphs: &[],
            })
            .collect();
        if !top_text_areas.is_empty() {
            self.overlay_text_renderer
                .prepare(
                    &self.device,
                    &self.queue,
                    &mut self.font_system,
                    &mut self.overlay_text_atlas,
                    &self.viewport,
                    top_text_areas,
                    &mut self.swash_cache,
                )
                .unwrap();
        }

        // --- render pass ---
        let output = match self.surface.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            Err(e) => {
                log::error!("[render] Surface error: {e:?}");
                return;
            }
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: settings.theme.bg_base[0] as f64,
                            g: settings.theme.bg_base[1] as f64,
                            b: settings.theme.bg_base[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..QUAD_INDICES.len() as u32, 0, 0..world_count as u32);

            if wf_vert_count > 0 {
                pass.set_pipeline(&self.waveform_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.waveform_vertex_buffer.slice(..));
                pass.draw(0..wf_vert_count as u32, 0..1);
            }

            if overlay_count > 0 {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.screen_camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(
                    0..QUAD_INDICES.len() as u32,
                    0,
                    world_count as u32..(world_count + overlay_count) as u32,
                );
            }

            // Preview waveform (screen-space, drawn on top of overlay/browser panel)
            if preview_wf_count > 0 {
                pass.set_pipeline(&self.waveform_pipeline);
                pass.set_bind_group(0, &self.screen_camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.waveform_vertex_buffer.slice(..));
                pass.draw(wf_vert_count as u32..(wf_vert_count + preview_wf_count) as u32, 0..1);
            }

            self.text_renderer
                .render(&self.text_atlas, &self.viewport, &mut pass)
                .unwrap();

            // Top overlay: context menu geometry drawn after all text
            if top_count > 0 {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.screen_camera_bind_group, &[]);
                pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(
                    0..QUAD_INDICES.len() as u32,
                    0,
                    (world_count + overlay_count) as u32..(world_count + overlay_count + top_count) as u32,
                );
            }
            // Context menu text drawn on top of context menu geometry
            if !top_text_buffers.is_empty() {
                self.overlay_text_renderer
                    .render(&self.overlay_text_atlas, &self.viewport, &mut pass)
                    .unwrap();
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }
}
