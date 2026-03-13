use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use glyphon::{
    Attrs, Buffer as TextBuffer, Color as TextColor, Family, FontSystem, Metrics, Resolution,
    Shaping, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::browser;
use crate::ui::context_menu::{
    ContextMenu, ContextMenuEntry, CTX_MENU_ITEM_HEIGHT, CTX_MENU_INLINE_HEIGHT,
    CTX_MENU_PADDING, CTX_MENU_SECTION_HEIGHT, CTX_MENU_SEPARATOR_HEIGHT, CTX_MENU_WIDTH,
};
use crate::effects;
use crate::ui::palette::{
    CommandPalette, PaletteMode, PaletteRow, COMMANDS, PALETTE_INPUT_HEIGHT,
    PALETTE_ITEM_HEIGHT, PALETTE_PADDING, PALETTE_SECTION_HEIGHT, PALETTE_WIDTH,
};
use crate::ui::plugin_editor;
use crate::settings::{Settings, SettingsWindow};
use crate::ui::toast;
use crate::ui::waveform;
use crate::ui::waveform::WaveformVertex;
use crate::audio::PIXELS_PER_SECOND;
use crate::{
    ExportRegion, TransportPanel, format_playback_time, DEFAULT_BPM, TRANSPORT_WIDTH,
    EXPORT_RENDER_PILL_W, EXPORT_RENDER_PILL_H,
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
    if (r < 0.01) {
        return in.color;
    }
    let center = in.rect_size * 0.5;
    let p = in.local_pos - center;
    let d = rounded_box_sdf(p, center, r);
    let fw = fwidth(d);
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
    pub(crate) viewport: Viewport,
    pub(crate) scale_factor: f32,

    pub(crate) browser_text_buffers: Vec<TextBuffer>,
    pub(crate) browser_text_generation: u64,
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
        let font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = glyphon::Cache::new(&device);
        let mut text_atlas = TextAtlas::new(&device, &queue, &cache, surface_format);
        let text_renderer = TextRenderer::new(
            &mut text_atlas,
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
            viewport,
            scale_factor,
            browser_text_buffers: Vec::new(),
            browser_text_generation: 0,
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
        command_palette: Option<&CommandPalette>,
        context_menu: Option<&ContextMenu>,
        sample_browser: Option<&browser::SampleBrowser>,
        plugin_browser: Option<(&browser::PluginBrowserSection, f32)>,
        browser_drag_ghost: Option<(&str, [f32; 2])>,
        is_playing: bool,
        is_recording: bool,
        playback_position: f64,
        export_region: Option<&ExportRegion>,
        effect_regions: &[effects::EffectRegion],
        editing_effect_name: Option<(usize, &str)>,
        waveforms: &[waveform::WaveformView],
        editing_waveform_name: Option<(usize, &str)>,
        plugin_editor: Option<&plugin_editor::PluginEditorWindow>,
        settings_window: Option<&SettingsWindow>,
        settings: &Settings,
        toast_manager: &toast::ToastManager,
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
        if wf_vert_count > 0 {
            self.queue.write_buffer(
                &self.waveform_vertex_buffer,
                0,
                bytemuck::cast_slice(&waveform_vertices[..wf_vert_count]),
            );
        }

        // Build overlay instances: browser panel + drag ghost + command palette
        let mut overlay_instances: Vec<InstanceRaw> = Vec::new();

        if let Some(br) = sample_browser {
            overlay_instances.extend(br.build_instances(w, h, self.scale_factor));
        }

        if let Some((pb, y_offset)) = plugin_browser {
            let panel_w = sample_browser.map_or(260.0 * self.scale_factor, |b| b.panel_width(self.scale_factor));
            let clip_top = browser::HEADER_HEIGHT * self.scale_factor;
            overlay_instances.extend(pb.build_instances(panel_w, y_offset, h, self.scale_factor, clip_top));
        }

        if let Some((_, pos)) = browser_drag_ghost {
            overlay_instances.push(InstanceRaw {
                position: [pos[0] - 4.0, pos[1] - 4.0],
                size: [160.0 * self.scale_factor, 24.0 * self.scale_factor],
                color: [0.20, 0.20, 0.28, 0.90],
                border_radius: 4.0 * self.scale_factor,
            });
        }

        if let Some(p) = command_palette {
            overlay_instances.extend(p.build_instances(w, h, self.scale_factor));
        }

        if let Some(cm) = context_menu {
            overlay_instances.extend(cm.build_instances(w, h, self.scale_factor));
        }

        if let Some(sw) = settings_window {
            overlay_instances.extend(sw.build_instances(settings, w, h, self.scale_factor));
        }

        if let Some(pe) = plugin_editor {
            overlay_instances.extend(pe.build_instances(w, h, self.scale_factor));
        }

        overlay_instances.extend(TransportPanel::build_instances(
            w,
            h,
            self.scale_factor,
            is_playing,
            is_recording,
        ));

        overlay_instances.extend(TransportPanel::build_fx_button_instances(
            w,
            h,
            self.scale_factor,
        ));

        overlay_instances.extend(TransportPanel::build_export_button_instances(
            w,
            h,
            self.scale_factor,
        ));

        overlay_instances.extend(toast_manager.build_instances(w, h, self.scale_factor));

        let overlay_count = overlay_instances.len().min(MAX_INSTANCES - world_count);
        if overlay_count > 0 {
            let offset = (world_count * std::mem::size_of::<InstanceRaw>()) as u64;
            self.queue.write_buffer(
                &self.instance_buffer,
                offset,
                bytemuck::cast_slice(&overlay_instances[..overlay_count]),
            );
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
            if br.text_generation != self.browser_text_generation {
                self.browser_text_buffers.clear();
                for te in &br.cached_text {
                    let mut buf = TextBuffer::new(
                        &mut self.font_system,
                        Metrics::new(te.font_size, te.line_height),
                    );
                    buf.set_size(
                        &mut self.font_system,
                        Some(te.max_width),
                        Some(te.line_height),
                    );
                    let attrs = Attrs::new()
                        .family(Family::Name(".AppleSystemUIFont"))
                        .weight(glyphon::Weight(te.weight));
                    buf.set_text(&mut self.font_system, &te.text, attrs, Shaping::Advanced);
                    buf.shape_until_scroll(&mut self.font_system, false);
                    self.browser_text_buffers.push(buf);
                }
                self.browser_text_generation = br.text_generation;
            }
        } else if !self.browser_text_buffers.is_empty() {
            self.browser_text_buffers.clear();
        }

        // Plugin browser section text
        if let Some((pb, _)) = plugin_browser {
            let panel_w = sample_browser.map_or(260.0 * scale, |b| b.panel_width(scale));
            let clip_top = browser::HEADER_HEIGHT * scale;
            for te in &pb.cached_text {
                let actual_y = te.base_y;
                if actual_y + te.line_height < clip_top || actual_y > h {
                    continue;
                }
                let mut buf = TextBuffer::new(
                    &mut self.font_system,
                    Metrics::new(te.font_size, te.line_height),
                );
                buf.set_size(&mut self.font_system, Some(te.max_width), Some(te.line_height));
                let attrs = Attrs::new()
                    .family(Family::Name(".AppleSystemUIFont"))
                    .weight(glyphon::Weight(te.weight));
                buf.set_text(&mut self.font_system, &te.text, attrs, Shaping::Advanced);
                buf.shape_until_scroll(&mut self.font_system, false);
                text_buffers.push(buf);
                text_meta.push((
                    te.x,
                    actual_y,
                    TextColor::rgba(te.color[0], te.color[1], te.color[2], te.color[3]),
                    TextBounds {
                        left: 0,
                        top: (actual_y.max(clip_top)) as i32,
                        right: (panel_w - 8.0 * scale) as i32,
                        bottom: (actual_y + te.line_height) as i32,
                    },
                ));
            }
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
            text_meta.push((
                pos[0] + 4.0 * scale,
                pos[1] - 4.0 + (24.0 * scale - line_h) * 0.5,
                TextColor::rgb(220, 220, 230),
                full_bounds,
            ));
        }

        if let Some(palette) = command_palette {
            let (ppos, _psize) = palette.palette_rect(w, h, scale);
            let margin = PALETTE_PADDING * scale;
            let list_top = ppos[1] + PALETTE_INPUT_HEIGHT * scale + 1.0 * scale;

            // Search input text (or placeholder)
            let (display_text, search_color) = match palette.mode {
                PaletteMode::VolumeFader => {
                    ("Master Volume", TextColor::rgb(235, 235, 240))
                }
                _ if palette.search_text.is_empty() => {
                    ("Search", TextColor::rgba(140, 140, 150, 160))
                }
                _ => {
                    (palette.search_text.as_str(), TextColor::rgb(235, 235, 240))
                }
            };
            let sfont = 15.0 * scale;
            let sline = 22.0 * scale;
            let mut buf = TextBuffer::new(&mut self.font_system, Metrics::new(sfont, sline));
            buf.set_size(
                &mut self.font_system,
                Some(PALETTE_WIDTH * scale - 60.0 * scale),
                Some(PALETTE_INPUT_HEIGHT * scale),
            );
            buf.set_text(
                &mut self.font_system,
                display_text,
                Attrs::new().family(Family::SansSerif),
                Shaping::Advanced,
            );
            buf.shape_until_scroll(&mut self.font_system, false);
            text_buffers.push(buf);
            text_meta.push((
                ppos[0] + 36.0 * scale,
                ppos[1] + (PALETTE_INPUT_HEIGHT * scale - sline) * 0.5,
                search_color,
                full_bounds,
            ));

            match palette.mode {
                PaletteMode::VolumeFader => {
                    let pad = 16.0 * scale;
                    let track_y = list_top + 36.0 * scale;
                    let track_h = 6.0 * scale;
                    let rms_y = track_y + track_h + 22.0 * scale;

                    let pct = (palette.fader_value * 100.0) as u32;
                    let vol_text = format!("{}%", pct);
                    let label_font = 13.0 * scale;
                    let label_line = 18.0 * scale;
                    let mut buf = TextBuffer::new(
                        &mut self.font_system,
                        Metrics::new(label_font, label_line),
                    );
                    buf.set_size(
                        &mut self.font_system,
                        Some(PALETTE_WIDTH * scale - margin * 2.0),
                        Some(20.0 * scale),
                    );
                    buf.set_text(
                        &mut self.font_system,
                        &vol_text,
                        Attrs::new().family(Family::SansSerif),
                        Shaping::Advanced,
                    );
                    buf.shape_until_scroll(&mut self.font_system, false);
                    text_buffers.push(buf);
                    text_meta.push((
                        ppos[0] + margin + pad,
                        list_top + 14.0 * scale,
                        TextColor::rgba(200, 200, 210, 220),
                        full_bounds,
                    ));

                    let db_val = if palette.fader_rms > 0.0001 {
                        20.0 * palette.fader_rms.log10()
                    } else {
                        -60.0
                    };
                    let rms_text = format!("RMS: {:.1} dB", db_val);
                    let small_font = 11.0 * scale;
                    let small_line = 15.0 * scale;
                    let mut buf = TextBuffer::new(
                        &mut self.font_system,
                        Metrics::new(small_font, small_line),
                    );
                    buf.set_size(
                        &mut self.font_system,
                        Some(PALETTE_WIDTH * scale - margin * 2.0),
                        Some(16.0 * scale),
                    );
                    buf.set_text(
                        &mut self.font_system,
                        &rms_text,
                        Attrs::new().family(Family::SansSerif),
                        Shaping::Advanced,
                    );
                    buf.shape_until_scroll(&mut self.font_system, false);
                    text_buffers.push(buf);
                    text_meta.push((
                        ppos[0] + margin + pad,
                        rms_y + 8.0 * scale,
                        TextColor::rgba(140, 140, 150, 180),
                        full_bounds,
                    ));
                }
                PaletteMode::Commands => {
                    let sect_font = 11.0 * scale;
                    let sect_line = 16.0 * scale;
                    let ifont = 13.5 * scale;
                    let iline = 20.0 * scale;
                    let shortcut_font = 12.0 * scale;
                    let shortcut_line = 17.0 * scale;

                    let mut y = list_top;
                    for row in palette.visible_rows() {
                        match row {
                            PaletteRow::Section(label) => {
                                let mut buf = TextBuffer::new(
                                    &mut self.font_system,
                                    Metrics::new(sect_font, sect_line),
                                );
                                buf.set_size(
                                    &mut self.font_system,
                                    Some(PALETTE_WIDTH * scale - margin * 4.0),
                                    Some(PALETTE_SECTION_HEIGHT * scale),
                                );
                                buf.set_text(
                                    &mut self.font_system,
                                    label,
                                    Attrs::new().family(Family::SansSerif),
                                    Shaping::Advanced,
                                );
                                buf.shape_until_scroll(&mut self.font_system, false);
                                text_buffers.push(buf);
                                text_meta.push((
                                    ppos[0] + margin + 12.0 * scale,
                                    y + (PALETTE_SECTION_HEIGHT * scale - sect_line) * 0.5
                                        + 2.0 * scale,
                                    TextColor::rgba(120, 140, 170, 200),
                                    full_bounds,
                                ));
                                y += PALETTE_SECTION_HEIGHT * scale;
                            }
                            PaletteRow::Command(ci) => {
                                let cmd = &COMMANDS[*ci];

                                let mut buf = TextBuffer::new(
                                    &mut self.font_system,
                                    Metrics::new(ifont, iline),
                                );
                                buf.set_size(
                                    &mut self.font_system,
                                    Some(PALETTE_WIDTH * scale * 0.65),
                                    Some(PALETTE_ITEM_HEIGHT * scale),
                                );
                                buf.set_text(
                                    &mut self.font_system,
                                    cmd.name,
                                    Attrs::new().family(Family::SansSerif),
                                    Shaping::Advanced,
                                );
                                buf.shape_until_scroll(&mut self.font_system, false);
                                text_buffers.push(buf);
                                text_meta.push((
                                    ppos[0] + margin + 12.0 * scale,
                                    y + (PALETTE_ITEM_HEIGHT * scale - iline) * 0.5,
                                    TextColor::rgb(215, 215, 222),
                                    full_bounds,
                                ));

                                if !cmd.shortcut.is_empty() {
                                    let mut buf = TextBuffer::new(
                                        &mut self.font_system,
                                        Metrics::new(shortcut_font, shortcut_line),
                                    );
                                    buf.set_size(
                                        &mut self.font_system,
                                        Some(80.0 * scale),
                                        Some(PALETTE_ITEM_HEIGHT * scale),
                                    );
                                    buf.set_text(
                                        &mut self.font_system,
                                        cmd.shortcut,
                                        Attrs::new().family(Family::SansSerif),
                                        Shaping::Advanced,
                                    );
                                    buf.shape_until_scroll(&mut self.font_system, false);
                                    text_buffers.push(buf);
                                    text_meta.push((
                                        ppos[0] + PALETTE_WIDTH * scale - margin - 70.0 * scale,
                                        y + (PALETTE_ITEM_HEIGHT * scale - shortcut_line) * 0.5,
                                        TextColor::rgba(120, 120, 135, 180),
                                        full_bounds,
                                    ));
                                }

                                y += PALETTE_ITEM_HEIGHT * scale;
                            }
                        }
                    }
                }
            }
        }

        if let Some(cm) = context_menu {
            let (mpos, _msize) = cm.menu_rect(w, h, scale);
            let pad = CTX_MENU_PADDING * scale;
            let label_font = 13.0 * scale;
            let label_line = 18.0 * scale;
            let shortcut_font = 12.0 * scale;
            let shortcut_line = 17.0 * scale;
            let section_font = 11.0 * scale;
            let section_line = 15.0 * scale;
            let has_any_checked = cm.entries.iter().any(|e| matches!(e, ContextMenuEntry::Item(it) if it.checked));
            let check_indent = if has_any_checked { 16.0 * scale } else { 0.0 };

            let mut y = mpos[1] + pad;
            for entry in &cm.entries {
                match entry {
                    ContextMenuEntry::Item(item) => {
                        let mut buf = TextBuffer::new(
                            &mut self.font_system,
                            Metrics::new(label_font, label_line),
                        );
                        buf.set_size(
                            &mut self.font_system,
                            Some(CTX_MENU_WIDTH * scale * 0.55),
                            Some(CTX_MENU_ITEM_HEIGHT * scale),
                        );
                        buf.set_text(
                            &mut self.font_system,
                            item.label,
                            Attrs::new().family(Family::SansSerif),
                            Shaping::Advanced,
                        );
                        buf.shape_until_scroll(&mut self.font_system, false);
                        text_buffers.push(buf);
                        text_meta.push((
                            mpos[0] + pad + 10.0 * scale + check_indent,
                            y + (CTX_MENU_ITEM_HEIGHT * scale - label_line) * 0.5,
                            TextColor::rgb(220, 220, 228),
                            full_bounds,
                        ));

                        if !item.shortcut.is_empty() {
                            let mut buf = TextBuffer::new(
                                &mut self.font_system,
                                Metrics::new(shortcut_font, shortcut_line),
                            );
                            buf.set_size(
                                &mut self.font_system,
                                Some(60.0 * scale),
                                Some(CTX_MENU_ITEM_HEIGHT * scale),
                            );
                            buf.set_text(
                                &mut self.font_system,
                                item.shortcut,
                                Attrs::new().family(Family::SansSerif),
                                Shaping::Advanced,
                            );
                            buf.shape_until_scroll(&mut self.font_system, false);
                            text_buffers.push(buf);
                            text_meta.push((
                                mpos[0] + CTX_MENU_WIDTH * scale - pad - 50.0 * scale,
                                y + (CTX_MENU_ITEM_HEIGHT * scale - shortcut_line) * 0.5,
                                TextColor::rgba(120, 120, 135, 180),
                                full_bounds,
                            ));
                        }

                        y += CTX_MENU_ITEM_HEIGHT * scale;
                    }
                    ContextMenuEntry::Separator => {
                        y += CTX_MENU_SEPARATOR_HEIGHT * scale;
                    }
                    ContextMenuEntry::SectionHeader(label) => {
                        let mut buf = TextBuffer::new(
                            &mut self.font_system,
                            Metrics::new(section_font, section_line),
                        );
                        buf.set_size(
                            &mut self.font_system,
                            Some(CTX_MENU_WIDTH * scale * 0.8),
                            Some(CTX_MENU_SECTION_HEIGHT * scale),
                        );
                        buf.set_text(
                            &mut self.font_system,
                            label,
                            Attrs::new().family(Family::SansSerif),
                            Shaping::Advanced,
                        );
                        buf.shape_until_scroll(&mut self.font_system, false);
                        text_buffers.push(buf);
                        text_meta.push((
                            mpos[0] + pad + 10.0 * scale,
                            y + (CTX_MENU_SECTION_HEIGHT * scale - section_line) * 0.5,
                            TextColor::rgba(150, 150, 160, 200),
                            full_bounds,
                        ));
                        y += CTX_MENU_SECTION_HEIGHT * scale;
                    }
                    ContextMenuEntry::InlineGroup(pills) => {
                        let row_h = CTX_MENU_INLINE_HEIGHT * scale;
                        let pill_h = 22.0 * scale;
                        let pill_pad_x = 7.0 * scale;
                        let pill_gap = 2.0 * scale;
                        let pill_font = 11.0 * scale;
                        let pill_line = 15.0 * scale;
                        let pill_y = y + (row_h - pill_h) * 0.5;
                        let mut px = mpos[0] + pad + 4.0 * scale;
                        for pill in pills {
                            let pw = pill.label.len() as f32 * pill_font * 0.55 + pill_pad_x * 2.0;
                            let alpha: u8 = if pill.active { 240 } else { 160 };
                            let mut buf = TextBuffer::new(
                                &mut self.font_system,
                                Metrics::new(pill_font, pill_line),
                            );
                            buf.set_size(&mut self.font_system, Some(pw), Some(pill_line));
                            buf.set_text(
                                &mut self.font_system,
                                pill.label,
                                Attrs::new().family(Family::SansSerif),
                                Shaping::Advanced,
                            );
                            buf.shape_until_scroll(&mut self.font_system, false);
                            text_buffers.push(buf);
                            text_meta.push((
                                px + pill_pad_x,
                                pill_y + (pill_h - pill_line) * 0.5,
                                TextColor::rgba(220, 220, 230, alpha),
                                full_bounds,
                            ));
                            px += pw + pill_gap;
                        }
                        y += row_h;
                    }
                }
            }
        }

        // Plugin editor text
        if let Some(pe) = plugin_editor {
            for te in pe.get_text_entries(w, h, scale) {
                let mut buf = TextBuffer::new(
                    &mut self.font_system,
                    Metrics::new(te.font_size, te.line_height),
                );
                buf.set_size(&mut self.font_system, Some(te.max_width), Some(te.line_height * 2.0));
                let attrs = Attrs::new()
                    .family(Family::Name(".AppleSystemUIFont"))
                    .weight(glyphon::Weight(te.weight));
                buf.set_text(&mut self.font_system, &te.text, attrs, Shaping::Advanced);
                buf.shape_until_scroll(&mut self.font_system, false);
                text_buffers.push(buf);
                text_meta.push((
                    te.x,
                    te.y,
                    TextColor::rgba(te.color[0], te.color[1], te.color[2], te.color[3]),
                    full_bounds,
                ));
            }
        }

        // Settings window text
        if let Some(sw) = settings_window {
            for te in sw.get_text_entries(settings, w, h, scale) {
                let mut buf = TextBuffer::new(
                    &mut self.font_system,
                    Metrics::new(te.font_size, te.line_height),
                );
                buf.set_size(&mut self.font_system, Some(300.0 * scale), Some(te.line_height * 2.0));
                let attrs = Attrs::new()
                    .family(Family::Name(".AppleSystemUIFont"))
                    .weight(glyphon::Weight(te.weight));
                buf.set_text(&mut self.font_system, &te.text, attrs, Shaping::Advanced);
                buf.shape_until_scroll(&mut self.font_system, false);
                text_buffers.push(buf);
                text_meta.push((
                    te.x,
                    te.y,
                    TextColor::rgba(te.color[0], te.color[1], te.color[2], te.color[3]),
                    full_bounds,
                ));
            }
        }

        // Compute context menu rect for text overlap checks
        let ctx_menu_rect: Option<([f32; 2], [f32; 2])> =
            context_menu.map(|cm| cm.menu_rect(w, h, scale));

        // Export region "Render" label with duration (world-space -> screen-space)
        if let Some(er) = export_region {
            if settings_window.is_none() && command_palette.is_none() {
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
                    let mut buf = TextBuffer::new(&mut self.font_system, Metrics::new(label_font, label_line));
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

        let world_left = camera.position[0];
        let world_right = world_left + w / camera.zoom;
        let world_top = camera.position[1];
        let world_bottom = world_top + h / camera.zoom;

        // Effect region name labels (world-space -> screen-space)
        for (er_idx, er) in effect_regions.iter().enumerate() {
            let er_right = er.position[0] + er.size[0];
            let er_bottom = er.position[1] + er.size[1];
            if er_right < world_left || er.position[0] > world_right
                || er_bottom < world_top || er.position[1] > world_bottom
            {
                continue;
            }
            let region_screen_w = er.size[0] * camera.zoom;
            if region_screen_w < 30.0 {
                continue;
            }

            let pad = 6.0 / camera.zoom;
            let name_x_world = er.position[0] + pad;
            let name_y_world = er.position[1] - 18.0 / camera.zoom;
            let name_screen_x = (name_x_world - camera.position[0]) * camera.zoom;
            let name_screen_y = (name_y_world - camera.position[1]) * camera.zoom;

            // Skip if full-screen overlay is open
            if settings_window.is_some() || command_palette.is_some() {
                continue;
            }
            // Skip if overlapping context menu
            if let Some((cm_pos, cm_size)) = ctx_menu_rect {
                let max_text_w = (region_screen_w - 12.0 * scale).max(20.0);
                let name_line = 14.0 * scale;
                if name_screen_x + max_text_w > cm_pos[0]
                    && name_screen_x < cm_pos[0] + cm_size[0]
                    && name_screen_y + name_line > cm_pos[1]
                    && name_screen_y < cm_pos[1] + cm_size[1]
                {
                    continue;
                }
            }

            let display_name = if let Some((idx, ref text)) = editing_effect_name {
                if idx == er_idx { format!("{}|", text) } else { er.name.clone() }
            } else {
                er.name.clone()
            };

            let name_font = 10.0 * scale;
            let name_line = 14.0 * scale;
            let mut buf = TextBuffer::new(
                &mut self.font_system,
                Metrics::new(name_font, name_line),
            );
            let max_text_w = (region_screen_w - 12.0 * scale).max(20.0);
            buf.set_size(
                &mut self.font_system,
                Some(max_text_w),
                Some(name_line),
            );
            let is_editing = editing_effect_name.map_or(false, |(idx, _)| idx == er_idx);
            let alpha = if is_editing { 255 } else { 180 };
            let attrs = Attrs::new()
                .family(Family::Name(".AppleSystemUIFont"))
                .weight(glyphon::Weight(500));
            buf.set_text(&mut self.font_system, &display_name, attrs, Shaping::Advanced);
            buf.shape_until_scroll(&mut self.font_system, false);
            text_buffers.push(buf);
            text_meta.push((
                name_screen_x,
                name_screen_y,
                TextColor::rgba(255, 255, 255, alpha),
                full_bounds,
            ));
        }

        // Waveform sample name labels (world-space -> screen-space)
        for (wf_idx, wf) in waveforms.iter().enumerate() {
            let wf_right = wf.position[0] + wf.size[0];
            let wf_bottom = wf.position[1] + wf.size[1];
            if wf_right < world_left || wf.position[0] > world_right
                || wf_bottom < world_top || wf.position[1] > world_bottom
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

            // Skip if full-screen overlay is open
            if settings_window.is_some() || command_palette.is_some() {
                continue;
            }
            // Skip if overlapping context menu
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
                if idx == wf_idx { format!("{}|", text) } else { wf.audio.filename.clone() }
            } else {
                wf.audio.filename.clone()
            };

            let name_font = 10.0 * scale;
            let name_line = 14.0 * scale;
            let mut buf = TextBuffer::new(
                &mut self.font_system,
                Metrics::new(name_font, name_line),
            );
            let max_text_w = (clip_screen_w - 12.0 * scale).max(20.0);
            buf.set_size(
                &mut self.font_system,
                Some(max_text_w),
                Some(name_line),
            );
            let is_editing = editing_waveform_name.map_or(false, |(idx, _)| idx == wf_idx);
            let alpha = if is_editing { 255 } else { 180 };
            let attrs = Attrs::new()
                .family(Family::Name(".AppleSystemUIFont"))
                .weight(glyphon::Weight(500));
            buf.set_text(&mut self.font_system, &display_name, attrs, Shaping::Advanced);
            buf.shape_until_scroll(&mut self.font_system, false);
            text_buffers.push(buf);
            text_meta.push((
                name_screen_x,
                name_screen_y,
                TextColor::rgba(255, 255, 255, alpha),
                full_bounds,
            ));
        }

        // Effect region plugin name labels (world-space -> screen-space)
        for er in effect_regions {
            // Skip entire loop if full-screen overlay is open
            if settings_window.is_some() || command_palette.is_some() {
                break;
            }
            let er_right = er.position[0] + er.size[0];
            let er_bottom = er.position[1] + er.size[1];
            if er_right < world_left || er.position[0] > world_right
                || er_bottom < world_top || er.position[1] > world_bottom
            {
                continue;
            }
            let labels = effects::plugin_label_rects(er, camera);
            for (i, rect) in labels.iter().enumerate() {
                let screen_x = (rect.position[0] - camera.position[0]) * camera.zoom;
                let screen_y = (rect.position[1] - camera.position[1]) * camera.zoom;
                let pill_w_screen = rect.size[0] * camera.zoom;
                let pill_h_screen = rect.size[1] * camera.zoom;

                // Skip if overlapping context menu
                if let Some((cm_pos, cm_size)) = ctx_menu_rect {
                    if screen_x + pill_w_screen > cm_pos[0]
                        && screen_x < cm_pos[0] + cm_size[0]
                        && screen_y + pill_h_screen > cm_pos[1]
                        && screen_y < cm_pos[1] + cm_size[1]
                    {
                        continue;
                    }
                }

                let name = &er.chain[i].plugin_name;
                let label_font = 10.0 * scale;
                let label_line = 14.0 * scale;
                let mut buf = TextBuffer::new(
                    &mut self.font_system,
                    Metrics::new(label_font, label_line),
                );
                buf.set_size(
                    &mut self.font_system,
                    Some(pill_w_screen - 8.0),
                    Some(pill_h_screen),
                );
                let attrs = Attrs::new()
                    .family(Family::Name(".AppleSystemUIFont"))
                    .weight(glyphon::Weight(500));
                buf.set_text(&mut self.font_system, name, attrs, Shaping::Advanced);
                buf.shape_until_scroll(&mut self.font_system, false);
                text_buffers.push(buf);
                text_meta.push((
                    screen_x + 4.0 * scale,
                    screen_y + (pill_h_screen - label_line) * 0.5,
                    TextColor::rgba(255, 255, 255, 220),
                    full_bounds,
                ));
            }
        }

        // Transport panel time text
        {
            let (tp_pos, tp_size) = TransportPanel::panel_rect(w, h, scale);
            let time_str = format_playback_time(playback_position);
            let tfont = 13.0 * scale;
            let tline = 18.0 * scale;
            let mut buf = TextBuffer::new(&mut self.font_system, Metrics::new(tfont, tline));
            buf.set_size(
                &mut self.font_system,
                Some(TRANSPORT_WIDTH * scale * 0.6),
                Some(tline),
            );
            buf.set_text(
                &mut self.font_system,
                &time_str,
                Attrs::new().family(Family::SansSerif),
                Shaping::Advanced,
            );
            buf.shape_until_scroll(&mut self.font_system, false);
            text_buffers.push(buf);
            text_meta.push((
                tp_pos[0] + 38.0 * scale,
                tp_pos[1] + (tp_size[1] - tline) * 0.5,
                TextColor::rgba(220, 220, 230, 220),
                full_bounds,
            ));
        }

        // Transport panel BPM text
        {
            let (tp_pos, tp_size) = TransportPanel::panel_rect(w, h, scale);
            let bpm_str = format!("{} bpm", DEFAULT_BPM as u32);
            let tfont = 13.0 * scale;
            let tline = 18.0 * scale;
            let mut buf = TextBuffer::new(&mut self.font_system, Metrics::new(tfont, tline));
            buf.set_size(&mut self.font_system, Some(80.0 * scale), Some(tline));
            buf.set_text(
                &mut self.font_system,
                &bpm_str,
                Attrs::new().family(Family::SansSerif),
                Shaping::Advanced,
            );
            buf.shape_until_scroll(&mut self.font_system, false);
            text_buffers.push(buf);
            text_meta.push((
                tp_pos[0] + tp_size[0] - 80.0 * scale,
                tp_pos[1] + (tp_size[1] - tline) * 0.5,
                TextColor::rgba(220, 220, 230, 220),
                full_bounds,
            ));
        }

        // Toast text
        for te in toast_manager.build_text_elements(w, h, scale) {
            let mut buf = TextBuffer::new(
                &mut self.font_system,
                Metrics::new(te.font_size, te.line_height),
            );
            buf.set_size(&mut self.font_system, Some(te.max_width), Some(te.line_height));
            buf.set_text(
                &mut self.font_system,
                &te.text,
                Attrs::new().family(Family::SansSerif),
                Shaping::Advanced,
            );
            buf.shape_until_scroll(&mut self.font_system, false);
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
        if let Some(br) = sample_browser {
            // Skip all browser text when a full-screen overlay is open
            if settings_window.is_none() && command_palette.is_none() {
            let panel_w = br.panel_width(scale);
            let header_h = browser::HEADER_HEIGHT * scale;
            for (idx, te) in br.cached_text.iter().enumerate() {
                if idx >= self.browser_text_buffers.len() {
                    break;
                }
                let actual_y = if te.is_header {
                    te.base_y
                } else {
                    te.base_y - br.scroll_offset
                };
                if !te.is_header && (actual_y + te.line_height < header_h || actual_y > h) {
                    continue;
                }
                // Skip if overlapping context menu
                if let Some((cm_pos, cm_size)) = ctx_menu_rect {
                    let text_right = panel_w - 8.0 * scale;
                    if te.x + text_right > cm_pos[0]
                        && te.x < cm_pos[0] + cm_size[0]
                        && actual_y + te.line_height > cm_pos[1]
                        && actual_y < cm_pos[1] + cm_size[1]
                    {
                        continue;
                    }
                }
                let clip_top = if actual_y < header_h {
                    header_h
                } else {
                    actual_y
                };
                browser_text_areas.push(TextArea {
                    buffer: &self.browser_text_buffers[idx],
                    left: te.x,
                    top: actual_y,
                    scale: 1.0,
                    default_color: TextColor::rgba(
                        te.color[0],
                        te.color[1],
                        te.color[2],
                        te.color[3],
                    ),
                    bounds: TextBounds {
                        left: 0,
                        top: clip_top as i32,
                        right: (panel_w - 8.0 * scale) as i32,
                        bottom: (actual_y + te.line_height) as i32,
                    },
                    custom_glyphs: &[],
                });
            }
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

        let text_areas: Vec<TextArea> = browser_text_areas.into_iter().chain(other_areas).collect();

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

        // --- render pass ---
        let output = match self.surface.get_current_texture() {
            Ok(t) => t,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            Err(e) => {
                log::error!("Surface error: {e:?}");
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
                            r: 0.09 * settings.brightness as f64,
                            g: 0.09 * settings.brightness as f64,
                            b: 0.12 * settings.brightness as f64,
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

            self.text_renderer
                .render(&self.text_atlas, &self.viewport, &mut pass)
                .unwrap();
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }
}
