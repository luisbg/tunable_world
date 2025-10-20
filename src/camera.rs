use bevy::{
    core_pipeline::{
        bloom::Bloom,
        dof::{DepthOfField, DepthOfFieldMode},
        tonemapping::Tonemapping,
    },
    pbr::{DistanceFog, FogFalloff, ScreenSpaceAmbientOcclusion},
    prelude::*,
};

use crate::post::{
    chroma_aberration::ChromaAberrationSettings, crt::CRTSettings,
    gradient_tint::GradientTintSettings, lut::LutSettings,
};

// Rotation speed (radians per second). ~0.8 rad/s ≈ 45.8°/s.
const ANGULAR_SPEED: f32 = 0.8;

#[derive(Component)]
pub struct FpsText;

#[derive(Resource)]
pub struct FpsUpdate {
    pub timer: Timer,
    pub cached_fps: f64,
}

/// Tag the camera we want to orbit around the target.
#[derive(Component)]
pub struct OrbitCamera {
    // Point the camera looks at
    target: Vec3,
    // Keep track of which preset we snapped to, 0 => 12, 1 => 3, 2 => 6, 3 => 9
    index_4: i32,
    // Base yaw offset; use PI/4 for isometric diagonals
    yaw_offset_rad: f32,
    // Continuous offset modified by A/D
    yaw_extra_rad: f32,
}

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub enum OrbitSet {
    Input, // read keyboard; mutate state
    Pose,  // compute and write Transform once
}

/// Camera with bloom, filmic tonemapping, gentle DoF-like vibe.
pub fn spawn_camera(mut commands: Commands) {
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
        LutSettings {
            enabled: 1,
            strength: 1.0,
            lut_size: 16,
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

/// Helper: compute the *local* transform that looks at `target` with `up = Vec3::Y`,
/// at a specific desired world-space position.
fn look_from(pos: Vec3, target: Vec3) -> Transform {
    Transform::from_translation(pos).looking_at(target, Vec3::Y)
}

/// Snap camera to one of the 4 clock angles around +Y, preserving the current distance and height.
pub fn orbit_snap_to_index(mut q_cam: Query<(&mut Transform, &mut OrbitCamera), With<Camera3d>>) {
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
pub fn orbit_camera_hotkeys(
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

pub fn orbit_camera_rotate_continuous(
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
