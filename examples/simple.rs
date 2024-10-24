use std::sync::Arc;

use image::GenericImageView;
use spright::Renderer;
use wgpu::{
    util::DeviceExt, Adapter, CreateSurfaceError, Device, DeviceDescriptor, PresentMode, Queue,
    RenderPass, Surface, SurfaceConfiguration,
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{EventLoop, EventLoopProxy},
    window::Window,
};

enum UserEvent {
    Graphics(Graphics),
}

struct Graphics {
    window: Arc<Window>,
    surface: Surface<'static>,
    surface_config: SurfaceConfiguration,
    device: Device,
    adapter: Adapter,
    queue: Queue,
}

impl Graphics {
    fn resize(&mut self, size: PhysicalSize<u32>) {
        self.surface_config.width = size.width.max(1);
        self.surface_config.height = size.height.max(1);
        self.surface.configure(&self.device, &self.surface_config);
        self.window.request_redraw();
    }
}

struct Inner {
    spright_renderer: Renderer,
    texture1: wgpu::Texture,
    texture2: wgpu::Texture,
}

fn load_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    img: &image::DynamicImage,
) -> wgpu::Texture {
    let (width, height) = img.dimensions();

    device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::default(),
        &img.to_rgba8(),
    )
}

impl Inner {
    fn new(gfx: &Graphics) -> Self {
        let spright_renderer = Renderer::new(
            &gfx.device,
            gfx.surface.get_capabilities(&gfx.adapter).formats[0],
        );
        Self {
            spright_renderer,
            texture1: load_texture(
                &gfx.device,
                &gfx.queue,
                &image::load_from_memory(include_bytes!("test.png")).unwrap(),
            ),
            texture2: load_texture(
                &gfx.device,
                &gfx.queue,
                &image::load_from_memory(include_bytes!("test2.png")).unwrap(),
            ),
        }
    }

    pub fn prepare(&mut self, device: &Device, queue: &Queue, target_size: wgpu::Extent3d) {
        self.spright_renderer.prepare(
            device,
            queue,
            target_size,
            &[
                spright::Sprite {
                    texture: &self.texture1,
                    src: spright::Rect::new(0, 0, 280 / 2, 210 / 2),
                    transform: glam::Affine2::IDENTITY,
                    tint: spright::Color::new(0xff, 0xff, 0xff, 0xff),
                },
                spright::Sprite {
                    texture: &self.texture1,
                    src: spright::Rect::new(0, 0, 280, 210),
                    transform: glam::Affine2::from_translation(glam::vec2(100.0, 100.0)),
                    tint: spright::Color::new(0xff, 0xff, 0xff, 0xff),
                },
                spright::Sprite {
                    texture: &self.texture2,
                    src: spright::Rect::new(0, 0, 386, 395),
                    transform: glam::Affine2::from_scale(glam::Vec2::new(2.0, 3.0))
                        * glam::Affine2::from_translation(glam::Vec2::new(200.0, 0.0)),
                    tint: spright::Color::new(0xff, 0xff, 0xff, 0xff),
                },
                spright::Sprite {
                    texture: &self.texture1,
                    src: spright::Rect::new(0, 0, 280, 210),
                    transform: glam::Affine2::from_translation(glam::Vec2::new(
                        140.0 * 3.0,
                        105.0 * 3.0,
                    )) * glam::Affine2::from_angle(1.0)
                        * glam::Affine2::from_scale(glam::Vec2::new(3.0, 3.0))
                        * glam::Affine2::from_translation(glam::Vec2::new(-140.0, -105.0)),
                    tint: spright::Color::new(0xff, 0xff, 0x00, 0x88),
                },
            ],
        );
    }

    pub fn render<'rpass>(&'rpass self, rpass: &mut RenderPass<'rpass>) {
        self.spright_renderer.render(rpass);
    }
}

struct Application {
    event_loop_proxy: EventLoopProxy<UserEvent>,
    gfx: Option<Graphics>,
    inner: Option<Inner>,
}

async fn create_graphics(window: Arc<Window>) -> Result<Graphics, CreateSurfaceError> {
    let instance = wgpu::Instance::default();

    let surface = instance.create_surface(window.clone())?;
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        })
        .await
        .expect("Failed to find an appropriate adapter");

    let (device, queue) = adapter
        .request_device(
            &DeviceDescriptor {
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                required_features: wgpu::Features::default(),
                ..Default::default()
            },
            None,
        )
        .await
        .expect("Failed to create device");

    let mut size = window.inner_size();
    size.width = size.width.max(1);
    size.height = size.height.max(1);

    let mut config = surface
        .get_default_config(&adapter, size.width, size.height)
        .unwrap();
    config.present_mode = PresentMode::AutoVsync;
    surface.configure(&device, &config);

    Ok(Graphics {
        window,
        surface,
        surface_config: config,
        adapter,
        device,
        queue,
    })
}

impl ApplicationHandler<UserEvent> for Application {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        let window_attrs = Window::default_attributes();

        let window = event_loop
            .create_window(window_attrs)
            .expect("failed to create window");

        let event_loop_proxy = self.event_loop_proxy.clone();
        let fut = async move {
            assert!(event_loop_proxy
                .send_event(UserEvent::Graphics(
                    create_graphics(Arc::new(window))
                        .await
                        .expect("failed to create graphics context")
                ))
                .is_ok());
        };

        pollster::block_on(fut);
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::Resized(size) => {
                let Some(gfx) = &mut self.gfx else {
                    return;
                };
                gfx.resize(size);
            }
            WindowEvent::RedrawRequested => {
                let Some(gfx) = &mut self.gfx else {
                    return;
                };

                let Some(inner) = &mut self.inner else {
                    return;
                };

                let frame = gfx
                    .surface
                    .get_current_texture()
                    .expect("Failed to acquire next swap chain texture");
                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder = gfx
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                inner.prepare(&gfx.device, &gfx.queue, frame.texture.size());
                {
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        ..Default::default()
                    });
                    inner.render(&mut rpass);
                }

                gfx.queue.submit(Some(encoder.finish()));
                gfx.window.pre_present_notify();
                frame.present();
                gfx.window.request_redraw();
            }
            WindowEvent::CloseRequested => event_loop.exit(),
            _ => {}
        };
    }

    fn user_event(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Graphics(mut gfx) => {
                gfx.resize(gfx.window.inner_size());
                let inner = Inner::new(&gfx);
                self.inner = Some(inner);
                self.gfx = Some(gfx);
            }
        }
    }
}

fn main() {
    let event_loop = EventLoop::with_user_event().build().unwrap();
    let mut app = Application {
        gfx: None,
        inner: None,
        event_loop_proxy: event_loop.create_proxy(),
    };
    event_loop.run_app(&mut app).unwrap();
}
