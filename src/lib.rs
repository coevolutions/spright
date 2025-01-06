use encase::{DynamicUniformBuffer, ShaderSize, ShaderType, UniformBuffer};
use glam::*;

pub type Color = rgb::RGBA8;

/// Represents a group of sprites to draw from the same texture.
#[derive(Debug, Clone)]
pub struct Group<'a> {
    /// Texture to draw with.
    pub texture: &'a wgpu::Texture,

    /// Items in the group.
    pub items: Vec<Item>,
}

/// Represents a sprite to draw.
#[derive(Debug, Clone)]
pub struct Item {
    /// Source offset from the texture.
    pub src_offset: IVec2,

    /// Source size.
    pub src_size: UVec2,

    /// Source layer.
    pub src_layer: u32,

    /// Target transform.
    pub transform: Affine2,

    /// Tint.
    pub tint: Color,
}

/// Encapsulates static state for rendering.
pub struct Renderer {
    render_pipeline: wgpu::RenderPipeline,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    target_uniforms_buffer: wgpu::Buffer,
    target_uniforms_bind_group: wgpu::BindGroup,
    texture_uniforms_buffer: DynamicBuffer,
    prepared_groups: Vec<PreparedGroup>,
    vertex_buffer: DynamicBuffer,
    index_buffer: DynamicBuffer,
    sampler: wgpu::Sampler,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    tex_coords: [f32; 2],
    layer: u32,
    tint: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Debug, ShaderType)]
struct TextureUniforms {
    size: Vec3,
    is_mask: u32,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, ShaderType)]
struct TargetUniforms {
    size: Vec3,
}

impl Vertex {
    const BUFFER_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2=> Uint32, 3 => Float32x4],
    };
}

struct DynamicBuffer {
    inner: wgpu::Buffer,
    label: Option<String>,
}

impl DynamicBuffer {
    fn new(device: &wgpu::Device, desc: &wgpu::BufferDescriptor) -> Self {
        Self {
            inner: device.create_buffer(desc),
            label: desc.label.map(|v| v.to_string()),
        }
    }

    fn reallocate(&mut self, device: &wgpu::Device, size: wgpu::BufferAddress) -> wgpu::Buffer {
        let mut old = device.create_buffer(&wgpu::BufferDescriptor {
            label: self.label.as_ref().map(|v| v.as_str()),
            size,
            usage: self.inner.usage(),
            mapped_at_creation: true,
        });
        std::mem::swap(&mut old, &mut self.inner);
        old
    }

    fn write(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &[u8]) {
        let size = data.len() as u64;
        if self.inner.size() < size {
            self.reallocate(device, size);
            {
                let mut view = self.inner.slice(..).get_mapped_range_mut();
                view.copy_from_slice(data);
            }
            self.inner.unmap();
        } else {
            queue.write_buffer(&self.inner, 0, data);
        }
    }
}

impl std::ops::Deref for DynamicBuffer {
    type Target = wgpu::Buffer;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

struct PreparedGroup {
    texture_bind_group: wgpu::BindGroup,
    index_buffer_start: u32,
    index_buffer_end: u32,
}

impl Renderer {
    /// Creates a new renderer.
    pub fn new(device: &wgpu::Device, texture_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("spright: texture_bind_group_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let target_uniforms_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("spright: target_uniforms_bind_group_layout"),
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

        let texture_uniforms_buffer = DynamicBuffer::new(
            &device,
            &wgpu::BufferDescriptor {
                label: Some("spright: texture_uniforms_buffer"),
                size: TextureUniforms::SHADER_SIZE.into(),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        );

        let target_uniforms_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("spright: target_uniforms_buffer"),
            size: TargetUniforms::SHADER_SIZE.into(),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let target_uniforms_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("spright: target_uniforms_bind_group"),
            layout: &target_uniforms_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: target_uniforms_buffer.as_entire_binding(),
            }],
        });

        let vertex_buffer = DynamicBuffer::new(
            &device,
            &wgpu::BufferDescriptor {
                label: Some("spright: vertex_buffer"),
                size: std::mem::size_of::<Vertex>() as u64 * 1024,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        );

        let index_buffer = DynamicBuffer::new(
            &device,
            &wgpu::BufferDescriptor {
                label: Some("spright: vertex_buffer"),
                size: std::mem::size_of::<u32>() as u64 * 1024,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        );

        Self {
            render_pipeline: device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("spright: render_pipeline"),
                cache: None,
                layout: Some(
                    &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("spright: render_pipeline.layout"),
                        bind_group_layouts: &[
                            &texture_bind_group_layout,
                            &target_uniforms_bind_group_layout,
                        ],
                        push_constant_ranges: &[],
                    }),
                ),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Vertex::BUFFER_LAYOUT],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: texture_format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::all(),
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
            }),
            texture_bind_group_layout,
            target_uniforms_buffer,
            target_uniforms_bind_group,
            texture_uniforms_buffer,
            vertex_buffer,
            index_buffer,
            prepared_groups: vec![],
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                mipmap_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            }),
        }
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_size: wgpu::Extent3d,
        groups: &[Group<'_>],
    ) {
        queue.write_buffer(&self.target_uniforms_buffer, 0, &{
            let mut buffer = UniformBuffer::new(vec![]);
            buffer
                .write(&TargetUniforms {
                    size: Vec3 {
                        x: target_size.width as f32,
                        y: target_size.height as f32,
                        z: 0.0,
                    },
                })
                .unwrap();
            buffer.into_inner()
        });

        self.prepared_groups.clear();

        let min_uniform_buffer_offset_alignment =
            device.limits().min_uniform_buffer_offset_alignment;

        let mut texture_uniforms_buffer = DynamicUniformBuffer::new_with_alignment(
            vec![],
            min_uniform_buffer_offset_alignment as u64,
        );

        for group in groups {
            texture_uniforms_buffer
                .write(&TextureUniforms {
                    size: Vec3 {
                        x: group.texture.width() as f32,
                        y: group.texture.height() as f32,
                        z: 0.0,
                    },
                    is_mask: (group.texture.format() == wgpu::TextureFormat::R8Unorm) as u32,
                })
                .unwrap();
        }

        self.texture_uniforms_buffer
            .write(device, queue, &texture_uniforms_buffer.into_inner());

        let mut vertices = vec![];
        let mut indices = vec![];

        for (i, group) in groups.into_iter().enumerate() {
            let index_buffer_start = indices.len() as u32;

            for item in group.items.iter() {
                let offset = vertices.len() as u32;

                let tint = [
                    item.tint.r as f32 / 255.0,
                    item.tint.g as f32 / 255.0,
                    item.tint.b as f32 / 255.0,
                    item.tint.a as f32 / 255.0,
                ];

                let left = item.src_offset.x;
                let top = item.src_offset.y;
                let right = item.src_offset.x + item.src_size.x as i32;
                let bottom = item.src_offset.y + item.src_size.y as i32;

                vertices.extend([
                    Vertex {
                        position: item
                            .transform
                            .transform_point2(Vec2::new(0.0, 0.0))
                            .extend(0.0)
                            .to_array(),
                        tex_coords: [left as f32, top as f32],
                        layer: item.src_layer,
                        tint,
                    },
                    Vertex {
                        position: item
                            .transform
                            .transform_point2(Vec2::new(0.0, item.src_size.y as f32))
                            .extend(0.0)
                            .to_array(),
                        tex_coords: [left as f32, bottom as f32],
                        layer: item.src_layer,
                        tint,
                    },
                    Vertex {
                        position: item
                            .transform
                            .transform_point2(Vec2::new(item.src_size.x as f32, 0.0))
                            .extend(0.0)
                            .to_array(),
                        tex_coords: [right as f32, top as f32],
                        layer: item.src_layer,
                        tint,
                    },
                    Vertex {
                        position: item
                            .transform
                            .transform_point2(Vec2::new(
                                item.src_size.x as f32,
                                item.src_size.y as f32,
                            ))
                            .extend(0.0)
                            .to_array(),
                        tex_coords: [right as f32, bottom as f32],
                        layer: item.src_layer,
                        tint,
                    },
                ]);

                indices.extend(
                    [
                        0, 1, 2, //
                        1, 2, 3,
                    ]
                    .map(|v| v + offset),
                );
            }

            self.prepared_groups.push(PreparedGroup {
                texture_bind_group: device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("spright: texture_bind_group"),
                    layout: &self.texture_bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(
                                &group.texture.create_view(&wgpu::TextureViewDescriptor {
                                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                                    ..Default::default()
                                }),
                            ),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &self.texture_uniforms_buffer,
                                offset: (i * min_uniform_buffer_offset_alignment as usize) as u64,
                                size: Some(TextureUniforms::SHADER_SIZE),
                            }),
                        },
                    ],
                }),
                index_buffer_start,
                index_buffer_end: indices.len() as u32,
            });
        }

        self.vertex_buffer
            .write(device, queue, bytemuck::cast_slice(&vertices[..]));
        self.index_buffer
            .write(device, queue, bytemuck::cast_slice(&indices[..]));
    }

    /// Renders prepared sprites.
    pub fn render<'rpass>(&'rpass self, rpass: &mut wgpu::RenderPass<'rpass>) {
        rpass.set_pipeline(&self.render_pipeline);
        rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        rpass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        rpass.set_bind_group(1, &self.target_uniforms_bind_group, &[]);
        for prepared_group in self.prepared_groups.iter() {
            rpass.set_bind_group(0, &prepared_group.texture_bind_group, &[]);
            rpass.draw_indexed(
                prepared_group.index_buffer_start..prepared_group.index_buffer_end,
                0,
                0..1,
            );
        }
    }
}
