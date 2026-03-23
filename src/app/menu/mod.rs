use std::{ops::IndexMut, time::Instant};

use bevy_ecs::{
    message::MessageWriter,
    system::{Local, NonSend, Res, ResMut},
};
use egui::{DragValue, Ui};
use egui_plot::{HLine, Legend, Line, Plot, PlotPoints, Span};

use crate::{
    app::{
        data::{
            atmosphere::AtmosphereParams,
            camera::Camera,
            fps::{self, FpsCounter},
            time::Time,
        },
        messages::ExitMessage,
        render::SurfaceState,
    },
    egui::EguiRenderState,
};

pub fn uint_editor(value: &mut u32, ui: &mut Ui) {
    ui.add(DragValue::new(value).speed(0.1));
}

pub fn float_editor(value: &mut f32, ui: &mut Ui) {
    ui.add(
        DragValue::new(value)
            .min_decimals(2)
            .max_decimals(10)
            .speed(0.01),
    );
}

pub fn vector_editor<V, const N: usize>(values: &mut V, ui: &mut Ui, use_rgba: bool)
where
    V: IndexMut<usize, Output = f32> + Into<[f32; N]>,
{
    ui.horizontal(|ui| {
        let labels_owned = (1..=N).map(|i| format!("{}", i)).collect::<Vec<String>>();

        let labels: &[&str] = if N <= 4 {
            if use_rgba {
                &["R", "G", "B", "A"]
            } else {
                &["X", "Y", "Z", "W"]
            }
        } else if N <= 26 {
            &[
                "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P",
                "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
            ]
        } else {
            &labels_owned.iter().map(|s| s.as_str()).collect::<Vec<_>>()
        };

        for i in 0..N {
            ui.label(labels[i]);
            float_editor(&mut values[i], ui);
        }
    });
}

pub fn diagnostics_menu(
    egui_render_state: NonSend<EguiRenderState>,
    surface_state: Res<SurfaceState>,
    time: Res<Time>,
    fps: Res<FpsCounter>,
    mut exit_messages: MessageWriter<ExitMessage>,
) {
    egui::Window::new("Info").show(egui_render_state.context(), |ui| {
        let info = surface_state.gpu.adapter.get_info();

        ui.heading("marigold renderer");
        ui.separator();

        ui.label(format!("GPU: {}", info.name));
        ui.label(format!("Driver: {}", info.driver_info));
        ui.label(format!("Backend: {}", info.backend.to_str()));
        ui.label(format!("Platform: {}", std::env::consts::OS));

        ui.separator();

        let average_fps = fps.average_fps();
        ui.label(format!("Average FPS: {:.1}", average_fps));
        ui.label(format!("Average frametime: {:.3}", 1000.0 / average_fps));

        ui.separator();

        if ui.button("Exit marigold").clicked() {
            exit_messages.write(ExitMessage);
        }
    });
}

pub fn fps_graph_menu(
    egui_render_state: NonSend<EguiRenderState>,
    surface_state: Res<SurfaceState>,
    fps: Res<FpsCounter>,
) {
    egui::Window::new("FPS Graph")
        .default_open(true)
        .show(egui_render_state.context(), |ui| {
            let refresh_rate_hz = surface_state
                .window
                .current_monitor()
                .and_then(|m| m.refresh_rate_millihertz().map(|r| r as f64 * 0.001));

            let points: PlotPoints<'_> = fps
                .samples()
                .iter()
                .enumerate()
                .map(|(i, d)| [i as f64, 1.0 / d.as_secs_f64()])
                .collect();

            Plot::new("FPS Graph")
                .legend(Legend::default())
                .default_x_bounds(0.0, fps::FPS_NUM_SAMPLES as f64)
                .default_y_bounds(0.0, 150.0)
                .show(ui, |plot_ui| {
                    plot_ui
                        .set_plot_bounds_y(0.0..=refresh_rate_hz.map(|r| r * 1.5).unwrap_or(150.0));

                    if let Some(refresh_rate_hz) = refresh_rate_hz {
                        plot_ui.hline(
                            HLine::new("refresh rate", refresh_rate_hz).color(egui::Color32::GREEN),
                        );
                    }

                    plot_ui.line(
                        Line::new("fps", points)
                            .color(egui::Color32::RED)
                            .style(egui_plot::LineStyle::Solid),
                    );
                });
        });
}

pub fn controls_menu(egui_render_state: NonSend<EguiRenderState>) {
    egui::Window::new("Controls").show(egui_render_state.context(), |ui| {
        ui.label("Toggle focus between Menu/Renderer: Escape");
        ui.label("Toggle show/hide menu: F1");
    });
}

pub fn camera_menu(egui_render_state: NonSend<EguiRenderState>, mut camera: ResMut<Camera>) {
    egui::Window::new("Camera")
        .default_open(false)
        .show(egui_render_state.context(), |ui| {
            ui.label("Position:");
            vector_editor(&mut camera.position, ui, false);
            ui.label(format!("Direction: {:.3}", camera.forward()));
            ui.horizontal(|ui| {
                ui.label("Speed:");
                float_editor(&mut camera.movement_speed, ui);
            });
        });
}

pub fn atmosphere_menu(
    egui_render_state: NonSend<EguiRenderState>,
    mut local_params: Local<AtmosphereParams>,
    mut automatically_apply_changes: Local<bool>,
    mut atmosphere_params: ResMut<AtmosphereParams>,
) {
    egui::Window::new("Atmosphere Settings")        
        .default_open(false)
        .scroll([false, true])
        .show(egui_render_state.context(), |ui| {
            let apply_changes_button = ui.button("Apply changes (may cause lag spike)");
            ui.checkbox(&mut automatically_apply_changes, "Automatically apply changes that won't cause lag spike");

            ui.separator();

            ui.label("Sun Color:");
            vector_editor(&mut local_params.sun_color, ui, true);

            ui.label("Sun Direction:");
            vector_editor(&mut local_params.sun_direction, ui, false);

            ui.separator();

            ui.label("Moon Color:");
            vector_editor(&mut local_params.moon_color, ui, true);

            ui.label("Moon Direction:");
            vector_editor(&mut local_params.moon_direction, ui, false);

            ui.horizontal(|ui| {
                ui.label("Moon To Sun Brightness Ratio:");
                float_editor(&mut local_params.moon_to_sun_illuminance_ratio, ui);
            });

            ui.horizontal(|ui| {
                ui.label("Overall Atmosphere Brightness Multiplier");
                float_editor(&mut local_params.brightness_multiplier, ui);
            });

            ui.separator();

            ui.horizontal(|ui| {
                let scale_label = ui.label("Meters Per Unit");
                if scale_label.hovered() {
                    scale_label.show_tooltip_text(
                        "All other distance/length values are unitless, scaled to meters by this value. \
                        This is essentially the scale of the atmosphere relative to the camera."
                    );
                }
                float_editor(&mut local_params.meters_per_unit, ui);
            });

            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Ground Radius:");
                float_editor(&mut local_params.ground_radius, ui);
            });

            ui.horizontal(|ui| {
                ui.label("Atmosphere Radius:");
                float_editor(&mut local_params.atmosphere_radius, ui);
            });

            let origin_label = ui.label("Atmosphere Origin:");
            if origin_label.hovered() {
                origin_label.show_tooltip_text("By default, this is 500 meters below the ground.");
            }
            vector_editor(&mut local_params.origin, ui, false);

            ui.separator();

            ui.label("Rayleigh Scattering Coefficients:");
            vector_editor(&mut local_params.rayleigh_scattering_base, ui, false);

            ui.horizontal(|ui| {
                ui.label("Rayleigh Absorption Coefficient:");
                float_editor(&mut local_params.rayleigh_absorption_base, ui);
            });

            ui.horizontal(|ui| {
                ui.label("Rayleigh Scale Height:");
                float_editor(&mut local_params.rayleigh_scale_height, ui);
            });

            ui.separator();

            ui.horizontal(|ui| {
                let mie_scattering_label = ui.label("Mie Scattering Coefficient:");
                if mie_scattering_label.hovered() {
                    mie_scattering_label.show_tooltip_text(
                        "This value implicitly contains turbidity as a multiplier."
                    );
                }
                float_editor(&mut local_params.mie_scattering_base, ui);
            });

            ui.horizontal(|ui| {
                ui.label("Mie Absorption Coefficient:");
                float_editor(&mut local_params.mie_absorption_base, ui);
            });

            ui.horizontal(|ui| {
                ui.label("Mie Scale Height:");
                float_editor(&mut local_params.mie_scale_height, ui);
            });

            ui.horizontal(|ui| {
                ui.label("Mie Phase Anisotropy Factor (g):");
                float_editor(&mut local_params.atmosphere_g, ui);
            });

            ui.separator();

            ui.label("Ozone Absorption Coefficients:");
            vector_editor(&mut local_params.ozone_absorption_base, ui, false);

            ui.separator();

            ui.label("Ground Albedo:");
            vector_editor(&mut local_params.ground_albedo, ui, true);

            ui.separator();

            ui.label("The below settings affect quality and performance");

            ui.horizontal(|ui| {
                ui.label("Transmittance LUT Steps:");
                uint_editor(&mut local_params.transmittance_lut_steps, ui);
            });

            ui.horizontal(|ui| {
                ui.label("Multiscattering LUT Steps:");
                uint_editor(&mut local_params.multiscattering_lut_steps, ui);
            });

            ui.horizontal(|ui| {
                ui.label("Multiscattering LUT sqrt samples:");
                uint_editor(&mut local_params.multiscattering_lut_sqrt_samples, ui);
            });

            ui.horizontal(|ui| {
                ui.label("Sky View LUT Steps:");
                uint_editor(&mut local_params.sky_view_lut_steps, ui);
            });

            // normalize direction vectors before uploading
            local_params.sun_direction = local_params.sun_direction.normalize();
            local_params.moon_direction = local_params.moon_direction.normalize();

            if apply_changes_button.clicked() {
                // apply changes since the user requested it
                log::info!("Applying editor changes to atmosphere params");
                *atmosphere_params = local_params.clone();
            }

            if *automatically_apply_changes && *local_params != *atmosphere_params && !local_params.should_rebake(&atmosphere_params) {
                // if the changes won't cause a rebake, automatically apply them
                log::info!("Automatically applying editor changes to atmosphere params");
                *atmosphere_params = local_params.clone();
            }
        });
}
