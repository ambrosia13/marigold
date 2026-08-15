use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, ScheduleLabel, SingleThreadedExecutor};

use crate::{
    app::{camera, input, scene, time},
    window::messages::{
        AtmosphereRebakeMessage, ExitMessage, KeyInputMessage, MouseInputMessage,
        MouseMotionMessage, init_message_type, update_message_type,
    },
};

pub type SystemResult = bevy_ecs::error::Result<()>;

#[derive(ScheduleLabel, Eq, PartialEq, Copy, Clone, Hash, Debug)]
struct OnResizeSchedule;

#[derive(ScheduleLabel, Eq, PartialEq, Copy, Clone, Hash, Debug)]
struct OnInitMessageSetupSchedule;

#[derive(ScheduleLabel, Eq, PartialEq, Copy, Clone, Hash, Debug)]
struct OnInitRenderSetupSchedule;

#[derive(ScheduleLabel, Eq, PartialEq, Copy, Clone, Hash, Debug)]
struct OnInitAppSetupSchedule;

#[derive(ScheduleLabel, Eq, PartialEq, Copy, Clone, Hash, Debug)]
struct OnInitMenuSetupSchedule;

#[derive(ScheduleLabel, Eq, PartialEq, Copy, Clone, Hash, Debug)]
struct OnRedrawPreFrameSchedule;

#[derive(ScheduleLabel, Eq, PartialEq, Copy, Clone, Hash, Debug)]
struct OnRedrawRenderSchedule;

#[derive(ScheduleLabel, Eq, PartialEq, Copy, Clone, Hash, Debug)]
struct OnRedrawPostFrameSchedule;

#[derive(ScheduleLabel, Eq, PartialEq, Copy, Clone, Hash, Debug)]
struct OnRedrawMessageUpdateSchedule;

#[derive(ScheduleLabel, Eq, PartialEq, Copy, Clone, Hash, Debug)]
struct OnRedrawMenuUpdateSchedule;

pub struct Schedules {
    // startup schedules
    pub on_init_message_setup: Schedule,
    pub on_init_render_setup: Schedule,
    pub on_init_app_setup: Schedule,
    pub on_init_menu_setup: Schedule,

    // per-frame schedules
    pub on_redraw_pre_frame: Schedule,
    pub on_redraw_render: Schedule,
    pub on_redraw_post_frame: Schedule,
    pub on_redraw_message_update: Schedule,
    pub on_redraw_menu_update: Schedule,

    // event-driven schedules
    pub on_resize: Schedule,
}

impl Default for Schedules {
    fn default() -> Self {
        // startup schedules
        let mut on_init_message_setup = Schedule::new(OnInitMessageSetupSchedule);
        let mut on_init_render_setup = Schedule::new(OnInitRenderSetupSchedule);
        let mut on_init_app_setup = Schedule::new(OnInitAppSetupSchedule);
        let mut on_init_menu_setup = Schedule::new(OnInitMenuSetupSchedule);

        // per-frame schedules
        let mut on_redraw_pre_frame = Schedule::new(OnRedrawPreFrameSchedule);
        let mut on_redraw_render = Schedule::new(OnRedrawRenderSchedule);
        let mut on_redraw_post_frame = Schedule::new(OnRedrawPostFrameSchedule);
        let mut on_redraw_message_update = Schedule::new(OnRedrawMessageUpdateSchedule);
        let mut on_redraw_menu_update = Schedule::new(OnRedrawMenuUpdateSchedule);

        // event-driven schedules
        let mut on_resize = Schedule::new(OnResizeSchedule);

        if crate::util::get_env_flag("ECS_SINGLE_THREADED") {
            log::info!("using single threaded ECS system execution due to environment variable");

            on_init_message_setup.set_executor(SingleThreadedExecutor::default());
            on_init_render_setup.set_executor(SingleThreadedExecutor::default());
            on_init_app_setup.set_executor(SingleThreadedExecutor::default());
            on_init_menu_setup.set_executor(SingleThreadedExecutor::default());

            on_redraw_pre_frame.set_executor(SingleThreadedExecutor::default());
            on_redraw_render.set_executor(SingleThreadedExecutor::default());
            on_redraw_post_frame.set_executor(SingleThreadedExecutor::default());
            on_redraw_message_update.set_executor(SingleThreadedExecutor::default());
            on_redraw_menu_update.set_executor(SingleThreadedExecutor::default());

            on_resize.set_executor(SingleThreadedExecutor::default());
        }

        let mut schedules = Self {
            on_init_message_setup,
            on_init_render_setup,
            on_init_app_setup,
            on_redraw_pre_frame,
            on_redraw_render,
            on_redraw_post_frame,
            on_redraw_message_update,
            on_resize,
            on_init_menu_setup,
            on_redraw_menu_update,
        };

        // app setup
        schedules.on_init_app_setup.add_systems(
            (
                time::Time::init, // time init runs before everything else
                (
                    time::FpsCounter::init,
                    input::Input::init,
                    camera::Camera::init,
                    (scene::enumerate_models, scene::load_active_model).chain(),
                ),
            )
                .chain(),
        );

        // render setup
        // schedules
        //     .on_init_render_setup
        //     .add_systems();

        // per-frame update
        schedules.on_redraw_pre_frame.add_systems((
            input::handle_keyboard_input_event,
            input::handle_mouse_input_event,
        ));

        schedules
            .on_redraw_render
            .add_systems(scene::upload_active_model);

        schedules
            .on_redraw_post_frame
            .add_systems(input::Input::update);

        // messages
        schedules.on_init_message_setup.add_systems((
            init_message_type::<MouseMotionMessage>,
            init_message_type::<KeyInputMessage>,
            init_message_type::<MouseInputMessage>,
            init_message_type::<ExitMessage>,
            init_message_type::<AtmosphereRebakeMessage>,
        ));

        schedules.on_redraw_message_update.add_systems((
            update_message_type::<MouseMotionMessage>,
            update_message_type::<KeyInputMessage>,
            update_message_type::<MouseInputMessage>,
            update_message_type::<ExitMessage>,
            update_message_type::<AtmosphereRebakeMessage>,
        ));

        schedules
    }
}
