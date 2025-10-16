use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy::{
    core_pipeline::{
        bloom::Bloom,
        dof::{DepthOfField, DepthOfFieldMode},
        tonemapping::Tonemapping,
    },
    math::primitives::{Cuboid, Plane3d, Sphere},
    pbr::{DistanceFog, FogFalloff, NotShadowCaster, ScreenSpaceAmbientOcclusion},
    render::render_resource::Face,
};
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};

mod inspector;
mod post;

use crate::inspector::{Editable, EditableMesh, InspectorPlugin, SpawnKind};
use crate::post::chroma_aberration::{ChromaAberrationPlugin, ChromaAberrationSettings};
use crate::post::crt::{CRTPlugin, CRTSettings};
use crate::post::gradient_tint::{GradientTintPlugin, GradientTintSettings};
use crate::post::outlines::{OutlineParams, OutlineShell, spawn_outlined, update_outlines};

// Rotation speed (radians per second). ~0.8 rad/s ≈ 45.8°/s.
const ANGULAR_SPEED: f32 = 0.8;

#[derive(Component)]
struct FpsText;

#[derive(Resource)]
struct FpsUpdate {
    timer: Timer,
    cached_fps: f64,
}

/// Tag the camera we want to orbit around the target.
#[derive(Component)]
struct OrbitCamera {
    // Point the camera looks at
    target: Vec3,
    // Keep track of which preset we snapped to, 0 => 12, 1 => 3, 2 => 6, 3 => 9
    index_4: i32,
    // Base yaw offset; use PI/4 for isometric diagonals
    yaw_offset_rad: f32,
    // Continuous offset modified by A/D
    yaw_extra_rad: f32,
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "MVS".into(), // Monument Valley-style Bevy World
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            FrameTimeDiagnosticsPlugin::default(), // collects fps and frame time
        ))
        .add_plugins(ChromaAberrationPlugin)
        .add_plugins(CRTPlugin)
        .add_plugins(GradientTintPlugin)
        // UI plugin (egui)
        .add_plugins(EguiPlugin::default())
        .add_plugins(InspectorPlugin)
        .add_systems(Startup, (spawn_camera, spawn_light, spawn_scene))
        .add_systems(PostStartup, setup_fps_text)
        .add_systems(EguiPrimaryContextPass, post_process_edit_panel)
        .add_systems(
            Update,
            (
                update_outlines,
                update_fps_text,
                orbit_camera_hotkeys,
                orbit_snap_to_index,
                orbit_camera_rotate_continuous,
            ),
        )
        .run();
}

/// HDR camera with bloom, filmic tonemapping, gentle DoF-like vibe via composition.
/// (Real DoF is optional; this keeps it simple and solid for 0.16.)
fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d { ..default() },
        Transform::from_xyz(9.0, 9.0, 13.0).looking_at(Vec3::new(3.0, 1.0, 2.5), Vec3::Y),
        Tonemapping::AcesFitted, // nice highlight rolloff
        // Subtle bloom; keep defaults tasteful. Use emissive accents to trigger it.
        Bloom::default(),
        // Soft, shallow fog for diorama depth
        DistanceFog {
            color: LinearRgba::from(Color::srgb(0.86, 0.90, 0.96)).into(),
            falloff: FogFalloff::Exponential { density: 0.035 },
            // push fog slightly “in” so background softens
            // (distance is relative to camera; play with value range in your game space)
            ..default()
        },
        DepthOfField {
            mode: DepthOfFieldMode::Bokeh, // or Gaussian
            focal_distance: 15.0,          // distance from camera to focus band
            aperture_f_stops: 0.2,         // lower = blurrier
            sensor_height: 0.01866,        // Super 35 default
            ..default()
        },
        // Extremely light SSAO helps creases without mud (optional; safe default)
        ScreenSpaceAmbientOcclusion::default(),
        Msaa::Off,
        // Add the setting to the camera.
        // This component is also used to determine on which camera to run the post processing effect.
        ChromaAberrationSettings {
            enabled: 1,
            intensity: 0.002,
        },
        CRTSettings {
            enabled: 1,
            intensity: 0.025,
            scanline_freq: 202.5,
            line_intensity: 0.1,
        },
        GradientTintSettings {
            enabled: 1,
            additive: 0,
            strength: 0.5,
            color_top_right: Vec4::new(0.9, 0.2, 0.3, 1.0), // pink-tint
            color_bottom_left: Vec4::new(0.2, 0.9, 0.8, 1.0), // cyan-tint
        },
        OrbitCamera {
            target: Vec3::ZERO,
            index_4: 0,                                  // 0..3 → 1:30, 4:30, 7:30, 10:30
            yaw_offset_rad: std::f32::consts::FRAC_PI_4, // 45°
            yaw_extra_rad: 0.0,
        },
        Name::new("MainCamera"),
    ));
}

/// Single sunny key light with shadows; modest intensity, warm hue.
/// Keep it simple and let the tonemapper/bloom do the glam.
fn spawn_light(mut commands: Commands) {
    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.92, 0.95, 1.0),
        brightness: 200.0, // low ambient: lets sun + fog shape the scene
        ..default()
    });

    commands.spawn((
        DirectionalLight {
            illuminance: 10_000.0, // outdoor sun-ish
            shadows_enabled: true,
            shadow_depth_bias: 0.02,
            ..default()
        },
        // 3/4 top-down angle
        Transform::from_rotation(Quat::from_euler(
            EulerRot::XYZ,
            (-38.0_f32).to_radians(),
            35.0_f32.to_radians(),
            0.0,
        )),
        Name::new("Sun"),
    ));
}

/// Chunky “terraced” ground at a few heights + a tiny emissive accent.
fn spawn_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // --- Palette (gentle pastels, mostly rough)
    let grass_a = materials.add(StandardMaterial {
        base_color: Color::srgb(126.0 / 255.0, 171.0 / 255.0, 139.0 / 255.0),
        perceptual_roughness: 0.85,
        metallic: 0.0,
        ..default()
    });
    let grass_b = materials.add(StandardMaterial {
        base_color: Color::srgb(0.58, 0.79, 0.64),
        perceptual_roughness: 0.9,
        ..default()
    });
    let dirt = materials.add(StandardMaterial {
        base_color: Color::srgb(0.72, 0.64, 0.54),
        perceptual_roughness: 0.95,
        ..default()
    });
    let stone = materials.add(StandardMaterial {
        base_color: Color::srgb(0.76, 0.78, 0.82),
        perceptual_roughness: 0.8,
        ..default()
    });
    // Emissive “glow” accent for the bloom to catch (keep base dark so bloom pops)
    let crystal = materials.add(StandardMaterial {
        base_color: Color::BLACK,
        emissive: LinearRgba::from(Color::srgb(0.75, 0.95, 1.0)) * 2.5, // try 1.5–3.0
        perceptual_roughness: 0.1,
        metallic: 0.0,
        ..default()
    });

    // Shared outline material (front-face culled so backfaces show; unlit for flat color)
    let outline_color = Color::srgb(0.08, 0.10, 0.12);
    let outline_material = materials.add(StandardMaterial {
        base_color: outline_color,
        unlit: true,
        cull_mode: Some(Face::Front),
        // keep depth test/write default so it hugs the mesh properly
        ..default()
    });

    // Make outline settings globally available (egui will edit these)
    commands.insert_resource(OutlineParams {
        enabled: true,
        width: 0.02,
        color: outline_color,
        material: outline_material.clone(),
    });

    // --- Mesh prims
    let plane = meshes.add(Mesh::from(Plane3d::default()));
    let step = meshes.add(Mesh::from(Cuboid::new(1.0, 1.0, 1.0)));
    let slab = meshes.add(Mesh::from(Cuboid::new(1.0, 1.0, 1.0)));
    let block = meshes.add(Mesh::from(Cuboid::new(1.0, 1.0, 1.0)));
    let sphere = meshes.add(Mesh::from(Sphere::new(0.5)));

    // --- Base ground (big plane) – slightly tilted camera gives the “tabletop” feel
    commands.spawn((
        Mesh3d(plane.clone()),
        MeshMaterial3d(grass_a.clone()),
        Transform::from_scale(Vec3::splat(30.0)), // large base
        Editable,
        Name::new("BaseGround"),
        EditableMesh {
            kind: SpawnKind::Plane,
        },
    ));

    // --- Terraces: a few chunky steps at different heights
    // Left terrace (low)
    spawn_outlined(
        &mut commands,
        step.clone(),
        grass_b.clone(),
        Transform::from_xyz(-2.5, 0.3, 1.0).with_scale(Vec3::new(4.0, 0.6, 4.0)),
        outline_material.clone(),
        0.03,
        "TerraceLow",
        SpawnKind::Cuboid,
    );

    // Mid terrace
    spawn_outlined(
        &mut commands,
        step.clone(),
        grass_a.clone(),
        Transform::from_xyz(1.5, 0.3, -0.5).with_scale(Vec3::new(4.0, 0.6, 4.0)),
        outline_material.clone(),
        0.03,
        "TerraceMid",
        SpawnKind::Cuboid,
    );

    // Tall terrace (stacked)
    let high = spawn_outlined(
        &mut commands,
        step.clone(),
        grass_b.clone(),
        Transform::from_xyz(5.0, 0.95, 3.5).with_scale(Vec3::new(4.0, 0.6, 4.0)),
        outline_material.clone(),
        0.03,
        "TerraceHighBase",
        SpawnKind::Cuboid,
    );
    // Cap (outlined)
    commands.entity(high).with_children(|c| {
        c.spawn((
            Mesh3d(slab.clone()),
            MeshMaterial3d(dirt.clone()),
            Transform::from_xyz(0.0, 1.0, 0.0),
            Name::new("TerraceHighCap"),
        ));
        c.spawn((
            Mesh3d(slab.clone()),
            MeshMaterial3d(outline_material.clone()),
            Transform::from_xyz(0.0, 1.0, 0.0).with_scale(Vec3::new(1.03, 1.03, 1.03)),
            NotShadowCaster,
            OutlineShell,
            Name::new("TerraceHighCap_Outline"),
        ));
    });

    // --- A few “stone” blocks to catch highlights (bevel-ish via lighting)
    for (i, &(dx, dz)) in [(-1.0, 0.0), (0.0, 1.0), (1.0, -1.0), (2.0, 2.0)]
        .iter()
        .enumerate()
    {
        spawn_outlined(
            &mut commands,
            block.clone(),
            stone.clone(),
            Transform::from_xyz(2.0 + dx as f32 * 0.9, 0.5, 1.5 + dz),
            outline_material.clone(),
            0.03,
            &format!("Stone{i}"),
            SpawnKind::Cuboid,
        );
    }

    // --- Emissive “crystal” on the mid terrace so bloom has a target
    spawn_outlined(
        &mut commands,
        sphere.clone(),
        crystal,
        Transform::from_xyz(1.5, 0.65, -0.5).with_scale(Vec3::new(0.6, 0.6, 0.6)),
        outline_material.clone(),
        0.03,
        "Crystal",
        SpawnKind::Sphere,
    );

    // --- A thin “water” slab (very light roughness so the sun sparkles a bit)
    let water = materials.add(StandardMaterial {
        base_color: Color::srgba(0.55, 0.85, 0.95, 0.8),
        perceptual_roughness: 0.05,
        metallic: 0.0,
        reflectance: 0.02,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    commands.spawn((
        Mesh3d(meshes.add(Mesh::from(Cuboid::new(3.5, 0.02, 2.2)))),
        MeshMaterial3d(water),
        Transform::from_xyz(-2.7, 0.02, -2.3),
        // water usually shouldn’t cast shadows in this simple setup
        NotShadowCaster,
        Editable,
        EditableMesh {
            kind: SpawnKind::Cuboid,
        },
        Name::new("Water"),
    ));
}

/// egui panel: tune post-processing effects
fn post_process_edit_panel(
    mut ctxs: EguiContexts,
    mut q_cam: Query<(&mut DepthOfField, &GlobalTransform), With<Camera3d>>,
    mut outline: ResMut<OutlineParams>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    (mut chroma_settings, mut crt_settings, mut gradient_tint_settings): (
        Query<&mut ChromaAberrationSettings>,
        Query<&mut CRTSettings>,
        Query<&mut GradientTintSettings>,
    ),
) {
    let Ok((mut dof, cam_xform)) = q_cam.single_mut() else {
        return;
    };

    // Local copies so sliders can edit smoothly
    let mut focal_distance = dof.focal_distance;
    let mut f_stops = dof.aperture_f_stops;
    let mut bokeh = matches!(dof.mode, DepthOfFieldMode::Bokeh);

    let mut enabled = outline.enabled;
    let mut width = outline.width;
    let mut color = outline.color;

    egui::Window::new("Effect settings")
        .default_width(300.0)
        .show(ctxs.ctx_mut().expect("single egui context"), |ui| {
            ui.heading("Depth of Field");
            ui.add(egui::Slider::new(&mut focal_distance, 1.0..=40.0).text("Focal distance"));
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

            ui.separator();

            ui.heading("Outline");
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

            ui.separator();

            ui.heading("Chromatic Aberration");
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

            ui.separator();

            ui.heading("CRT");
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

            ui.separator();

            ui.heading("Gradient Tint");
            if let Ok(mut gt) = gradient_tint_settings.single_mut() {
                let mut on = gt.enabled != 0;
                let mut additive = gt.additive != 0;
                let color_top_right = gt.color_top_right;
                let color_bottom_left = gt.color_bottom_left;

                ui.add(
                    egui::Slider::new(&mut gt.strength, 0.0..=1.0)
                        .logarithmic(false)
                        .text("Intensity"),
                );
                let mut rgb = [color_top_right[0], color_top_right[1], color_top_right[2]];
                if ui.color_edit_button_rgb(&mut rgb).changed() {
                    gt.color_top_right = Vec4::new(rgb[0], rgb[1], rgb[2], 1.0);
                }
                rgb = [
                    color_bottom_left[0],
                    color_bottom_left[1],
                    color_bottom_left[2],
                ];
                if ui.color_edit_button_rgb(&mut rgb).changed() {
                    gt.color_bottom_left = Vec4::new(rgb[0], rgb[1], rgb[2], 1.0);
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

fn setup_fps_text(mut commands: Commands, asset_server: Res<AssetServer>) {
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

fn update_fps_text(
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

/// Helper: compute the *local* transform that looks at `target` with `up = Vec3::Y`,
/// at a specific desired world-space position.
fn look_from(pos: Vec3, target: Vec3) -> Transform {
    Transform::from_translation(pos).looking_at(target, Vec3::Y)
}

/// Snap camera to one of the 4 clock angles around +Y, preserving the current distance and height.
fn orbit_snap_to_index(mut q_cam: Query<(&mut Transform, &mut OrbitCamera), With<Camera3d>>) {
    for (mut tf, ocam) in &mut q_cam {
        let target = ocam.target;

        // Current distance from target
        let offset = tf.translation - target;
        let dist = offset.length().max(0.0001);
        let y = offset.y; // keep current height
        // radial distance in XZ required to preserve the same 3D distance
        let r_xy = (dist * dist - y * y).max(0.0).sqrt();

        // Diagonals: base at 45 degress then 90 degree steps → 1:30, 4:30, 7:30, 10:30
        let angle = ocam.yaw_offset_rad
            + (ocam.index_4.rem_euclid(4) as f32) * std::f32::consts::FRAC_PI_2
            + ocam.yaw_extra_rad;
        let x = r_xy * angle.cos();
        let z = r_xy * angle.sin();

        let pos = Vec3::new(x, y, z) + target;
        *tf = look_from(pos, target);
    }
}

/// Hotkeys to snap the camera:
/// 1 / 2 / 3 / 4  => 12 / 3 / 6 / 9 o'clock
/// Q / E          => rotate left / right by 90 degrees
fn orbit_camera_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    mut q_cam: Query<(&mut Transform, &mut OrbitCamera), With<Camera3d>>,
) {
    // Early out if no relevant key pressed
    let any = keys.just_pressed(KeyCode::Digit1)
        || keys.just_pressed(KeyCode::Digit2)
        || keys.just_pressed(KeyCode::Digit3)
        || keys.just_pressed(KeyCode::Digit4)
        || keys.just_pressed(KeyCode::KeyQ)
        || keys.just_pressed(KeyCode::KeyE);
    if !any {
        return;
    }

    for (mut tf, mut ocam) in &mut q_cam {
        // Determine current y & distance once per press (preserve them across snaps)
        let target = ocam.target;
        let offset = tf.translation - target;
        let dist = offset.length().max(0.0001);
        let y = offset.y;

        // Update index based on input
        if keys.just_pressed(KeyCode::Digit1) {
            ocam.index_4 = 0;
            ocam.yaw_extra_rad = 0.0;
        }
        if keys.just_pressed(KeyCode::Digit2) {
            ocam.index_4 = 1;
            ocam.yaw_extra_rad = 0.0;
        }
        if keys.just_pressed(KeyCode::Digit3) {
            ocam.index_4 = 2;
            ocam.yaw_extra_rad = 0.0;
        }
        if keys.just_pressed(KeyCode::Digit4) {
            ocam.index_4 = 3;
            ocam.yaw_extra_rad = 0.0;
        }
        if keys.just_pressed(KeyCode::KeyQ) {
            ocam.index_4 -= 1;
            ocam.yaw_extra_rad = 0.0;
        }
        if keys.just_pressed(KeyCode::KeyE) {
            ocam.index_4 += 1;
            ocam.yaw_extra_rad = 0.0;
        }

        // Compute new position on the ring at same distance & height
        let r_xy = (dist * dist - y * y).max(0.0).sqrt();
        let angle = ocam.yaw_offset_rad
            + (ocam.index_4.rem_euclid(4) as f32) * std::f32::consts::FRAC_PI_2
            + ocam.yaw_extra_rad;
        let x = r_xy * angle.cos();
        let z = r_xy * angle.sin();

        let pos = Vec3::new(x, y, z) + target;

        // Point at target with up=Y
        *tf = look_from(pos, target);
    }
}

fn orbit_camera_rotate_continuous(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut q_cam: Query<(&mut Transform, &mut OrbitCamera), With<Camera3d>>,
) {
    let left = keys.pressed(KeyCode::KeyA);
    let right = keys.pressed(KeyCode::KeyD);
    if !(left || right) {
        return;
    }

    let dt = time.delta_secs();
    let dir = (right as i32 - left as i32) as f32; // right = +1, left = -1

    for (mut tf, mut ocam) in &mut q_cam {
        // Update extra yaw, wrap around TAU just to keep it bounded
        ocam.yaw_extra_rad =
            (ocam.yaw_extra_rad + dir * ANGULAR_SPEED * dt) % std::f32::consts::TAU;

        // Keep current height & distance
        let target = ocam.target;
        let offset = tf.translation - target;
        let dist = offset.length().max(0.0001);
        let y = offset.y;
        let r_xy = (dist * dist - y * y).max(0.0).sqrt();

        // Total angle = diagonal base + 90° steps + continuous extra
        let angle = ocam.yaw_offset_rad
            + (ocam.index_4.rem_euclid(4) as f32) * std::f32::consts::FRAC_PI_2
            + ocam.yaw_extra_rad;

        let x = r_xy * angle.cos();
        let z = r_xy * angle.sin();
        let pos = Vec3::new(x, y, z) + target;

        *tf = Transform::from_translation(pos).looking_at(target, Vec3::Y);
    }
}
