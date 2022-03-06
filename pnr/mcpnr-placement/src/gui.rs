use std::marker::PhantomData;
use std::time::Instant;

use anyhow::{Context, Result};
use egui::FullOutput;
use egui_wgpu_backend::{RenderPass as EGUIRenderPass, ScreenDescriptor};
use egui_winit_platform::Platform;
use log::{error, warn};
use winit::event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Window, WindowBuilder};

use crate::Config;

struct FrameContext {
    output: wgpu::SurfaceTexture,
    output_view: wgpu::TextureView,
    encoder: wgpu::CommandEncoder,
}

struct RenderState<'window> {
    surface: wgpu::Surface,

    // Ensures that the Window the surface is attached to lives long enough
    surface_window_phantom: PhantomData<&'window ()>,

    surface_config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    size: winit::dpi::PhysicalSize<u32>,
    scale_factor: f64,

    egui_render_pass: EGUIRenderPass,
}

impl<'window> RenderState<'window> {
    async fn new<'win>(window: &'win Window) -> Result<RenderState<'win>> {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::Backends::all());
        // The surface_window_phantom will ensure the Window lives long enough.
        let surface = unsafe { instance.create_surface(window) };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .context("Request adapter")?;

        // TODO: wire up trace path to the command line or an env variable
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .context("Request device")?;

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface
                .get_preferred_format(&adapter)
                .context("Get surface preferred format")?,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
        };

        surface.configure(&device, &surface_config);

        let egui_render_pass = EGUIRenderPass::new(&device, surface_config.format, 1);

        Ok(RenderState {
            surface,
            surface_window_phantom: PhantomData::default(),
            surface_config,
            device,
            queue,
            size,
            scale_factor: window.scale_factor(),

            egui_render_pass,
        })
    }

    pub fn resize(&mut self, new_scale_factor: f64, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.scale_factor = new_scale_factor;
            self.surface_config.width = new_size.width;
            self.surface_config.height = new_size.height;
            self.surface.configure(&self.device, &self.surface_config);
        }
    }

    pub fn update(&mut self) {}

    pub fn prepare_frame(&mut self) -> Result<FrameContext> {
        let output = self
            .surface
            .get_current_texture()
            .context("Get output texture")?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render encoder"),
            });

        {
            let _clear_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Pass"),
                color_attachments: &[wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        }),
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });
        }

        Ok(FrameContext {
            output,
            encoder,
            output_view: view,
        })
    }

    pub fn submit_frame(
        &mut self,
        mut frame: FrameContext,
        egui_platform: &Platform,
        egui_output: FullOutput,
    ) -> Result<()> {
        let paint_jobs = egui_platform.context().tessellate(egui_output.shapes);

        let screen_descriptor = ScreenDescriptor {
            physical_width: self.size.width,
            physical_height: self.size.height,
            scale_factor: self.scale_factor as f32,
        };

        self.egui_render_pass
            .add_textures(&self.device, &self.queue, &egui_output.textures_delta)
            .context("Failed to add textures")?;
        self.egui_render_pass.update_buffers(
            &self.device,
            &self.queue,
            &paint_jobs,
            &screen_descriptor,
        );

        self.egui_render_pass
            .execute(
                &mut frame.encoder,
                &frame.output_view,
                &paint_jobs,
                &screen_descriptor,
                None,
            )
            .context("execute EGUI render pass")?;

        self.egui_render_pass
            .remove_textures(egui_output.textures_delta);

        self.queue.submit(std::iter::once(frame.encoder.finish()));
        frame.output.present();

        Ok(())
    }
}

#[derive(Default)]
struct UIState {
    do_debug_render: bool,
}

fn render_ui(state: &mut UIState, ctx: &egui::Context, ui: &mut egui::Ui) {
    ui.collapsing("EGUI inspection", |ui| {
        ui.checkbox(&mut state.do_debug_render, "Do debug rendering");
        ctx.set_debug_on_hover(state.do_debug_render);
        ctx.inspection_ui(ui);
    });
}

pub(crate) fn run_gui(config: &Config) -> Result<()> {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new().build(&event_loop).unwrap();
    // We need to leak here because the winit event loop's run
    // requires a static reference.
    let window: &'static Window = Box::leak(Box::new(window));

    let mut state = pollster::block_on(RenderState::new(window))?;

    let size = window.inner_size();
    let size = size.to_logical(window.scale_factor());
    let mut platform = Platform::new(egui_winit_platform::PlatformDescriptor {
        physical_width: size.width,
        physical_height: size.height,
        scale_factor: window.scale_factor(),
        font_definitions: Default::default(),
        style: Default::default(),
    });

    let start_time = Instant::now();

    let mut ui_state = UIState::default();

    event_loop.run(move |event, _, control_flow| {
        platform.handle_event(&event);

        match event {
            Event::WindowEvent {
                ref event,
                window_id,
            } if window_id == window.id() => match event {
                WindowEvent::CloseRequested
                | WindowEvent::KeyboardInput {
                    input:
                        KeyboardInput {
                            state: ElementState::Pressed,
                            virtual_keycode: Some(VirtualKeyCode::Escape),
                            ..
                        },
                    ..
                } => *control_flow = ControlFlow::Exit,

                WindowEvent::Resized(physical_size) => {
                    state.resize(window.scale_factor(), *physical_size);
                }
                WindowEvent::ScaleFactorChanged {
                    new_inner_size,
                    scale_factor,
                } => {
                    state.resize(*scale_factor, **new_inner_size);
                }

                _ => {}
            },

            Event::RedrawRequested(window_id) if window_id == window.id() => {
                platform.update_time(start_time.elapsed().as_secs_f64());

                state.update();
                let frame = match state.prepare_frame() {
                    Ok(f) => f,
                    Err(e) => {
                        match e.downcast_ref::<wgpu::SurfaceError>() {
                            Some(wgpu::SurfaceError::Lost) => {
                                state.resize(state.scale_factor, state.size)
                            }
                            Some(wgpu::SurfaceError::OutOfMemory) => {
                                *control_flow = ControlFlow::Exit
                            }
                            Some(e) => warn!("Surface reported error: {:?}", e),
                            None => {
                                error!("Failed to render: {:?}", e);
                            }
                        };
                        return;
                    }
                };

                let egui_output = {
                    platform.begin_frame();

                    let ctx = &platform.context();

                    let response = egui::SidePanel::left("inspection_panel")
                        .show(ctx, |ui| render_ui(&mut ui_state, ctx, ui));

                    platform.end_frame(Some(&window))
                };

                match state.submit_frame(frame, &platform, egui_output) {
                    Ok(_) => {}
                    Err(e) => {
                        error!("Failed to submit frame: {:?}", e);
                        return;
                    }
                }
            }

            Event::MainEventsCleared => {
                window.request_redraw();
            }
            _ => {}
        }
    });
}
