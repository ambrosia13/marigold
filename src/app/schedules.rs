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

use bevy_ecs::schedule::{ExecutorKind, IntoScheduleConfigs, Schedule, ScheduleLabel};

use crate::app::{
    data::{atmosphere, camera, fps, input, scene, time},
    menu,
    messages::{
        AtmosphereRebakeMessage, ExitMessage, KeyInputMessage, MouseInputMessage,
        MouseMotionMessage, init_message_type, update_message_type,
    },
    pass::{background, bake, display, geometry, post},
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

        if std::env::var("SINGLE_THREADED").is_ok_and(|v| v == "1") {
            log::info!("using single threaded system execution due to environment variable");

            on_init_message_setup.set_executor_kind(ExecutorKind::SingleThreaded);
            on_init_render_setup.set_executor_kind(ExecutorKind::SingleThreaded);
            on_init_app_setup.set_executor_kind(ExecutorKind::SingleThreaded);
            on_init_menu_setup.set_executor_kind(ExecutorKind::SingleThreaded);

            on_redraw_pre_frame.set_executor_kind(ExecutorKind::SingleThreaded);
            on_redraw_render.set_executor_kind(ExecutorKind::SingleThreaded);
            on_redraw_post_frame.set_executor_kind(ExecutorKind::SingleThreaded);
            on_redraw_message_update.set_executor_kind(ExecutorKind::SingleThreaded);
            on_redraw_menu_update.set_executor_kind(ExecutorKind::SingleThreaded);

            on_resize.set_executor_kind(ExecutorKind::SingleThreaded);
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

        schedules.on_init_app_setup.add_systems(
            (
                time::Time::init,
                fps::FpsCounter::init,
                (
                    input::Input::init,
                    camera::Camera::init,
                    // debug_menu::DebugMenus::init,
                    atmosphere::AtmosphereParams::init,
                    (
                        scene::geometry::init_geometry_buffers,
                        scene::geometry::mesh::LoadedMeshes::init,
                        scene::geometry::mesh::load_all_mesh_assets,
                    )
                        .chain(),
                ),
            )
                .chain(),
        );

        schedules.on_init_render_setup.add_systems(
            (
                (
                    camera::ScreenBinding::init,
                    atmosphere::AtmosphereBinding::init,
                    scene::SceneBinding::init,
                    // debug_menu::DebugMenuBinding::init,
                ),
                (
                    bake::AtmosphereBakePass::init,
                    background::AtmosphereCubemapPass::init,
                    background::BackgroundBinding::init,
                    geometry::GeometryTextures::init,
                    geometry::GeometryCommon::init, // doesn't need to run on resize
                    geometry::create_pathtrace_pipeline,
                    post::PostTextures::init,
                    post::PostPasses::init,
                    display::DisplayPass::init,
                )
                    .chain(),
            )
                .chain(),
        );

        schedules.on_redraw_pre_frame.add_systems((
            input::handle_keyboard_input_event,
            input::handle_mouse_input_event,
            camera::Camera::update,
            scene::geometry::update_geometry_buffers,
        ));

        schedules.on_redraw_render.add_systems(
            (
                (
                    camera::ScreenBinding::update,
                    // debug_menu::DebugMenuBinding::update,
                    atmosphere::AtmosphereBinding::update,
                    scene::SceneBinding::update,
                ),
                bake::AtmosphereBakePass::update,
                background::AtmosphereCubemapPass::update,
                geometry::draw_geometry,
                post::PostTextures::update, // copy geometry output to post texture input
                post::PostPasses::update,
                display::DisplayPass::update,
            )
                .chain(),
        );

        schedules.on_redraw_post_frame.add_systems(
            (
                input::Input::update,
                // debug_menu::DebugMenus::update,
            )
                .chain(),
        );

        schedules.on_redraw_menu_update.add_systems(
            (
                menu::diagnostics_menu,
                menu::fps_graph_menu,
                menu::controls_menu,
                menu::camera_menu,
                menu::atmosphere_menu,
            )
                .chain(),
        );

        schedules.on_resize.add_systems(
            (
                camera::Camera::on_resize,
                (
                    geometry::GeometryTextures::init,
                    post::PostTextures::init,
                    display::DisplayPass::init,
                )
                    .chain(),
            )
                .chain(),
        );

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
