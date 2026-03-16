use bevy_ecs::system::{Local, NonSend};

use crate::egui::EguiRenderState;

pub fn diagnostics_menu(egui_render_state: NonSend<EguiRenderState>, mut text: Local<String>) {
    egui::Window::new("marigold renderer").resizable(true).show(
        egui_render_state.context(),
        |ui| {
            ui.label("hello");
            ui.text_edit_singleline(&mut *text);
            log::info!("{}", *text);
        },
    );
}
