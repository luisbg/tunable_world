use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

const PLAYER_START: Vec3 = Vec3::new(0.0, 2.0, 0.0);
const PLAYER_SIZE: Vec2 = Vec2::new(0.25, 0.5);
const PLAYER_SPEED: f32 = 5.0; // speed units per second

#[derive(Component)]
pub struct Player;

#[derive(Component, Deref, DerefMut, Default)]
pub struct Velocity(pub Vec3);

pub fn spawn_player(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    let mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.82, 0.82, 0.86),
        perceptual_roughness: 0.6,
        metallic: 0.0,
        ..Default::default()
    });

    commands.spawn((
        Mesh3d(meshes.add(Capsule3d::new(PLAYER_SIZE.x, PLAYER_SIZE.y / 2.0))),
        MeshMaterial3d(mat),
        Transform::from_xyz(PLAYER_START.x, PLAYER_START.y, PLAYER_START.z),
        Player,
        Velocity(Vec3::ZERO),
        RigidBody::KinematicPositionBased,
        Collider::capsule_y(PLAYER_SIZE.x, PLAYER_SIZE.y / 2.0),
        KinematicCharacterController {
            up: Vec3::Y, // ground normal
            slide: true, // slide along walls
            // small gap to avoid jitter when pressed against geometry
            offset: CharacterLength::Absolute(0.02),
            // Snap a little to the ground if you use slopes/steps:
            // snap_to_ground: Some(CharacterLength::Absolute(0.1)),
            ..default()
        },
    ));
}

pub fn player_move(
    time: Res<Time>,
    cam_q: Query<&Transform, With<Camera3d>>,
    mut player_q: Query<(&Velocity, &mut KinematicCharacterController), With<Player>>,
) {
    let dt = time.delta_secs();
    let Ok(cam_tf) = cam_q.single() else {
        return;
    };

    // Camera-relative basis on ground plane (same as input system)
    let cam_rot = cam_tf.rotation;
    let cam_right = cam_rot * Vec3::X;
    let cam_forward = cam_rot * -Vec3::Z;

    let right_xz = Vec2::new(cam_right.x, cam_right.z).normalize();
    let forward_xz = Vec2::new(cam_forward.x, cam_forward.z).normalize();

    for (vel, mut kcc) in &mut player_q {
        // KCC expects per-frame translation
        let desired = Vec3::new(vel.0.x, 0.0, vel.0.z) * dt;

        // --- Classify motion direction in *camera space* ---
        let d2 = Vec2::new(desired.x, desired.z); // world motion on XZ
        let x_comp = d2.dot(right_xz); // +right / -left
        let y_comp = d2.dot(forward_xz); // +up (screen) / -down

        // Apply the *unrotated* world translation to KCC
        kcc.translation = Some(desired);
    }
}

pub fn player_input(
    keys: Res<ButtonInput<KeyCode>>,
    cam_q: Query<&Transform, With<Camera3d>>,
    mut q_player_vel: Query<&mut Velocity, With<Player>>,
) {
    let Ok(cam_tf) = cam_q.single() else {
        return;
    };

    // Gather raw input in "screen space" (x = right, y = up)
    let mut input = Vec2::ZERO;
    if keys.pressed(KeyCode::ArrowUp) || keys.pressed(KeyCode::KeyW) {
        input.y += 1.0;
    }
    if keys.pressed(KeyCode::ArrowDown) || keys.pressed(KeyCode::KeyS) {
        input.y -= 1.0;
    }
    if keys.pressed(KeyCode::ArrowLeft) || keys.pressed(KeyCode::KeyA) {
        input.x -= 1.0;
    }
    if keys.pressed(KeyCode::ArrowRight) || keys.pressed(KeyCode::KeyD) {
        input.x += 1.0;
    }

    let dir_world_xz = if input != Vec2::ZERO {
        // Normalize so diagonals aren't faster
        let input = input.normalize();

        // Camera-relative axes projected to the ground (XZ) plane.
        //    - "screen right" = camera's local +X
        //    - "screen up"    = camera's *forward* (camera looks along -Z), so use -Z
        let cam_rot = cam_tf.rotation;
        let cam_right = cam_rot * Vec3::X;
        let cam_forward = cam_rot * -Vec3::Z;

        let right_xz = Vec2::new(cam_right.x, cam_right.z).normalize();
        let forward_xz = Vec2::new(cam_forward.x, cam_forward.z).normalize();

        // Build world-space 2D direction on XZ
        (right_xz * input.x) + (forward_xz * input.y)
    } else {
        Vec2::ZERO
    };

    // Write a PER-SECOND velocity (do NOT multiply by delta here)
    for mut vel in &mut q_player_vel {
        vel.0 = Vec3::new(dir_world_xz.x, 0.0, dir_world_xz.y) * PLAYER_SPEED;
    }
}
