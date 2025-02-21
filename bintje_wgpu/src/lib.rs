//! Rasterize Bintje's wide tile per-tile command lists using wgpu.
//!
//! The limits are low enough to, in principle, run on WebGL2.
//!
//! This currently hardcodes the texture size to 256x256 pixels.

use color::PremulRgba8;

/// Re-export pollster's `block_on` for convenience.
pub use pollster::block_on;

/// Targetting WebGL2.
const LIMITS: wgpu::Limits = wgpu::Limits::downlevel_webgl2_defaults();

pub struct RenderContext {
    #[expect(unused, reason = "might come in handy later")]
    instance: wgpu::Instance,
    #[expect(unused, reason = "might come in handy later")]
    adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl RenderContext {
    pub async fn create() -> Self {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = wgpu::util::initialize_adapter_from_env_or_default(&instance, None)
            .await
            .expect("would like to get an adapter");
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: LIMITS,
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .expect("failed to find a device");

        RenderContext {
            instance,
            adapter,
            device,
            queue,
        }
    }

    /// Create the actual rasterizer. Currently this only creates the shader required for
    /// rasterizing draw commands (fills with and without alpha masks).
    pub fn rasterizer(&mut self, width: u16, height: u16) -> Rasterizer {
        debug_assert!(width <= 256 && height <= 256);

        let draw_shader = self
            .device
            .create_shader_module(wgpu::include_wgsl!("draw_shader.wgsl"));

        let target_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                // width: width.into(),
                // height: height.into(),
                width: 256,
                height: 256,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let vertex_instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vertex instance buffer"),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            size: 2 << 16,
            mapped_at_creation: false,
        });
        let alpha_masks_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("alpha masks buffer"),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            size: LIMITS.max_uniform_buffer_binding_size as u64,
            mapped_at_creation: false,
        });
        let bind_group_layout =
            self.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: None,
                    entries: &[
                        // Alpha masks
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: Some(
                                    (LIMITS.max_uniform_buffer_binding_size as u64)
                                        .try_into()
                                        .unwrap(),
                                ),
                            },
                            count: None,
                        },
                    ],
                });

        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &draw_shader,
                    entry_point: Some("vs"),
                    buffers: &[DrawCmdVertexInstance::buffer_layout()],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &draw_shader,
                    entry_point: Some("fs"),
                    targets: &[Some(wgpu::ColorTargetState {
                        // We send non-linear sRGB8 to the shader, but let the shader pretend its
                        // linear sRGB.
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Cw,
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

        Rasterizer {
            device: self.device.clone(),
            queue: self.queue.clone(),
            pipeline,

            width,
            height,

            target_texture,
            texture_copy_buffer: TextureCopyBuffer::new(&self.device, width, height),

            bind_group_layout,
            vertex_instance_buffer,
            alpha_masks_buffer,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct DrawCmdVertexInstance {
    x: u16,
    y: u16,
    width: u16,
    alpha_idx: u16,
    color: PremulRgba8,
}

impl DrawCmdVertexInstance {
    fn buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Uint16,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<u16>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Uint16,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[u16; 2]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Uint16,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[u16; 3]>() as wgpu::BufferAddress,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Uint16,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[u16; 4]>() as wgpu::BufferAddress,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Uint32,
                },
            ],
        }
    }
}

pub struct Rasterizer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub pipeline: wgpu::RenderPipeline,

    width: u16,
    height: u16,

    target_texture: wgpu::Texture,
    texture_copy_buffer: TextureCopyBuffer,

    bind_group_layout: wgpu::BindGroupLayout,
    vertex_instance_buffer: wgpu::Buffer,
    alpha_masks_buffer: wgpu::Buffer,
}

/// A buffer to copy textures into from the GPU.
///
/// This pads internal buffer to adhere to the `bytes_per_row` size requirement of
/// [`wgpu::CommandEncoder::copy_texture_to_buffer`], see [`wgpu::TexelCopyBufferLayout`].
struct TextureCopyBuffer {
    buffer: wgpu::Buffer,
    bytes_per_row: u32,
}

impl TextureCopyBuffer {
    pub fn new(device: &wgpu::Device, width: u16, height: u16) -> Self {
        let bytes_per_row = ((width as u32) * 4).next_multiple_of(256);

        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("texture-out"),
            size: bytes_per_row as u64 * height as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        Self {
            buffer,
            bytes_per_row,
        }
    }
}

impl Rasterizer {
    fn submit(
        &self,
        mut encoder: wgpu::CommandEncoder,
        clear_texture: bool,
        instances: &mut Vec<DrawCmdVertexInstance>,
        alpha_masks: &mut Vec<u8>,
    ) {
        self.queue.write_buffer(
            &self.alpha_masks_buffer,
            0,
            bytemuck::cast_slice(alpha_masks),
        );
        self.queue.write_buffer(
            &self.vertex_instance_buffer,
            0,
            bytemuck::cast_slice(instances),
        );

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self
                        .target_texture
                        .create_view(&wgpu::TextureViewDescriptor::default()),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if clear_texture {
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.alpha_masks_buffer.as_entire_binding(),
                }],
            });

            render_pass.set_vertex_buffer(
                0,
                self.vertex_instance_buffer
                    .slice(0..(instances.len() * size_of::<DrawCmdVertexInstance>()) as u64),
            );
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.set_pipeline(&self.pipeline);
            render_pass.draw(0..4, 0..instances.len() as u32);
        }

        instances.clear();
        alpha_masks.clear();

        self.queue.submit([encoder.finish()]);
    }

    /// Rasterize the per-tile command lists and given alpha masks, and copy the resulting GPU
    /// texture to the destination image.
    ///
    /// Note: the texture size is currently hardcoded to 256x256 pixels.
    pub fn rasterize(
        &mut self,
        alpha_masks: &[u8],
        wide_tiles: &[bintje::WideTile],
        width: u16,
        dest_img: &mut [u8],
    ) {
        let wide_tiles_per_row = width.div_ceil(bintje::WideTile::WIDTH_PX);

        let mut instances = Vec::new();
        let mut alpha_masks_buffer = Vec::<u8>::new();
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        let mut submitted = false;
        for (idx, wide_tile) in wide_tiles.iter().enumerate() {
            let wide_tile_y = (idx / wide_tiles_per_row as usize) as u16;
            let wide_tile_x = (idx - (wide_tile_y as usize * wide_tiles_per_row as usize)) as u16;

            // TODO(Tom): this doesn't account for overflowing the vertex instance buffer (what are
            // the limits?)
            for command in &wide_tile.commands {
                match command {
                    bintje::Command::Sample(sample) => {
                        let alpha_mask_size = sample.width as usize
                            * bintje::Tile::WIDTH as usize
                            * bintje::Tile::HEIGHT as usize;
                        let alpha_idx = alpha_masks_buffer.len();
                        if alpha_idx + alpha_mask_size
                            > LIMITS.max_uniform_buffer_binding_size as usize
                        {
                            let encoder = std::mem::replace(
                                &mut encoder,
                                self.device.create_command_encoder(
                                    &wgpu::CommandEncoderDescriptor { label: None },
                                ),
                            );
                            self.submit(
                                encoder,
                                !submitted,
                                &mut instances,
                                &mut alpha_masks_buffer,
                            );
                            submitted = true;
                        }
                        alpha_masks_buffer.extend_from_slice(
                            &alpha_masks[sample.alpha_idx as usize
                                ..sample.alpha_idx as usize + alpha_mask_size],
                        );
                        instances.push(DrawCmdVertexInstance {
                            x: (wide_tile_x * bintje::WideTile::WIDTH_TILES + sample.x)
                                * bintje::Tile::WIDTH,
                            y: wide_tile_y * bintje::Tile::HEIGHT,
                            width: sample.width * bintje::Tile::WIDTH,
                            color: sample.color,
                            alpha_idx: alpha_idx as u16
                                / (bintje::Tile::WIDTH * bintje::Tile::HEIGHT),
                        });
                    }
                    bintje::Command::SparseFill(sparse_fill) => {
                        instances.push(DrawCmdVertexInstance {
                            x: (wide_tile_x * bintje::WideTile::WIDTH_TILES + sparse_fill.x)
                                * bintje::Tile::WIDTH,
                            y: wide_tile_y * bintje::Tile::HEIGHT,
                            width: sparse_fill.width * bintje::Tile::WIDTH,
                            color: sparse_fill.color,
                            alpha_idx: u16::MAX,
                        });
                    }
                    _ => {}
                }
            }
        }
        if !instances.is_empty() {
            self.submit(encoder, !submitted, &mut instances, &mut alpha_masks_buffer);
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.target_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.texture_copy_buffer.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    // Must be a multiple of 256 bytes.
                    bytes_per_row: Some(self.texture_copy_buffer.bytes_per_row),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: self.width.into(),
                height: self.height.into(),
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);

        self.texture_copy_buffer
            .buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                if result.is_err() {
                    panic!("failed to map texture for reading")
                }
            });

        self.device.poll(wgpu::Maintain::Wait);
        let mut img_idx = 0;
        for row in (self.texture_copy_buffer.buffer.slice(..).get_mapped_range())
            .chunks_exact(self.texture_copy_buffer.bytes_per_row as usize)
        {
            dest_img[img_idx..img_idx + self.width as usize * 4]
                .copy_from_slice(&row[0..self.width as usize * 4]);
            img_idx += self.width as usize * 4;
        }
        self.texture_copy_buffer.buffer.unmap();
    }
}
