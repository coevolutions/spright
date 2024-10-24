use std::sync::Arc;

use spright::Renderer;
use wgpu::{
    Adapter, CreateSurfaceError, Device, DeviceDescriptor, PresentMode, Queue, RenderPass, Surface,
    SurfaceConfiguration,
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{EventLoop, EventLoopProxy},
    window::Window,
};

struct Prepared {
    spright: spright::Prepared,
}

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

impl Inner {
    fn new(gfx: &Graphics) -> Self {
        let [w, h]: [u32; 2] = gfx.window.inner_size().into();
        let spright_renderer = Renderer::new(
            &gfx.device,
            gfx.surface.get_capabilities(&gfx.adapter).formats[0],
            [w as f32, h as f32],
        );
        Self {
            spright_renderer,
            texture1: spright::texture::load(
                &gfx.device,
                &gfx.queue,
                &image::load_from_memory(include_bytes!("test.png")).unwrap(),
            ),
            texture2: spright::texture::load(
                &gfx.device,
                &gfx.queue,
                &image::load_from_memory(include_bytes!("test2.png")).unwrap(),
            ),
        }
    }

    pub fn resize(&mut self, queue: &Queue, screen_size: winit::dpi::PhysicalSize<u32>) {
        self.spright_renderer
            .resize(queue, [screen_size.width as f32, screen_size.height as f32]);
    }

    pub fn prepare(&self, device: &Device) -> Prepared {
        Prepared {
            spright: self.spright_renderer.prepare(
                device,
                &[
                    spright::Group {
                        texture: &self.texture1,
                        texture_kind: spright::TextureKind::Color,
                        sprites: &[
                            spright::Sprite {
                                src: spright::Rect {
                                    x: 0.0,
                                    y: 0.0,
                                    width: 280.0 / 2.0,
                                    height: 210.0 / 2.0,
                                },
                                dest_size: spright::Size {
                                    width: 280.0,
                                    height: 210.0,
                                },
                                transform: spright::AffineTransform::identity(),
                                tint: spright::Color::new(0xff, 0xff, 0xff, 0xff),
                            },
                            spright::Sprite {
                                src: spright::Rect {
                                    x: 0.0,
                                    y: 0.0,
                                    width: 280.0,
                                    height: 210.0,
                                },
                                dest_size: spright::Size {
                                    width: 280.0,
                                    height: 210.0,
                                },
                                transform: spright::AffineTransform::identity(),
                                tint: spright::Color::new(0xff, 0xff, 0xff, 0xff),
                            },
                        ],
                    },
                    spright::Group {
                        texture: &self.texture2,
                        texture_kind: spright::TextureKind::Color,
                        sprites: &[spright::Sprite {
                            src: spright::Rect {
                                x: 0.0,
                                y: 0.0,
                                width: 386.0,
                                height: 395.0,
                            },
                            dest_size: spright::Size {
                                width: 386.0,
                                height: 395.0,
                            },
                            transform: spright::AffineTransform::translation(200.0, 0.0)
                                * spright::AffineTransform::scaling(2.0, 3.0),
                            tint: spright::Color::new(0xff, 0xff, 0xff, 0xff),
                        }],
                    },
                    spright::Group {
                        texture: &self.texture1,
                        texture_kind: spright::TextureKind::Color,
                        sprites: &[spright::Sprite {
                            src: spright::Rect {
                                x: 0.0,
                                y: 0.0,
                                width: 280.0,
                                height: 210.0,
                            },
                            dest_size: spright::Size {
                                width: 280.0,
                                height: 210.0,
                            },
                            transform: spright::AffineTransform::translation(-140.0, -105.0)
                                * spright::AffineTransform::scaling(3.0, 3.0)
                                * spright::AffineTransform::rotation(1.0)
                                * spright::AffineTransform::translation(140.0 * 3.0, 105.0 * 3.0),
                            tint: spright::Color::new(0xff, 0xff, 0x00, 0x88),
                        }],
                    },
                ],
            ),
        }
    }

    pub fn render<'rpass>(&'rpass self, rpass: &mut RenderPass<'rpass>, prepared: &Prepared) {
        self.spright_renderer.render(rpass, &prepared.spright);
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
                required_features: wgpu::Features::default()
                    | wgpu::Features::TEXTURE_BINDING_ARRAY,
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
            WindowEvent::Resized(new_size) => {
                let Some(gfx) = &mut self.gfx else {
                    return;
                };
                gfx.resize(new_size);

                let Some(inner) = &mut self.inner else {
                    return;
                };
                inner.resize(&gfx.queue, new_size);
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

                let prepared = inner.prepare(&gfx.device);
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
                    inner.render(&mut rpass, &prepared);
                }

                gfx.queue.submit(Some(encoder.finish()));
                frame.present();
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
