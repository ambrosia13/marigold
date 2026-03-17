use std::sync::Arc;

use bevy_ecs::{message::Messages, world::World};
use glam::DVec2;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::KeyCode,
    platform::wayland::WindowAttributesExtWayland,
    window::{CursorGrabMode, Window, WindowAttributes},
};

use crate::{
    app::{
        messages::{ExitMessage, KeyInputMessage, MouseInputMessage, MouseMotionMessage},
        render::{FrameRecord, SurfaceState},
        schedules::Schedules,
    },
    egui::EguiRenderState,
};

pub mod data;
pub mod menu;
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

// toggle with Esc
#[derive(Default, PartialEq, Eq, Clone, Copy)]
enum FocusState {
    #[default]
    Renderer,
    Menu,
}

// toggle with F1
#[derive(Default, PartialEq, Eq, Clone, Copy)]
enum MenuState {
    #[default]
    Shown,
    Hidden,
}

struct AppState {
    world: World,
    window: Arc<Window>, // this field should be dropped after world, since world contains the surface, which references the window
    schedules: Schedules,
    focus_state: FocusState,
    menu_state: MenuState,
}

impl AppState {
    pub fn init(event_loop: &ActiveEventLoop) -> anyhow::Result<Self> {
        let window_attributes = WindowAttributes::default()
            .with_title("marigold renderer")
            .with_name("marigold", "");

        let window = Arc::new(event_loop.create_window(window_attributes)?);

        let mut world = World::new();
        let mut schedules = Schedules::default();

        let surface_state = pollster::block_on(SurfaceState::new(window.clone()))?;
        let egui_render_state = EguiRenderState::new(
            &surface_state.gpu.device,
            surface_state.config.format,
            None,
            1,
            &window,
        );

        // initial world data
        world.insert_non_send_resource(egui_render_state);
        world.insert_resource(surface_state);

        // run startup systems
        schedules.on_init_message_setup.run(&mut world);
        schedules.on_init_app_setup.run(&mut world);
        schedules.on_init_render_setup.run(&mut world);
        schedules.on_init_menu_setup.run(&mut world);

        Ok(Self {
            window,
            world,
            schedules,
            focus_state: Default::default(),
            menu_state: Default::default(),
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

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        #[allow(unused)]
        let Some(AppState {
            window,
            world,
            schedules,
            focus_state,
            menu_state,
        }) = &mut self.state
        else {
            return;
        };

        #[allow(clippy::single_match)]
        match event {
            winit::event::DeviceEvent::MouseMotion { delta }
                if *focus_state == FocusState::Renderer =>
            {
                world.write_message(MouseMotionMessage(DVec2::new(delta.0, -delta.1)));
            }
            _ => {}
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
            focus_state,
            menu_state,
        }) = &mut self.state
        else {
            return;
        };

        if window.id() != window_id {
            return;
        }

        if *focus_state == FocusState::Menu && *menu_state == MenuState::Shown {
            // allow egui to process
            let mut egui_render_state = world.non_send_resource_mut::<EguiRenderState>();
            egui_render_state.handle_input(window, &event);
        }

        // update cursor confinement depending on state
        match *focus_state {
            FocusState::Renderer => {
                window
                    .set_cursor_grab(CursorGrabMode::Confined)
                    .or_else(|_e| window.set_cursor_grab(CursorGrabMode::Locked))
                    .unwrap();

                window.set_cursor_visible(false);
            }
            FocusState::Menu => {
                window.set_cursor_grab(CursorGrabMode::None).unwrap();
                window.set_cursor_visible(true);
            }
        }

        match event {
            // input events
            WindowEvent::KeyboardInput { event, .. } => {
                if event.physical_key == KeyCode::Escape
                    && event.state == ElementState::Pressed
                    && !event.repeat
                {
                    *focus_state = match *focus_state {
                        FocusState::Renderer => {
                            log::info!("focus changed to menu, unlocking cursor");
                            FocusState::Menu
                        }
                        FocusState::Menu => {
                            log::info!("focus changed to renderer, locking cursor");
                            FocusState::Renderer
                        }
                    }
                }

                if event.physical_key == KeyCode::F1
                    && event.state == ElementState::Pressed
                    && !event.repeat
                {
                    *menu_state = match *menu_state {
                        MenuState::Shown => {
                            log::info!("menu hidden, changing focus to renderer");
                            // focus on the renderer if menu is hidden
                            *focus_state = FocusState::Renderer;
                            MenuState::Hidden
                        }
                        MenuState::Hidden => {
                            log::info!("menu shown");
                            MenuState::Shown
                        }
                    }
                }

                // send to the app if not focused on menu, otherwise egui will process
                if *focus_state == FocusState::Renderer {
                    world.write_message(KeyInputMessage(event));
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                // send to app if not focused on menu, otherwise egui will process
                #[allow(clippy::collapsible_match)]
                if *focus_state == FocusState::Renderer {
                    world.write_message(MouseInputMessage { state, button });
                }
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
                // check for requests to exit the program
                if !world.resource::<Messages<ExitMessage>>().is_empty() {
                    log::info!(
                        "application exit was requested by a system, exiting window event loop"
                    );
                    event_loop.exit();
                }

                // We want another frame after this one
                window.request_redraw();

                // run the pre-render systems
                schedules.on_redraw_pre_frame.run(world);

                // initialize frame
                let surface_state = world.resource::<SurfaceState>();
                let gpu = surface_state.gpu.clone();

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

                // need lifetime/borrowing shenanigans because we manually render egui rather than put it in a system
                let surface_texture_view = frame.surface_texture_view.clone();

                let mut egui_render_state = world.non_send_resource_mut::<EguiRenderState>();
                egui_render_state.begin_frame(window);

                // pass the frame ownership over to the world
                world.insert_resource(frame);

                // render the frame
                schedules.on_redraw_render.run(world);

                // run the menu systems, if menu is supposed to be shown
                if *menu_state == MenuState::Shown {
                    schedules.on_redraw_menu_update.run(world);
                }

                // now that the frame has been rendered, take frame data back so we can draw egui on top
                let mut frame = world.remove_resource::<FrameRecord>().unwrap();

                let mut egui_render_state = world.non_send_resource_mut::<EguiRenderState>();
                egui_render_state.end_frame_and_draw(
                    &gpu.device,
                    &gpu.queue,
                    &mut frame.encoder,
                    window,
                    &surface_texture_view,
                    egui_wgpu::ScreenDescriptor {
                        size_in_pixels: [window.inner_size().width, window.inner_size().height],
                        pixels_per_point: window.scale_factor() as f32,
                    },
                );

                // clean up and present the frame
                window.pre_present_notify();

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
