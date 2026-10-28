//! Bind group layouts, the sampler, the vertex and uniform formats, and every render pipeline.

use bytemuck::{Pod, Zeroable};
use kagari_shader::{MAX_ARGS_BYTES, TRANSFORMS_BYTES};

/// The format of the design target, the layers and the still.
pub(crate) const LINEAR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// The format `render_to_rgba` resolves into.
pub(crate) const READBACK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// One corner of a quad, in target pixels.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(crate) struct Vertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    /// Premultiplied multiplier.
    pub color: [f32; 4],
    /// 0..1 across the quad.
    pub local: [f32; 2],
}

const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 4] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32x2];

pub(crate) const VERTEX_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
    array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
    step_mode: wgpu::VertexStepMode::Vertex,
    attributes: &VERTEX_ATTRIBUTES,
};

/// The uniforms of one draw (`Uniforms` in `color.wgsl`): the target size, the motion slot of the
/// vertices, then the per-draw parameters `a`, `b`, `c` and the blur weights. The first 64 bytes
/// are `KgFrame` of the user shader prelude.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(crate) struct Uniforms {
    pub target_size: [f32; 2],
    pub xf_slot: u32,
    pub pad0: u32,
    pub a: [f32; 4],
    pub b: [f32; 4],
    pub c: [f32; 4],
    pub weights: [[f32; 4]; 25],
}

impl Uniforms {
    pub fn new(target_size: (u32, u32)) -> Uniforms {
        Uniforms {
            target_size: [target_size.0 as f32, target_size.1 as f32],
            ..Zeroable::zeroed()
        }
    }
}

/// The byte size of [`Uniforms`].
pub(crate) const UNIFORM_SIZE: u64 = std::mem::size_of::<Uniforms>() as u64;

/// The uniform of one motion stage (`KgMotionIn` of the motion prelude, 112 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(crate) struct MotionUniform {
    /// Parent affine rows `(a, c, e, 0)` and `(b, d, f, 0)`.
    pub parent0: [f32; 4],
    pub parent1: [f32; 4],
    /// `(x, y, scale, rotation)` of the CPU-sampled pose.
    pub pose: [f32; 4],
    pub from: [f32; 4],
    pub to: [f32; 4],
    /// `(time, progress, 0, 0)`.
    pub timing: [f32; 4],
    pub slot: u32,
    pub chained: u32,
    pub pad: [u32; 2],
}

/// The byte size of [`MotionUniform`].
pub(crate) const MOTION_UNIFORM_SIZE: u64 = std::mem::size_of::<MotionUniform>() as u64;

/// The stride of one uniform slot: `UNIFORM_SIZE` rounded up to the dynamic-offset alignment
/// (at least 256).
pub(crate) fn uniform_slot(limits: &wgpu::Limits) -> u64 {
    let align = u64::from(limits.min_uniform_buffer_offset_alignment.max(256));
    UNIFORM_SIZE.div_ceil(align).saturating_mul(align)
}

/// A render pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Pipe {
    /// An image texel decoded to linear, times the vertex colour; premultiplied over.
    Image,
    /// A linear texel times the vertex colour; premultiplied over.
    Linear,
    /// Coverage times the vertex colour; premultiplied over.
    Mask,
    /// As `Mask`, per-channel maximum.
    MaskMax,
    /// A layer times the red channel of a mask layer; premultiplied over.
    MaskComposite,
    /// One Gaussian blur pass; replaces.
    Blur,
    /// A transition between two layers; premultiplied over.
    Transition,
    /// The design target onto the surface.
    Present,
    /// The design target into the readback texture.
    Resolve,
}

/// The pipeline layouts of user shaders; absent when the device has no storage buffers.
pub(crate) struct UserLayouts {
    /// Group 2 of every role: the arguments (read-only storage, dynamic offset).
    pub args: wgpu::BindGroupLayout,
    pub color: wgpu::PipelineLayout,
    pub mix: wgpu::PipelineLayout,
    pub texture: wgpu::PipelineLayout,
    pub mask: wgpu::PipelineLayout,
    /// Absent when the device has no compute shaders.
    pub motion: Option<MotionLayouts>,
}

/// The layouts of motion (compute) shaders.
pub(crate) struct MotionLayouts {
    /// Group 0: the `KgMotionIn` uniform (dynamic offset).
    pub uniform: wgpu::BindGroupLayout,
    /// Group 1: the motion states and the displacement rows (read-write storage).
    pub buffers: wgpu::BindGroupLayout,
    pub layout: wgpu::PipelineLayout,
}

/// The layouts, the sampler and the pipelines shared by every frame.
pub(crate) struct Pipelines {
    /// Group 0 of raster pipelines: the per-draw uniforms and the displacement rows.
    pub uniforms: wgpu::BindGroupLayout,
    /// One texture plus the sampler.
    pub tex1: wgpu::BindGroupLayout,
    /// Two textures plus the sampler.
    pub tex2: wgpu::BindGroupLayout,
    /// Three textures plus the sampler.
    pub tex3: wgpu::BindGroupLayout,
    /// The (empty) group 1 of mask shaders, and its bind group.
    pub empty_bind: wgpu::BindGroup,
    pub sampler: wgpu::Sampler,
    pub user: Option<UserLayouts>,
    image: wgpu::RenderPipeline,
    linear: wgpu::RenderPipeline,
    mask: wgpu::RenderPipeline,
    mask_max: wgpu::RenderPipeline,
    mask_composite: wgpu::RenderPipeline,
    blur: wgpu::RenderPipeline,
    transition: wgpu::RenderPipeline,
    present: wgpu::RenderPipeline,
    resolve: wgpu::RenderPipeline,
}

pub(crate) const PREMUL: wgpu::BlendState = wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING;

const MAX_COMPONENT: wgpu::BlendComponent = wgpu::BlendComponent {
    src_factor: wgpu::BlendFactor::One,
    dst_factor: wgpu::BlendFactor::One,
    operation: wgpu::BlendOperation::Max,
};

const MAX: wgpu::BlendState = wgpu::BlendState {
    color: MAX_COMPONENT,
    alpha: MAX_COMPONENT,
};

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn textures_layout(device: &wgpu::Device, label: &str, textures: u32) -> wgpu::BindGroupLayout {
    let mut entries: Vec<wgpu::BindGroupLayoutEntry> = (0..textures).map(texture_entry).collect();
    entries.push(wgpu::BindGroupLayoutEntry {
        binding: textures,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    });
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &entries,
    })
}

fn module(device: &wgpu::Device, label: &str, source: &'static str) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    })
}

pub(crate) struct PipeDesc<'a> {
    pub label: &'a str,
    pub layout: &'a wgpu::PipelineLayout,
    pub module: &'a wgpu::ShaderModule,
    pub vertex: &'a str,
    pub fragment: &'a str,
    pub format: wgpu::TextureFormat,
    pub blend: Option<wgpu::BlendState>,
    pub quad: bool,
}

pub(crate) fn pipeline(device: &wgpu::Device, d: PipeDesc<'_>) -> wgpu::RenderPipeline {
    let buffers = [Some(VERTEX_LAYOUT)];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(d.label),
        layout: Some(d.layout),
        vertex: wgpu::VertexState {
            module: d.module,
            entry_point: Some(d.vertex),
            compilation_options: Default::default(),
            buffers: if d.quad { &buffers } else { &[] },
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: d.module,
            entry_point: Some(d.fragment),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: d.format,
                blend: d.blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

impl Pipelines {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Pipelines {
        let uniforms = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("kagari uniforms"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(UNIFORM_SIZE),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(TRANSFORMS_BYTES),
                    },
                    count: None,
                },
            ],
        });
        let tex1 = textures_layout(device, "kagari tex1", 1);
        let tex2 = textures_layout(device, "kagari tex2", 2);
        let tex3 = textures_layout(device, "kagari tex3", 3);
        let empty = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("kagari empty"),
            entries: &[],
        });
        let empty_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("kagari empty"),
            layout: &empty,
            entries: &[],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("kagari linear"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let layout = |label: &str, group1: &wgpu::BindGroupLayout| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[Some(&uniforms), Some(group1)],
                immediate_size: 0,
            })
        };
        let l1 = layout("kagari layout tex1", &tex1);
        let l2 = layout("kagari layout tex2", &tex2);
        let l3 = layout("kagari layout tex3", &tex3);

        let quad = module(
            device,
            "kagari quad.wgsl",
            concat!(
                include_str!("shaders/color.wgsl"),
                include_str!("shaders/quad.wgsl")
            ),
        );
        let mask_composite = module(
            device,
            "kagari mask_composite.wgsl",
            concat!(
                include_str!("shaders/color.wgsl"),
                include_str!("shaders/mask_composite.wgsl")
            ),
        );
        let blur = module(
            device,
            "kagari blur.wgsl",
            concat!(
                include_str!("shaders/color.wgsl"),
                include_str!("shaders/blur.wgsl")
            ),
        );
        let transition = module(
            device,
            "kagari transition.wgsl",
            concat!(
                include_str!("shaders/color.wgsl"),
                include_str!("shaders/transition.wgsl")
            ),
        );
        let present = module(
            device,
            "kagari present.wgsl",
            concat!(
                include_str!("shaders/color.wgsl"),
                include_str!("shaders/present.wgsl")
            ),
        );

        let make = |label, layout, module, fragment, blend| {
            pipeline(
                device,
                PipeDesc {
                    label,
                    layout,
                    module,
                    vertex: "vs_main",
                    fragment,
                    format: LINEAR_FORMAT,
                    blend,
                    quad: true,
                },
            )
        };
        let present_fragment = if target_format.is_srgb() {
            "fs_present_srgb"
        } else {
            "fs_present_encode"
        };
        let make_present = |label, fragment, format| {
            pipeline(
                device,
                PipeDesc {
                    label,
                    layout: &l1,
                    module: &present,
                    vertex: "vs_fullscreen",
                    fragment,
                    format,
                    blend: None,
                    quad: false,
                },
            )
        };
        Pipelines {
            image: make("kagari image", &l1, &quad, "fs_image", Some(PREMUL)),
            linear: make("kagari linear", &l1, &quad, "fs_linear", Some(PREMUL)),
            mask: make("kagari mask", &l1, &quad, "fs_mask", Some(PREMUL)),
            mask_max: make("kagari mask max", &l1, &quad, "fs_mask", Some(MAX)),
            mask_composite: make(
                "kagari mask composite",
                &l2,
                &mask_composite,
                "fs_mask_composite",
                Some(PREMUL),
            ),
            blur: make("kagari blur", &l1, &blur, "fs_blur", None),
            transition: make(
                "kagari transition",
                &l3,
                &transition,
                "fs_transition",
                Some(PREMUL),
            ),
            present: make_present("kagari present", present_fragment, target_format),
            resolve: make_present("kagari resolve", "fs_present_srgb", READBACK_FORMAT),
            user: user_layouts(device, &uniforms, &tex1, &tex2, &empty),
            uniforms,
            empty_bind,
            tex1,
            tex2,
            tex3,
            sampler,
        }
    }

    pub fn get(&self, pipe: Pipe) -> &wgpu::RenderPipeline {
        match pipe {
            Pipe::Image => &self.image,
            Pipe::Linear => &self.linear,
            Pipe::Mask => &self.mask,
            Pipe::MaskMax => &self.mask_max,
            Pipe::MaskComposite => &self.mask_composite,
            Pipe::Blur => &self.blur,
            Pipe::Transition => &self.transition,
            Pipe::Present => &self.present,
            Pipe::Resolve => &self.resolve,
        }
    }

    /// A bind group of `views` (in binding order) followed by the sampler.
    pub fn textures(&self, device: &wgpu::Device, views: &[&wgpu::TextureView]) -> wgpu::BindGroup {
        let layout = match views.len() {
            1 => &self.tex1,
            2 => &self.tex2,
            _ => &self.tex3,
        };
        let mut entries: Vec<wgpu::BindGroupEntry<'_>> =
            Vec::with_capacity(views.len().saturating_add(1));
        let mut binding = 0u32;
        for view in views {
            entries.push(wgpu::BindGroupEntry {
                binding,
                resource: wgpu::BindingResource::TextureView(view),
            });
            binding = binding.saturating_add(1);
        }
        entries.push(wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::Sampler(&self.sampler),
        });
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout,
            entries: &entries,
        })
    }
}

fn buffer_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    ty: wgpu::BufferBindingType,
    dynamic: bool,
    min: u64,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty,
            has_dynamic_offset: dynamic,
            min_binding_size: wgpu::BufferSize::new(min),
        },
        count: None,
    }
}

/// The user shader layouts, gated on the device: no storage buffers → no user shaders; no compute
/// → no motion shaders. Logged once.
fn user_layouts(
    device: &wgpu::Device,
    uniforms: &wgpu::BindGroupLayout,
    tex1: &wgpu::BindGroupLayout,
    tex2: &wgpu::BindGroupLayout,
    empty: &wgpu::BindGroupLayout,
) -> Option<UserLayouts> {
    let limits = device.limits();
    if limits.max_storage_buffers_per_shader_stage == 0 {
        log::warn!("the GPU has no storage buffers; shaders are not drawn");
        return None;
    }
    let args = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("kagari shader args"),
        entries: &[buffer_entry(
            0,
            wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE,
            wgpu::BufferBindingType::Storage { read_only: true },
            true,
            u64::from(MAX_ARGS_BYTES),
        )],
    });
    let layout = |label: &str, groups: &[&wgpu::BindGroupLayout]| {
        let groups: Vec<Option<&wgpu::BindGroupLayout>> =
            groups.iter().copied().map(Some).collect();
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(label),
            bind_group_layouts: &groups,
            immediate_size: 0,
        })
    };
    let motion = if limits.max_compute_workgroups_per_dimension == 0 {
        log::warn!("the GPU has no compute shaders; motion shaders are not drawn");
        None
    } else {
        let uniform = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("kagari motion uniform"),
            entries: &[buffer_entry(
                0,
                wgpu::ShaderStages::COMPUTE,
                wgpu::BufferBindingType::Uniform,
                true,
                MOTION_UNIFORM_SIZE,
            )],
        });
        let storage = wgpu::BufferBindingType::Storage { read_only: false };
        let buffers = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("kagari motion buffers"),
            entries: &[
                buffer_entry(0, wgpu::ShaderStages::COMPUTE, storage, false, 16),
                buffer_entry(1, wgpu::ShaderStages::COMPUTE, storage, false, 16),
            ],
        });
        Some(MotionLayouts {
            layout: layout("kagari layout motion", &[&uniform, &buffers, &args]),
            uniform,
            buffers,
        })
    };
    Some(UserLayouts {
        color: layout("kagari layout color shader", &[uniforms, tex1, &args]),
        mix: layout("kagari layout mix shader", &[uniforms, tex2, &args]),
        texture: layout("kagari layout texture shader", &[uniforms, tex1, &args]),
        mask: layout("kagari layout mask shader", &[uniforms, empty, &args]),
        motion,
        args,
    })
}
