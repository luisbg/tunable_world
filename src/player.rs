use bevy::prelude::*;
use bevy_rapier3d::prelude::*;

const PLAYER_START: Vec3 = Vec3::new(0.0, 4.0, 0.0);
const PLAYER_SIZE: Vec2 = Vec2::new(0.25, 0.5);
const PLAYER_SPEED: f32 = 2.0; // speed units per second

const GRAVITY_Y: f32 = -24.0; // tune to taste
const TERMINAL_SPEED_Y: f32 = -50.0;

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
            snap_to_ground: Some(CharacterLength::Absolute(0.2)), // helps climb ramps & stay grounded
            autostep: Some(CharacterAutostep {
                max_height: CharacterLength::Absolute(0.4),
                min_width: CharacterLength::Absolute(0.2),
                include_dynamic_bodies: false,
            }),
            // Tweak slope angles to taste:
            max_slope_climb_angle: 50.0_f32.to_radians(),
            min_slope_slide_angle: 60.0_f32.to_radians(),
            ..default()
        },
    ));
}

// Sets X/Z from input
pub fn player_horizontal_velocity(
    mut q_player_vel: Query<&mut Velocity, With<Player>>,
    keys: Res<ButtonInput<KeyCode>>,
    cam_q: Query<&Transform, With<Camera3d>>,
) {
    let Ok(cam_tf) = cam_q.single() else {
        return;
    };

    let mut input = Vec2::ZERO;
    if keys.pressed(KeyCode::ArrowLeft) {
        input.x -= 1.0;
    }
    if keys.pressed(KeyCode::ArrowRight) {
        input.x += 1.0;
    }
    if keys.pressed(KeyCode::ArrowUp) {
        input.y += 1.0;
    }
    if keys.pressed(KeyCode::ArrowDown) {
        input.y -= 1.0;
    }
    if input.length_squared() > 1.0 {
        input = input.normalize();
    }

    // Camera-relative basis on ground plane
    let cam_rot = cam_tf.rotation;
    let cam_right = cam_rot * Vec3::X;
    let cam_forward = cam_rot * -Vec3::Z;

    let right_xz = Vec2::new(cam_right.x, cam_right.z).normalize_or_zero();
    let forward_xz = Vec2::new(cam_forward.x, cam_forward.z).normalize_or_zero();

    // World-space movement direction on XZ
    let dir_world_xz = (right_xz * input.x) + (forward_xz * input.y) * PLAYER_SPEED;

    for mut vel in &mut q_player_vel {
        let old_y = vel.y;
        vel.0 = Vec3::new(
            dir_world_xz.x * PLAYER_SPEED,
            old_y,
            dir_world_xz.y * PLAYER_SPEED,
        );
    }
}

// Integrates Y and pushes KCC
pub fn player_motion_with_gravity(
    time: Res<Time>,
    mut q: Query<
        (
            &mut Velocity,
            &mut KinematicCharacterController,
            Option<&KinematicCharacterControllerOutput>,
        ),
        With<Player>,
    >,
) {
    let dt = time.delta_secs();

    for (mut vel, mut kcc, output) in &mut q {
        // grounded info from previous KCC step (present after the first physics tick)
        let grounded = output.map(|o| o.grounded).unwrap_or(false);

        // gravity integration
        if grounded && vel.y < 0.0 {
            vel.y = 0.0; // clear accumulated downward speed
        } else {
            vel.y = (vel.y + GRAVITY_Y * dt).max(TERMINAL_SPEED_Y);
        }

        // The controller expects a displacement this frame.
        let frame_delta = **vel * dt;
        kcc.translation = Some(frame_delta);
    }
}
