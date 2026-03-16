use std::sync::Arc;

use bevy_ecs::world::World;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowAttributes},
};

use crate::app::{
    messages::{KeyInputMessage, MouseInputMessage},
    render::{FrameRecord, SurfaceState},
    schedules::Schedules,
};

pub mod data;
// pub mod debug_menu;
pub mod messages;
pub mod pass;
pub mod render;
pub mod schedules;

pub fn run() {
    let mut event_loop = EventLoop::builder();

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Ok(env_var) = std::env::var("WINIT_UNIX_BACKEND") {
            match env_var.as_str() {
                "x11" => {
                    use winit::platform::x11::EventLoopBuilderExtX11;

                    event_loop.with_x11();
                }
                "wayland" => {
                    use winit::platform::wayland::EventLoopBuilderExtWayland;

                    event_loop.with_wayland();
                }
                _ => panic!("WINIT_UNIX_BACKEND must be one of `x11` or `wayland`"),
            }
        }
    }

    let event_loop = event_loop
        .build()
        .expect("Couldn't create window event loop");

    let mut app = App { state: None };

    event_loop.run_app(&mut app).unwrap();
}

struct AppState {
    window: Arc<Window>,
    world: World,
    schedules: Schedules,
}

impl AppState {
    pub fn init(event_loop: &ActiveEventLoop) -> anyhow::Result<Self> {
        let window_attributes = WindowAttributes::default().with_title("marigold renderer");

        let window = Arc::new(event_loop.create_window(window_attributes)?);

        let mut world = World::new();
        let mut schedules = Schedules::default();

        let surface_state = pollster::block_on(SurfaceState::new(window.clone()))?;

        // initial world data
        world.insert_resource(surface_state);

        // run startup systems
        schedules.on_init_message_setup.run(&mut world);
        schedules.on_init_app_setup.run(&mut world);
        schedules.on_init_render_setup.run(&mut world);

        Ok(Self {
            window,
            world,
            schedules,
        })
    }
}

pub struct App {
    state: Option<AppState>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_none() {
            self.state = Some(AppState::init(event_loop).unwrap());
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let Some(AppState {
            window,
            world,
            schedules,
        }) = &mut self.state
        else {
            return;
        };

        if window.id() != window_id {
            return;
        }

        match event {
            // input events
            WindowEvent::KeyboardInput { event, .. } => {
                world.write_message(KeyInputMessage(event));
            }
            WindowEvent::MouseInput { state, button, .. } => {
                world.write_message(MouseInputMessage { state, button });
            }
            WindowEvent::MouseWheel { delta: _, .. } => {}

            // lifecycle events
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                let mut surface_state = world.resource_mut::<SurfaceState>();
                surface_state.resize(size);

                schedules.on_resize.run(world);
            }
            WindowEvent::RedrawRequested => {
                // We want another frame after this one
                window.request_redraw();

                // run the pre-render systems
                schedules.on_redraw_pre_frame.run(world);

                // initialize frame
                let surface_state = world.resource::<SurfaceState>();
                let frame = match surface_state.begin_frame() {
                    Ok(r) => r,
                    Err(
                        wgpu::SurfaceError::Lost
                        | wgpu::SurfaceError::Outdated
                        | wgpu::SurfaceError::Other,
                    ) => {
                        log::warn!("Unable to get surface handle, reconfiguring");
                        surface_state.reconfigure_surface();
                        return;
                    }
                    Err(wgpu::SurfaceError::Timeout) => {
                        log::warn!("Surface timeout, skipping frame");
                        return;
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => {
                        log::error!("Out of memory, exiting");
                        event_loop.exit();
                        return;
                    }
                };

                // render the frame
                world.insert_resource(frame);
                schedules.on_redraw_render.run(world);

                // clean up and prepare to present
                window.pre_present_notify();

                let frame = world.remove_resource::<FrameRecord>().unwrap();
                let surface_state = world.resource::<SurfaceState>();

                surface_state.finish_frame(frame);

                // run the post-render systems
                schedules.on_redraw_post_frame.run(world);
                schedules.on_redraw_message_update.run(world);
            }
            _ => {}
        }
    }
}
