use std::sync::Arc;

use bevy_ecs::world::World;
use winit::{
    event_loop::ActiveEventLoop,
    platform::wayland::WindowAttributesExtWayland,
    window::{Window, WindowAttributes},
};

use crate::{
    vulkan::{GpuHandle, GpuHandlePreInit, SurfaceState},
    window::schedules::Schedules,
};

// toggle with Esc
#[derive(Default, PartialEq, Eq, Clone, Copy)]
pub(super) enum FocusState {
    #[default]
    Renderer,
    Menu,
}

// toggle with F1
#[derive(Default, PartialEq, Eq, Clone, Copy)]
pub(super) enum MenuState {
    #[default]
    Shown,
    Hidden,
}

/// app state that is tied to the event loop (i.e. not tied to the window)
#[allow(clippy::large_enum_variant)]
pub(super) enum AppState {
    // the state before the window is created
    PreInit {
        gpu_pre_init: GpuHandlePreInit,
    },
    // the state after the window is created (basically everything)
    Window {
        world: World,
        window: Arc<Window>, // this field should be dropped after world, since world contains the surface, which references the window
        schedules: Schedules,
        focus_state: FocusState,
        menu_state: MenuState,
    },
}

impl AppState {
    pub fn initialize(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        assert!(matches!(self, AppState::PreInit { .. }));

        let AppState::PreInit { gpu_pre_init } = self else {
            unreachable!()
        };

        let window_attributes = WindowAttributes::default()
            .with_title("marigold renderer")
            .with_name("marigold", "");

        let window = Arc::new(event_loop.create_window(window_attributes)?);

        let mut world = World::new();
        let mut schedules = Schedules::default();

        let (gpu, surface) = GpuHandle::new(gpu_pre_init.clone(), window.clone())?;
        let surface_state = SurfaceState::new(&gpu, surface, window.clone())?;

        // initial world data
        world.insert_resource(gpu);
        world.insert_non_send_resource(surface_state);

        // run startup systems
        schedules.on_init_message_setup.run(&mut world);
        schedules.on_init_app_setup.run(&mut world);
        schedules.on_init_render_setup.run(&mut world);
        schedules.on_init_menu_setup.run(&mut world);

        let new_state = AppState::Window {
            window,
            world,
            schedules,
            focus_state: Default::default(),
            menu_state: Default::default(),
        };

        *self = new_state;

        Ok(())
    }
}
