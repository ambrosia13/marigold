use bevy_ecs::message::Messages;
use glam::DVec2;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::KeyCode,
    window::CursorGrabMode,
};

use crate::{
    app::time::{FpsCounter, Time},
    vk::{FrameError, FrameRecord, GpuHandle, SurfaceState},
    window::{
        messages::{ExitMessage, KeyInputMessage, MouseInputMessage, MouseMotionMessage},
        state::{AppState, FocusState, MenuState},
    },
};

pub mod messages;
pub mod schedules;
pub mod state;

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

    let gpu_pre_init = GpuHandle::pre_init(&event_loop).expect("failed to instantiate vulkan");
    let app_state = AppState::PreInit { gpu_pre_init };

    let mut app = App { app_state };

    event_loop.run_app(&mut app).unwrap();
}

struct App {
    app_state: AppState,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if matches!(self.app_state, AppState::PreInit { .. }) {
            self.app_state.initialize(event_loop).unwrap();
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        #[allow(unused)]
        let AppState::Window {
            window,
            world,
            schedules,
            focus_state,
            menu_state,
        } = &mut self.app_state
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
        #[allow(unused)]
        let AppState::Window {
            window,
            world,
            schedules,
            focus_state,
            menu_state,
        } = &mut self.app_state
        else {
            return;
        };

        if window.id() != window_id {
            return;
        }

        // if *focus_state == FocusState::Menu && *menu_state == MenuState::Shown {
        //     // allow egui to process
        //     let mut egui_render_state = world.non_send_resource_mut::<EguiRenderState>();
        //     egui_render_state.handle_input(window, &event);
        // }

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

        schedules.on_window_event.run(world);

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
            WindowEvent::MouseWheel { .. } => {}

            // lifecycle events
            WindowEvent::CloseRequested => {
                let surface_state = world.resource::<SurfaceState>();
                surface_state.gpu.device.wait_idle().unwrap();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                let mut surface_state = world.resource_mut::<SurfaceState>();
                surface_state.resize(size);

                schedules.on_resize.run(world);
            }
            WindowEvent::RedrawRequested => {
                // update time & fps counters before doing any work for the frame
                let mut time = world.resource_mut::<Time>();
                time.tick(); // calculates & updates delta
                let delta = time.delta(); // fetches updated delta
                let mut fps = world.resource_mut::<FpsCounter>();
                fps.tick(delta);

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
                let mut surface_state = world.resource_mut::<SurfaceState>();
                let frame = match surface_state.begin_frame() {
                    Ok(r) => r,
                    Err(FrameError::SkipFrame) => {
                        return;
                    }
                    Err(e) => {
                        log::info!(
                            "exiting window event loop due to unrecoverable error: {:?}",
                            e
                        );
                        event_loop.exit();
                        return;
                    }
                };

                // pass the frame ownership over to the world
                world.insert_non_send(frame);

                // render the frame
                schedules.on_redraw_render.run(world);

                // run the menu systems, if menu is supposed to be shown
                if *menu_state == MenuState::Shown {
                    schedules.on_redraw_menu_update.run(world);
                }

                // now that the frame has been rendered, take frame data back so we can draw egui on top
                let frame = world.remove_non_send::<FrameRecord>().unwrap();

                let mut surface_state = world.resource_mut::<SurfaceState>();
                surface_state
                    .finish_frame(frame)
                    .expect("failed to submit frame");

                // run the post-render systems
                schedules.on_redraw_post_frame.run(world);

                schedules.on_redraw_message_update.run(world);
            }
            _ => {}
        }
    }
}
