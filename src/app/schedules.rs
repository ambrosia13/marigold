/*
***App lifecycle***

Startup:
    OnInitEventSetup
    OnInitRenderSetup
    OnInitAppSetup

Per-frame:
    OnRedrawPreFrame
    OnRedrawRender
    OnRedrawPostFrame
    OnRedrawEventUpdate

Event-driven:
    OnResize
*/

use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, ScheduleLabel};

use crate::app::{
    data::{self, camera, input, time},
    messages::{
        KeyInputMessage, MouseInputMessage, MouseMotionMessage, init_message_type,
        update_message_type,
    },
    pass::{display, post},
};

#[derive(ScheduleLabel, Eq, PartialEq, Copy, Clone, Hash, Debug)]
struct OnResizeSchedule;

#[derive(ScheduleLabel, Eq, PartialEq, Copy, Clone, Hash, Debug)]
struct OnInitMessageSetupSchedule;

#[derive(ScheduleLabel, Eq, PartialEq, Copy, Clone, Hash, Debug)]
struct OnInitRenderSetupSchedule;

#[derive(ScheduleLabel, Eq, PartialEq, Copy, Clone, Hash, Debug)]
struct OnInitAppSetupSchedule;

#[derive(ScheduleLabel, Eq, PartialEq, Copy, Clone, Hash, Debug)]
struct OnRedrawPreFrameSchedule;

#[derive(ScheduleLabel, Eq, PartialEq, Copy, Clone, Hash, Debug)]
struct OnRedrawRenderSchedule;

#[derive(ScheduleLabel, Eq, PartialEq, Copy, Clone, Hash, Debug)]
struct OnRedrawPostFrameSchedule;

#[derive(ScheduleLabel, Eq, PartialEq, Copy, Clone, Hash, Debug)]
struct OnRedrawMessageUpdateSchedule;

pub struct Schedules {
    // startup schedules
    pub on_init_message_setup: Schedule,
    pub on_init_render_setup: Schedule,
    pub on_init_app_setup: Schedule,

    // per-frame schedules
    pub on_redraw_pre_frame: Schedule,
    pub on_redraw_render: Schedule,
    pub on_redraw_post_frame: Schedule,
    pub on_redraw_message_update: Schedule,

    // event-driven schedules
    pub on_resize: Schedule,
}

impl Default for Schedules {
    fn default() -> Self {
        // startup schedules
        let on_init_message_setup = Schedule::new(OnInitMessageSetupSchedule);
        let on_init_render_setup = Schedule::new(OnInitRenderSetupSchedule);
        let on_init_app_setup = Schedule::new(OnInitAppSetupSchedule);

        // per-frame schedules
        let on_redraw_pre_frame = Schedule::new(OnRedrawPreFrameSchedule);
        let on_redraw_render = Schedule::new(OnRedrawRenderSchedule);
        let on_redraw_post_frame = Schedule::new(OnRedrawPostFrameSchedule);
        let on_redraw_message_update = Schedule::new(OnRedrawMessageUpdateSchedule);

        // event-driven schedules
        let on_resize = Schedule::new(OnResizeSchedule);

        let mut schedules = Self {
            on_init_message_setup,
            on_init_render_setup,
            on_init_app_setup,
            on_redraw_pre_frame,
            on_redraw_render,
            on_redraw_post_frame,
            on_redraw_message_update,
            on_resize,
        };

        schedules
            .on_init_app_setup
            .add_systems((time::Time::init, (input::Input::init, camera::Camera::init)).chain());

        schedules.on_init_render_setup.add_systems((
            camera::ScreenBinding::init,
            (post::PostTextures::init, display::DisplayBinding::init).chain(),
        ));

        schedules.on_redraw_pre_frame.add_systems((
            input::handle_keyboard_input_event,
            input::handle_mouse_input_event,
            camera::Camera::update,
        ));

        schedules
            .on_redraw_render
            .add_systems(camera::ScreenBinding::update);

        schedules
            .on_redraw_post_frame
            .add_systems((input::Input::update, time::Time::update).chain());

        schedules.on_resize.add_systems((
            camera::Camera::on_resize,
            (post::PostTextures::init, display::DisplayBinding::init).chain(),
        ));

        // messages
        schedules.on_init_message_setup.add_systems((
            init_message_type::<MouseMotionMessage>,
            init_message_type::<KeyInputMessage>,
            init_message_type::<MouseInputMessage>,
        ));

        schedules.on_redraw_message_update.add_systems((
            update_message_type::<MouseMotionMessage>,
            update_message_type::<KeyInputMessage>,
            update_message_type::<MouseInputMessage>,
        ));

        schedules
    }
}
