use bevy::input::mouse::MouseButtonInput;
use bevy::math::Vec3A;
use bevy::prelude::*;
use bevy::render::primitives::Aabb;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

/// Tag any entity you want to be clickable/editable.
#[derive(Component)]
pub struct Editable;

/// Tag the currently selected entity (helps for highlighting, if you want).
#[derive(Component)]
pub struct Selected;

/// Keeps UI state and the currently selected entity.
#[derive(Resource, Default)]
struct InspectorState {
    selected: Option<Entity>,
    // Cached UI fields (what the user is editing)
    pos: Vec3,
    scale: Vec3,
    window_open: bool,
}

/// Plugin to wire everything up.
pub struct InspectorPlugin;
impl Plugin for InspectorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InspectorState>()
            .add_systems(Update, pick_on_click)
            .add_systems(EguiPrimaryContextPass, inspector_window);
    }
}

/// Ray-AABB intersection helper (slab method). Returns Some(t) if hit; t is entry distance.
fn ray_aabb_intersection(origin: Vec3, dir: Vec3, aabb_min: Vec3, aabb_max: Vec3) -> Option<f32> {
    // Avoid div by zero; replace zero components with a small epsilon.
    let eps = 1e-8;
    let inv_dir = Vec3::new(
        1.0 / (if dir.x.abs() < eps {
            eps.copysign(dir.x)
        } else {
            dir.x
        }),
        1.0 / (if dir.y.abs() < eps {
            eps.copysign(dir.y)
        } else {
            dir.y
        }),
        1.0 / (if dir.z.abs() < eps {
            eps.copysign(dir.z)
        } else {
            dir.z
        }),
    );

    let mut t1 = (aabb_min.x - origin.x) * inv_dir.x;
    let mut t2 = (aabb_max.x - origin.x) * inv_dir.x;
    let mut tmin = t1.min(t2);
    let mut tmax = t1.max(t2);

    t1 = (aabb_min.y - origin.y) * inv_dir.y;
    t2 = (aabb_max.y - origin.y) * inv_dir.y;
    tmin = tmin.max(t1.min(t2));
    tmax = tmax.min(t1.max(t2));

    t1 = (aabb_min.z - origin.z) * inv_dir.z;
    t2 = (aabb_max.z - origin.z) * inv_dir.z;
    tmin = tmin.max(t1.min(t2));
    tmax = tmax.min(t1.max(t2));

    if tmax >= tmin.max(0.0) {
        Some(tmin.max(0.0))
    } else {
        None
    }
}

/// Transform a local-space AABB to world space using the entity's GlobalTransform.
/// Works for any combination of rotation + non-uniform scale + translation.
fn aabb_world(local: Aabb, global: &GlobalTransform) -> Aabb {
    // Affine3A = [ R*S | t ]
    let aff = global.affine();
    let m = aff.matrix3; // Mat3A (rotation * scale)
    let t = aff.translation; // Vec3A

    // Original center/half-extents
    let c_local = Vec3A::from(local.center);
    let he = local.half_extents;

    // World center: R*S*c + t
    let c_world = m * c_local + t;

    // World half-extents: abs(R*S) * he  (column-wise absolute)
    let x = m.x_axis; // column 0
    let y = m.y_axis; // column 1
    let z = m.z_axis; // column 2
    let he_world = Vec3::new(
        x.x.abs() * he.x + y.x.abs() * he.y + z.x.abs() * he.z,
        x.y.abs() * he.x + y.y.abs() * he.y + z.y.abs() * he.z,
        x.z.abs() * he.x + y.z.abs() * he.y + z.z.abs() * he.z,
    );

    Aabb {
        center: c_world.into(),
        half_extents: he_world.into(),
    }
}

/// On left-click in the 3D viewport, cast a ray and select the closest hit Editable entity.
fn pick_on_click(
    mut ev_mousebtn: EventReader<MouseButtonInput>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    mut state: ResMut<InspectorState>,
    mut commands: Commands,
    mut q_selected: Query<Entity, With<Selected>>,
    //    q_editables: Query<(Entity, &GlobalTransform, &Mesh3d), With<Editable>>,
    q_editables: Query<(Entity, &GlobalTransform, &Aabb), With<Editable>>,
    //    meshes: Res<Assets<Mesh>>,
    mut egui_ctxs: EguiContexts,
) {
    // Only act on left button press events
    let clicked = ev_mousebtn
        .read()
        .any(|e| e.button == MouseButton::Left && e.state.is_pressed());
    if !clicked {
        return;
    }

    // If egui wants the pointer, don't pick (prevents UI clicks selecting scene).
    if egui_ctxs
        .ctx_mut()
        .expect("single egui context")
        .wants_pointer_input()
    {
        return;
    }

    // Get primary window & any active 3D camera with a cursor position available.
    let window = match windows.single() {
        Ok(w) => w,
        Err(_) => return,
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };

    let mut best_hit: Option<(Entity, f32)> = None;

    // Try each camera until one gives us a world ray for this cursor pos.
    for (camera, cam_xform) in cameras.iter() {
        if !camera.is_active {
            continue;
        }
        // Convert screen cursor to a world ray (origin + direction)
        let Ok(ray) = camera.viewport_to_world(cam_xform, cursor_pos) else {
            continue;
        };
        let origin = ray.origin;
        let dir = ray.direction;

        // Test against all editables using their world AABB
        for (e, global, aabb) in q_editables.iter() {
            let world_aabb = aabb_world(*aabb, global);
            let min = world_aabb.center - world_aabb.half_extents;
            let max = world_aabb.center + world_aabb.half_extents;

            if let Some(t) = ray_aabb_intersection(origin, *dir, min.into(), max.into()) {
                // Keep the nearest hit
                if best_hit.map_or(true, |(_, best_t)| t < best_t) {
                    best_hit = Some((e, t));
                }
            }
        }

        // If this camera produced any hit, commit selection and stop checking other cameras.
        if let Some((hit_e, _t)) = best_hit {
            // Clear previous selection tag, if any
            if let Ok(prev) = q_selected.single_mut() {
                commands.entity(prev).remove::<Selected>();
            }
            // Tag new selection
            commands.entity(hit_e).insert(Selected);

            // Initialize inspector state for UI
            state.selected = Some(hit_e);
            state.window_open = true;
            return;
        }
    }
}

/// egui window that shows when an entity is selected. Edits translation & scale live.
fn inspector_window(
    mut state: ResMut<InspectorState>,
    mut egui_ctxs: EguiContexts,
    mut q_tf: Query<&mut Transform>,
) {
    let Some(entity) = state.selected else { return };

    // Load current values from Transform when opening, then keep editing the cached fields.
    if let Ok(tf) = q_tf.get_mut(entity) {
        // If the window was just opened this frame (or we don't have cache yet), sync cache
        // This keeps the cache in sync if the entity was transformed by other systems.
        if !state.window_open {
            state.pos = tf.translation;
            state.scale = tf.scale;
            state.window_open = true;
        } else if state.pos == Vec3::ZERO && state.scale == Vec3::ZERO {
            state.pos = tf.translation;
            state.scale = tf.scale;
        }
    }

    let ctx = egui_ctxs.ctx_mut().expect("single egui context");
    let mut open = state.window_open;
    egui::Window::new("Object Inspector")
        .open(&mut open)
        .resizable(true)
        .collapsible(true)
        .show(ctx, |ui| {
            ui.label("Edit the selected object’s transform");

            ui.separator();
            ui.heading("Position");
            ui.horizontal(|ui| {
                ui.label("x");
                ui.add(egui::DragValue::new(&mut state.pos.x).speed(0.05));
                ui.label("y");
                ui.add(egui::DragValue::new(&mut state.pos.y).speed(0.05));
                ui.label("z");
                ui.add(egui::DragValue::new(&mut state.pos.z).speed(0.05));
            });

            ui.heading("Scale");
            ui.horizontal(|ui| {
                ui.label("x");
                ui.add(
                    egui::DragValue::new(&mut state.scale.x)
                        .speed(0.02)
                        .range(0.001..=1000.0),
                );
                ui.label("y");
                ui.add(
                    egui::DragValue::new(&mut state.scale.y)
                        .speed(0.02)
                        .range(0.001..=1000.0),
                );
                ui.label("z");
                ui.add(
                    egui::DragValue::new(&mut state.scale.z)
                        .speed(0.02)
                        .range(0.001..=1000.0),
                );
            });

            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Reset Pos").clicked() {
                    state.pos = Vec3::ZERO;
                }
                if ui.button("Reset Scale (1,1,1)").clicked() {
                    state.scale = Vec3::ONE;
                }
            });
            ui.small("Tip: hold Shift for finer DragValue steps");
        });

    // Apply changes live while open
    if open {
        if let Ok(mut tf) = q_tf.get_mut(entity) {
            tf.translation = state.pos;
            tf.scale = state.scale;
        }
    } else {
        // Window closed by user
        state.window_open = false;
        // Keep the selection, but stop forcing cache
        state.pos = Vec3::ZERO;
        state.scale = Vec3::ZERO;
    }
}
