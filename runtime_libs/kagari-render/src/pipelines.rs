//! Bind group layouts, the sampler, the vertex and uniform formats, and every render pipeline.

use bytemuck::{Pod, Zeroable};

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

const VERTEX_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
    array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
    step_mode: wgpu::VertexStepMode::Vertex,
    attributes: &VERTEX_ATTRIBUTES,
};

/// The uniforms of one draw (`Uniforms` in `color.wgsl`): the target size, then the per-draw
/// parameters `a`, `b`, `c` and the blur weights.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(crate) struct Uniforms {
    pub target_size: [f32; 2],
    pub pad0: [f32; 2],
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
    /// As `Linear`, added.
    LinearAdd,
    /// Coverage times the vertex colour; premultiplied over.
    Mask,
    /// As `Mask`, per-channel maximum.
    MaskMax,
    /// A blend mode over the backdrop copy; replaces.
    Blend,
    /// One Gaussian blur pass; replaces.
    Blur,
    /// A transition between two layers; premultiplied over.
    Transition,
    /// The design target onto the surface.
    Present,
    /// The design target into the readback texture.
    Resolve,
}

/// The layouts, the sampler and the pipelines shared by every frame.
pub(crate) struct Pipelines {
    pub uniforms: wgpu::BindGroupLayout,
    /// One texture plus the sampler.
    pub tex1: wgpu::BindGroupLayout,
    /// Two textures plus the sampler.
    pub tex2: wgpu::BindGroupLayout,
    /// Three textures plus the sampler.
    pub tex3: wgpu::BindGroupLayout,
    pub sampler: wgpu::Sampler,
    image: wgpu::RenderPipeline,
    linear: wgpu::RenderPipeline,
    linear_add: wgpu::RenderPipeline,
    mask: wgpu::RenderPipeline,
    mask_max: wgpu::RenderPipeline,
    blend: wgpu::RenderPipeline,
    blur: wgpu::RenderPipeline,
    transition: wgpu::RenderPipeline,
    present: wgpu::RenderPipeline,
    resolve: wgpu::RenderPipeline,
}

const PREMUL: wgpu::BlendState = wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING;

const ADD: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    },
};

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

struct PipeDesc<'a> {
    label: &'a str,
    layout: &'a wgpu::PipelineLayout,
    module: &'a wgpu::ShaderModule,
    vertex: &'a str,
    fragment: &'a str,
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
    quad: bool,
}

fn pipeline(device: &wgpu::Device, d: PipeDesc<'_>) -> wgpu::RenderPipeline {
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
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(UNIFORM_SIZE),
                },
                count: None,
            }],
        });
        let tex1 = textures_layout(device, "kagari tex1", 1);
        let tex2 = textures_layout(device, "kagari tex2", 2);
        let tex3 = textures_layout(device, "kagari tex3", 3);
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
            concat!(include_str!("shaders/color.wgsl"), include_str!("shaders/quad.wgsl")),
        );
        let blend = module(
            device,
            "kagari blend.wgsl",
            concat!(include_str!("shaders/color.wgsl"), include_str!("shaders/blend.wgsl")),
        );
        let blur = module(
            device,
            "kagari blur.wgsl",
            concat!(include_str!("shaders/color.wgsl"), include_str!("shaders/blur.wgsl")),
        );
        let transition = module(
            device,
            "kagari transition.wgsl",
            concat!(include_str!("shaders/color.wgsl"), include_str!("shaders/transition.wgsl")),
        );
        let present = module(
            device,
            "kagari present.wgsl",
            concat!(include_str!("shaders/color.wgsl"), include_str!("shaders/present.wgsl")),
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
            linear_add: make("kagari linear add", &l1, &quad, "fs_linear", Some(ADD)),
            mask: make("kagari mask", &l1, &quad, "fs_mask", Some(PREMUL)),
            mask_max: make("kagari mask max", &l1, &quad, "fs_mask", Some(MAX)),
            blend: make("kagari blend", &l2, &blend, "fs_blend", None),
            blur: make("kagari blur", &l1, &blur, "fs_blur", None),
            transition: make("kagari transition", &l3, &transition, "fs_transition", Some(PREMUL)),
            present: make_present("kagari present", present_fragment, target_format),
            resolve: make_present("kagari resolve", "fs_present_srgb", READBACK_FORMAT),
            uniforms,
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
            Pipe::LinearAdd => &self.linear_add,
            Pipe::Mask => &self.mask,
            Pipe::MaskMax => &self.mask_max,
            Pipe::Blend => &self.blend,
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
        let mut entries: Vec<wgpu::BindGroupEntry<'_>> = Vec::with_capacity(views.len().saturating_add(1));
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
