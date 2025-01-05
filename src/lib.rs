use encase::{DynamicUniformBuffer, ShaderSize, ShaderType, UniformBuffer};
use glam::*;
use itertools::Itertools as _;

pub type Color = rgb::RGBA8;

#[derive(Debug, Clone, Copy)]
struct Rect {
    offset: IVec2,
    size: UVec2,
}

impl Rect {
    fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            offset: IVec2::new(x, y),
            size: UVec2::new(width, height),
        }
    }

    const fn left(&self) -> i32 {
        self.offset.x
    }

    const fn top(&self) -> i32 {
        self.offset.y
    }

    const fn right(&self) -> i32 {
        self.offset.x + self.size.x as i32
    }

    const fn bottom(&self) -> i32 {
        self.offset.y + self.size.y as i32
    }
}

/// Represents a slice of a texture to draw.
#[derive(Debug, Clone, Copy)]
pub struct TextureSlice<'a> {
    texture: &'a wgpu::Texture,
    layer: u32,
    rect: Rect,
}

impl<'a> TextureSlice<'a> {
    /// Creates a new texture slice from a raw texture.
    pub fn from_layer(texture: &'a wgpu::Texture, layer: u32) -> Option<Self> {
        let size = texture.size();
        if layer >= size.depth_or_array_layers {
            return None;
        }

        Some(Self {
            texture,
            layer,
            rect: Rect::new(0, 0, size.width, size.height),
        })
    }

    /// Slices the texture slice.
    ///
    /// Note that `offset` represents an offset into the slice and not into the overall texture -- the returned slice's offset will be the current offset + new offset.
    ///
    /// Returns [`None`] if the slice goes out of bounds.
    pub fn slice(&self, offset: glam::IVec2, size: glam::UVec2) -> Option<Self> {
        let rect = Rect {
            offset: self.rect.offset + offset,
            size,
        };

        if rect.left() < self.rect.left()
            || rect.right() > self.rect.right()
            || rect.top() < self.rect.top()
            || rect.bottom() > self.rect.bottom()
        {
            return None;
        }

        Some(Self {
            texture: self.texture,
            layer: self.layer,
            rect,
        })
    }

    /// Gets the size of the texture slice.
    pub fn size(&self) -> glam::UVec2 {
        self.rect.size
    }
}

/// Represents a sprite to draw.
#[derive(Debug, Clone)]
pub struct Sprite<'a> {
    /// The slice of texture to draw from.
    pub slice: TextureSlice<'a>,

    /// Transformation of the source rectangle into screen space.
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
        sprites: &[Sprite<'_>],
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

        let grouped = sprites
            .iter()
            .chunk_by(|s| s.slice.texture)
            .into_iter()
            .map(|(_, chunk)| chunk.collect::<Vec<_>>())
            .collect::<Vec<_>>();

        let mut texture_uniforms_buffer = DynamicUniformBuffer::new_with_alignment(
            vec![],
            min_uniform_buffer_offset_alignment as u64,
        );
        for sprites in grouped.iter() {
            let texture = sprites.first().unwrap().slice.texture;

            texture_uniforms_buffer
                .write(&TextureUniforms {
                    size: Vec3 {
                        x: texture.width() as f32,
                        y: texture.height() as f32,
                        z: 0.0,
                    },
                    is_mask: (texture.format() == wgpu::TextureFormat::R8Unorm) as u32,
                })
                .unwrap();
        }
        self.texture_uniforms_buffer
            .write(device, queue, &texture_uniforms_buffer.into_inner());

        let mut vertices = vec![];
        let mut indices = vec![];

        for (i, sprites) in grouped.into_iter().enumerate() {
            let texture = sprites.first().unwrap().slice.texture;

            let index_buffer_start = indices.len() as u32;

            for s in sprites {
                let offset = vertices.len() as u32;

                let tint = [
                    s.tint.r as f32 / 255.0,
                    s.tint.g as f32 / 255.0,
                    s.tint.b as f32 / 255.0,
                    s.tint.a as f32 / 255.0,
                ];

                vertices.extend([
                    Vertex {
                        position: s
                            .transform
                            .transform_point2(Vec2::new(0.0, 0.0))
                            .extend(0.0)
                            .to_array(),
                        tex_coords: [s.slice.rect.left() as f32, s.slice.rect.top() as f32],
                        layer: s.slice.layer,
                        tint,
                    },
                    Vertex {
                        position: s
                            .transform
                            .transform_point2(Vec2::new(0.0, s.slice.rect.size.y as f32))
                            .extend(0.0)
                            .to_array(),
                        tex_coords: [s.slice.rect.left() as f32, s.slice.rect.bottom() as f32],
                        layer: s.slice.layer,
                        tint,
                    },
                    Vertex {
                        position: s
                            .transform
                            .transform_point2(Vec2::new(s.slice.rect.size.x as f32, 0.0))
                            .extend(0.0)
                            .to_array(),
                        tex_coords: [s.slice.rect.right() as f32, s.slice.rect.top() as f32],
                        layer: s.slice.layer,
                        tint,
                    },
                    Vertex {
                        position: s
                            .transform
                            .transform_point2(Vec2::new(
                                s.slice.rect.size.x as f32,
                                s.slice.rect.size.y as f32,
                            ))
                            .extend(0.0)
                            .to_array(),
                        tex_coords: [s.slice.rect.right() as f32, s.slice.rect.bottom() as f32],
                        layer: s.slice.layer,
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
                            resource: wgpu::BindingResource::TextureView(&texture.create_view(
                                &wgpu::TextureViewDescriptor {
                                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                                    ..Default::default()
                                },
                            )),
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
