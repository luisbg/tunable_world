#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

use bevy::{
    diagnostic::FrameTimeDiagnosticsPlugin,
    math::primitives::{Cuboid, Plane3d, Sphere},
    pbr::NotShadowCaster,
    prelude::*,
    render::render_resource::Face,
};
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};

mod camera;
mod inspector;
mod post;

use crate::camera::{
    OrbitSet, orbit_camera_hotkeys, orbit_camera_rotate_continuous, orbit_snap_to_index,
    spawn_camera,
};
use crate::inspector::{Editable, EditableMesh, InspectorPlugin, SpawnKind};
use crate::post::chroma_aberration::ChromaAberrationPlugin;
use crate::post::crt::CRTPlugin;
use crate::post::gradient_tint::GradientTintPlugin;
use crate::post::lut::{LutPlugin, lut_apply_pending};
use crate::post::outlines::{OutlineParams, OutlineShell, spawn_outlined, update_outlines};
use crate::post::ui::{post_process_edit_panel, setup_fps_text, update_fps_text};

/// Global UI state for toggling panels like the Inspector.
#[derive(Resource)]
pub struct SceneEditState {
    pub open: bool,
}

impl Default for SceneEditState {
    fn default() -> Self {
        Self { open: true }
    }
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
        .add_plugins(LutPlugin)
        // UI plugin (egui)
        .add_plugins(EguiPlugin::default())
        .add_plugins(InspectorPlugin)
        .init_resource::<SceneEditState>()
        .add_systems(Startup, (spawn_camera, spawn_light, spawn_scene))
        .add_systems(PostStartup, setup_fps_text)
        .add_systems(EguiPrimaryContextPass, post_process_edit_panel)
        .configure_sets(Update, (OrbitSet::Input, OrbitSet::Pose).chain())
        .add_systems(
            Update,
            (
                update_outlines,
                update_fps_text,
                orbit_camera_hotkeys.in_set(OrbitSet::Input),
                orbit_snap_to_index.in_set(OrbitSet::Pose),
                orbit_camera_rotate_continuous.in_set(OrbitSet::Pose),
                lut_apply_pending,
                space_closes_scene_inspector,
                esc_quits_app,
            ),
        )
        .run();
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
            collider: Some(true),
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
            collider: Some(true),
        },
        Name::new("Water"),
    ));
}

/// Hide the Scene Editor UI when Spacebar is pressed.
fn space_closes_scene_inspector(kb: Res<ButtonInput<KeyCode>>, mut state: ResMut<SceneEditState>) {
    if kb.just_pressed(KeyCode::Space) {
        state.open = !state.open;
    }
}

/// Quit the whole app on Escape.
fn esc_quits_app(kb: Res<ButtonInput<KeyCode>>, mut exit: EventWriter<bevy::app::AppExit>) {
    if kb.just_pressed(KeyCode::Escape) {
        exit.write(bevy::app::AppExit::Success);
    }
}
