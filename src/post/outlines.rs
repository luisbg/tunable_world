use bevy::pbr::NotShadowCaster;
use bevy::prelude::*;

use crate::inspector::Editable;

/// Tag on the outline child entity so we can update it en masse.
#[derive(Component)]
pub struct OutlineShell;

/// Outline settings (shared across all outlines).
#[derive(Resource)]
pub struct OutlineParams {
    pub enabled: bool,
    pub width: f32,   // uniform scale delta (0.0 => off, ~0.02–0.06 good)
    pub color: Color, // outline color
    pub material: Handle<StandardMaterial>,
}

/// Helper: spawn a mesh with an outline child.
pub fn spawn_outlined(
    commands: &mut Commands,
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    transform: Transform,
    outline_mat: Handle<StandardMaterial>,
    width: f32,
    name: &str,
) -> Entity {
    let parent = commands
        .spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            transform,
            Editable,
            Name::new(name.to_string()),
        ))
        .id();

    // Outline child: slightly larger backfaces-only, unlit
    commands.entity(parent).with_children(|c| {
        c.spawn((
            Mesh3d(mesh),
            MeshMaterial3d(outline_mat),
            Transform::from_scale(Vec3::splat(1.0 + width.max(0.0))),
            NotShadowCaster,
            OutlineShell,
            Name::new(format!("{name}_Outline")),
        ));
    });

    parent
}

/// Update all outline shells: scale for width; hide by scaling to zero if disabled.
pub fn update_outlines(
    outline: Res<OutlineParams>,
    mut q_shells: Query<&mut Transform, With<OutlineShell>>,
) {
    if !outline.is_changed() && q_shells.is_empty() {
        return;
    }
    let scale = if outline.enabled {
        1.0 + outline.width.max(0.0)
    } else {
        0.0 // effectively hides the outline without relying on Visibility API differences
    };
    for mut t in &mut q_shells {
        // Keep whatever translation/rotation they have; just adjust uniform scale
        let basis = t.scale.x.max(t.scale.y).max(t.scale.z);
        // If we previously hid it (0), basis could be 0; just set anew.
        let _ = basis; // not used further; set directly:
        t.scale = Vec3::splat(scale);
    }
}
