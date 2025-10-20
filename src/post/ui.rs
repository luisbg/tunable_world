use bevy::{
    core_pipeline::{
        dof::{DepthOfField, DepthOfFieldMode},
        tonemapping::Tonemapping,
    },
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
    render::render_resource::Face,
};
use bevy_egui::{EguiContexts, egui};

use crate::camera::{FpsText, FpsUpdate};
use crate::post::{
    chroma_aberration::ChromaAberrationSettings,
    crt::CRTSettings,
    gradient_tint::GradientTintSettings,
    lut::{LutSettings, LutUiState},
    outlines::OutlineParams,
};

fn section(ui: &mut egui::Ui, title: &str, default_open: bool, body: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(title)
        .default_open(default_open)
        .show(ui, |ui| body(ui));
}

/// egui panel: tune post-processing effects
pub fn post_process_edit_panel(
    mut ctxs: EguiContexts,
    mut q_cam: Query<(&mut DepthOfField, &mut Tonemapping, &GlobalTransform), With<Camera3d>>,
    mut outline: ResMut<OutlineParams>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    (mut chroma_settings, mut crt_settings, mut gradient_tint_settings, mut lut_settings): (
        Query<&mut ChromaAberrationSettings>,
        Query<&mut CRTSettings>,
        Query<&mut GradientTintSettings>,
        Query<&mut LutSettings>,
    ),
    mut ui_state: ResMut<LutUiState>,
) {
    let Ok((mut dof, mut tonemapping, cam_xform)) = q_cam.single_mut() else {
        return;
    };

    // Local copies so sliders can edit smoothly
    let mut focal_distance = dof.focal_distance;
    let mut f_stops = dof.aperture_f_stops;
    let mut bokeh = matches!(dof.mode, DepthOfFieldMode::Bokeh);

    let mut enabled = outline.enabled;
    let mut width = outline.width;
    let mut color = outline.color;

    // --- Effect Settings window (collapsible sections)
    egui::Window::new("Effect settings")
        .default_width(300.0)
        .show(ctxs.ctx_mut().expect("single egui context"), |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // Depth of Field
                    section(ui, "Depth of Field", false, |ui| {
                        ui.add(
                            egui::Slider::new(&mut focal_distance, 1.0..=40.0)
                                .text("Focal distance"),
                        );
                        ui.add(
                            egui::Slider::new(&mut f_stops, 0.01..=64.0)
                                .logarithmic(true)
                                .text("Aperture (f-stops)"),
                        );
                        ui.checkbox(&mut bokeh, "Bokeh mode (prettier)");

                        ui.horizontal(|ui| {
                            if ui.button("Snap focus to origin").clicked() {
                                let cam_pos = cam_xform.translation();
                                focal_distance = cam_pos.length();
                            }
                            if ui.button("Reset DoF").clicked() {
                                focal_distance = 8.0;
                                f_stops = 2.0;
                                bokeh = true;
                            }
                        });
                    });

                    // Outline
                    section(ui, "Outline", false, |ui| {
                        ui.checkbox(&mut enabled, "Enabled");
                        ui.add(egui::Slider::new(&mut width, 0.0..=0.10).text("Width"));

                        // Simple RGB picker (gamma-aware conversions aren’t critical here)
                        let mut rgb = [
                            color.to_linear().red,
                            color.to_linear().green,
                            color.to_linear().blue,
                        ];
                        if ui.color_edit_button_rgb(&mut rgb).changed() {
                            color = Color::linear_rgb(rgb[0], rgb[1], rgb[2]);
                        }

                        if ui.button("Reset Outline").clicked() {
                            enabled = true;
                            width = 0.02;
                            color = Color::srgb(0.08, 0.10, 0.12);
                        }
                    });

                    // Chromatic Aberration
                    section(ui, "Chromatic Aberration", false, |ui| {
                        if let Ok(mut ca) = chroma_settings.single_mut() {
                            let mut on = ca.enabled != 0;

                            ui.add(
                                egui::Slider::new(&mut ca.intensity, 0.0..=0.05)
                                    .logarithmic(true)
                                    .text("Intensity"),
                            );
                            let resp = ui.checkbox(&mut on, "Enabled");
                            if resp.changed() {
                                ca.enabled = on as u32; // 1 or 0
                            }
                        }
                    });

                    // CRT
                    section(ui, "CRT", false, |ui| {
                        if let Ok(mut crt) = crt_settings.single_mut() {
                            let mut on = crt.enabled != 0;

                            ui.add(
                                egui::Slider::new(&mut crt.intensity, 0.0..=0.5)
                                    .logarithmic(true)
                                    .text("Intensity"),
                            );
                            ui.add(
                                egui::Slider::new(&mut crt.scanline_freq, 50.0..=500.0)
                                    .logarithmic(true)
                                    .text("Scanline Frequency"),
                            );
                            ui.add(
                                egui::Slider::new(&mut crt.line_intensity, 0.0..=1.0)
                                    .logarithmic(true)
                                    .text("Line Intensity"),
                            );
                            let resp = ui.checkbox(&mut on, "Enabled");
                            if resp.changed() {
                                crt.enabled = on as u32; // 1 or 0
                            }
                        }
                    });

                    // Gradient Tint
                    section(ui, "Gradient Tint", false, |ui| {
                        if let Ok(mut gt) = gradient_tint_settings.single_mut() {
                            let mut on = gt.enabled != 0;
                            let mut additive = gt.additive != 0;

                            ui.add(
                                egui::Slider::new(&mut gt.strength, 0.0..=1.0)
                                    .logarithmic(false)
                                    .text("Intensity"),
                            );

                            // Top-right color
                            let mut rgb_tr = [
                                gt.color_top_right.x,
                                gt.color_top_right.y,
                                gt.color_top_right.z,
                            ];
                            if ui.color_edit_button_rgb(&mut rgb_tr).changed() {
                                gt.color_top_right =
                                    Vec4::new(rgb_tr[0], rgb_tr[1], rgb_tr[2], 1.0);
                            }

                            // Bottom-left color
                            let mut rgb_bl = [
                                gt.color_bottom_left.x,
                                gt.color_bottom_left.y,
                                gt.color_bottom_left.z,
                            ];
                            if ui.color_edit_button_rgb(&mut rgb_bl).changed() {
                                gt.color_bottom_left =
                                    Vec4::new(rgb_bl[0], rgb_bl[1], rgb_bl[2], 1.0);
                            }

                            let mut resp = ui.checkbox(&mut on, "Enabled");
                            if resp.changed() {
                                gt.enabled = on as u32; // 1 or 0
                            }
                            resp = ui.checkbox(&mut additive, "Additive");
                            if resp.changed() {
                                gt.additive = additive as u32; // 1 or 0
                            }
                        }
                    });

                    // LUT
                    section(ui, "LUT", false, |ui| {
                        if let Ok(mut lut) = lut_settings.single_mut() {
                            let mut on = lut.enabled != 0;
                            let resp = ui.checkbox(&mut on, "Enabled");
                            if resp.changed() {
                                lut.enabled = on as u32; // 1 or 0
                            }

                            ui.label("PNG path:");
                            let te = egui::TextEdit::singleline(&mut ui_state.path)
                                .hint_text("luts/lookup.png")
                                .desired_width(200.0);
                            ui.add(te);

                            if ui.button("Load").clicked() {
                                ui_state.pending = Some(ui_state.path.clone());
                            }
                        }
                    });

                    section(ui, "Renderer Features", true, |ui| {
                        // ---- Tonemapping ----
                        let mut tm_on = *tonemapping != Tonemapping::None;
                        if ui.checkbox(&mut tm_on, "Tonemapping").changed() {
                            if tm_on && *tonemapping == Tonemapping::None {
                                *tonemapping = Tonemapping::AcesFitted;
                            } else if !tm_on {
                                *tonemapping = Tonemapping::None;
                            }
                        }
                    });
                });
        });

    // Apply DoF
    dof.focal_distance = focal_distance.max(0.1);
    dof.aperture_f_stops = f_stops.clamp(0.01, 64.0);
    dof.mode = if bokeh {
        DepthOfFieldMode::Bokeh
    } else {
        DepthOfFieldMode::Gaussian
    };

    // Apply Outline params (change shared material color)
    if let Some(mat) = materials.get_mut(&outline.material) {
        mat.base_color = color;
        mat.unlit = true;
        mat.cull_mode = Some(Face::Front);
    }
    outline.enabled = enabled;
    outline.width = width.clamp(0.0, 0.25);
    outline.color = color;
}

pub fn setup_fps_text(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(FpsUpdate {
        timer: Timer::from_seconds(1.0, TimerMode::Repeating),
        cached_fps: 0.0,
    });

    commands.spawn((
        Text::new(""),
        TextFont {
            // This font is loaded and will be used instead of the default font.
            font: asset_server.load("fonts/Roboto_static_regular.ttf"),
            font_size: 16.0,
            ..default()
        },
        FpsText,
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(15.0),
            right: Val::Px(15.0),
            ..default()
        },
    ));
}

pub fn update_fps_text(
    time: Res<Time>,
    diagnostics: Res<DiagnosticsStore>,
    mut q: Query<&mut Text, With<FpsText>>,
    mut upd: ResMut<FpsUpdate>,
) {
    upd.timer.tick(time.delta());

    // Only refresh the cached numbers once per second
    if upd.timer.finished() {
        if let Ok(mut text) = q.single_mut() {
            if let Some(fps) = diagnostics
                .get(&FrameTimeDiagnosticsPlugin::FPS)
                .and_then(|d| d.smoothed())
            {
                upd.cached_fps = fps;
            }

            text.0 = format!("{:.0}", upd.cached_fps);
        }
    }
}
