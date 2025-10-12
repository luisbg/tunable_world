use bevy::core_pipeline::bloom::Bloom;
use bevy::core_pipeline::dof::{DepthOfField, DepthOfFieldMode};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::math::primitives::{Cuboid, Plane3d, Sphere};
use bevy::pbr::ScreenSpaceAmbientOcclusion;
use bevy::pbr::{DistanceFog, FogFalloff, NotShadowCaster};
use bevy::prelude::*;
use bevy::render::mesh::Mesh;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "MVS".into(), // Monument Valley-style Bevy World
                ..default()
            }),
            ..default()
        }))
        // UI plugin (egui)
        .add_plugins(EguiPlugin::default())
        .add_systems(Startup, (spawn_camera, spawn_light, spawn_scene))
        .add_systems(EguiPrimaryContextPass, dof_egui_panel)
        .run();
}

/// HDR camera with bloom, filmic tonemapping, gentle DoF-like vibe via composition.
/// (Real DoF is optional; this keeps it simple and solid for 0.16.)
fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d { ..default() },
        Camera {
            hdr: true,
            ..default()
        },
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

    // --- Mesh prims
    let plane = meshes.add(Mesh::from(Plane3d::default()));
    let step = meshes.add(Mesh::from(Cuboid::new(4.0, 0.6, 4.0)));
    let slab = meshes.add(Mesh::from(Cuboid::new(4.0, 0.3, 4.0)));
    let block = meshes.add(Mesh::from(Cuboid::new(1.0, 1.0, 1.0)));
    let sphere = meshes.add(Mesh::from(Sphere::new(0.22)));

    // --- Base ground (big plane) – slightly tilted camera gives the “tabletop” feel
    commands.spawn((
        Mesh3d(plane.clone()),
        MeshMaterial3d(grass_a.clone()),
        Transform::from_scale(Vec3::splat(30.0)), // large base
        Name::new("BaseGround"),
    ));

    // --- Terraces: a few chunky steps at different heights
    // Left terrace (low)
    commands.spawn((
        Mesh3d(step.clone()),
        MeshMaterial3d(grass_b.clone()),
        Transform::from_xyz(-2.5, 0.3, 1.0),
        Name::new("TerraceLow"),
    ));

    // Mid terrace
    commands.spawn((
        Mesh3d(step.clone()),
        MeshMaterial3d(grass_a.clone()),
        Transform::from_xyz(1.5, 0.3, -0.5),
        Name::new("TerraceMid"),
    ));

    // Tall terrace (stacked)
    commands.spawn((
        Mesh3d(step.clone()),
        MeshMaterial3d(grass_b.clone()),
        Transform::from_xyz(5.0, 0.3, 3.5),
        Name::new("TerraceHighBase"),
    ));
    commands.spawn((
        Mesh3d(slab.clone()),
        MeshMaterial3d(dirt.clone()),
        Transform::from_xyz(5.0, 0.75, 3.5),
        Name::new("TerraceHighCap"),
    ));

    // --- A few “stone” blocks to catch highlights (bevel-ish via lighting)
    for (i, &(dx, dz)) in [(-1.0, 0.0), (0.0, 1.0), (1.0, -1.0), (2.0, 2.0)]
        .iter()
        .enumerate()
    {
        commands.spawn((
            Mesh3d(block.clone()),
            MeshMaterial3d(stone.clone()),
            Transform::from_xyz(2.0 + dx as f32 * 0.9, 0.5, 1.5 + dz),
            Name::new(format!("Stone{}", i)),
        ));
    }

    // --- Emissive “crystal” on the mid terrace so bloom has a target
    commands.spawn((
        Mesh3d(sphere.clone()),
        MeshMaterial3d(crystal.clone()),
        Transform::from_xyz(1.5, 0.65, -0.5),
        Name::new("Crystal"),
    ));

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
        Name::new("Water"),
    ));
}

/// Small egui window to live-tune camera Depth of Field
fn dof_egui_panel(
    mut ctxs: EguiContexts,
    mut q_cam: Query<(&mut DepthOfField, &GlobalTransform), With<Camera3d>>,
) {
    let Ok((mut dof, cam_xform)) = q_cam.single_mut() else {
        return;
    };

    // Local copies so sliders can edit smoothly
    let mut focal_distance = dof.focal_distance;
    let mut f_stops = dof.aperture_f_stops;
    let mut bokeh = matches!(dof.mode, DepthOfFieldMode::Bokeh);

    egui::Window::new("Depth of Field")
        .default_width(260.0)
        .show(ctxs.ctx_mut().expect("single egui context"), |ui| {
            ui.label("Tune for a miniature/tilt-shift feel:");
            ui.add(egui::Slider::new(&mut focal_distance, 1.0..=40.0).text("Focal distance"));
            ui.add(
                egui::Slider::new(&mut f_stops, 0.01..=64.0)
                    .logarithmic(true)
                    .text("Aperture (f-stops)"),
            );
            ui.checkbox(&mut bokeh, "Bokeh mode (prettier, costlier)");
            ui.horizontal(|ui| {
                if ui.button("Snap focus to origin (0,0,0)").clicked() {
                    let cam_pos = cam_xform.translation();
                    focal_distance = cam_pos.length();
                }
                if ui.button("Reset").clicked() {
                    focal_distance = 8.0;
                    f_stops = 2.0;
                    bokeh = true;
                }
            });
        });

    // Apply changes back to the component
    dof.focal_distance = focal_distance.max(0.1);
    dof.aperture_f_stops = f_stops.clamp(0.01, 64.0);
    dof.mode = if bokeh {
        DepthOfFieldMode::Bokeh
    } else {
        DepthOfFieldMode::Gaussian
    };
}
