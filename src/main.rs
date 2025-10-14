use bevy::prelude::*;
use bevy::{
    core_pipeline::{
        bloom::Bloom,
        core_3d::graph::{Core3d, Node3d},
        dof::{DepthOfField, DepthOfFieldMode},
        fullscreen_vertex_shader::fullscreen_shader_vertex_state,
        tonemapping::Tonemapping,
    },
    ecs::query::QueryItem,
    math::primitives::{Cuboid, Plane3d, Sphere},
    pbr::{DistanceFog, FogFalloff, NotShadowCaster, ScreenSpaceAmbientOcclusion},
    render::{
        RenderApp,
        extract_component::{
            ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin,
            UniformComponentPlugin,
        },
        render_graph::{
            NodeRunError, RenderGraphApp, RenderGraphContext, RenderLabel, ViewNode, ViewNodeRunner,
        },
        render_resource::{
            Face,
            binding_types::{sampler, texture_2d, uniform_buffer},
            *,
        },
        renderer::{RenderContext, RenderDevice},
        view::ViewTarget,
    },
};
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass, egui};

const SHADER_ASSET_PATH: &str = "shaders/post_processing.wgsl";

/// Tag on the outline child entity so we can update it en masse.
#[derive(Component)]
struct OutlineShell;

/// Outline settings (shared across all outlines).
#[derive(Resource)]
struct OutlineParams {
    enabled: bool,
    width: f32,   // uniform scale delta (0.0 => off, ~0.02–0.06 good)
    color: Color, // outline color
    material: Handle<StandardMaterial>,
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
        .add_plugins(PostProcessPlugin)
        // UI plugin (egui)
        .add_plugins(EguiPlugin::default())
        .add_systems(Startup, (spawn_camera, spawn_light, spawn_scene))
        .add_systems(EguiPrimaryContextPass, dof_and_outline_panel)
        .add_systems(Update, (update_outlines, tweak_ca_with_keyboard))
        .run();
}

struct PostProcessPlugin;

impl Plugin for PostProcessPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            // The settings will be a component that lives in the main world but will
            // be extracted to the render world every frame.
            // This makes it possible to control the effect from the main world.
            // This plugin will take care of extracting it automatically.
            // It's important to derive [`ExtractComponent`] on [`PostProcessingSettings`]
            // for this plugin to work correctly.
            ExtractComponentPlugin::<PostProcessSettings>::default(),
            // The settings will also be the data used in the shader.
            // This plugin will prepare the component for the GPU by creating a uniform buffer
            // and writing the data to that buffer every frame.
            UniformComponentPlugin::<PostProcessSettings>::default(),
        ));

        // We need to get the render app from the main app
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            // Bevy's renderer uses a render graph which is a collection of nodes in a directed acyclic graph.
            // It currently runs on each view/camera and executes each node in the specified order.
            // It will make sure that any node that needs a dependency from another node
            // only runs when that dependency is done.
            //
            // Each node can execute arbitrary work, but it generally runs at least one render pass.
            // A node only has access to the render world, so if you need data from the main world
            // you need to extract it manually or with the plugin like above.
            // Add a [`Node`] to the [`RenderGraph`]
            // The Node needs to impl FromWorld
            //
            // The [`ViewNodeRunner`] is a special [`Node`] that will automatically run the node for each view
            // matching the [`ViewQuery`]
            .add_render_graph_node::<ViewNodeRunner<PostProcessNode>>(
                // Specify the label of the graph, in this case we want the graph for 3d
                Core3d,
                // It also needs the label of the node
                PostProcessLabel,
            )
            .add_render_graph_edges(
                Core3d,
                // Specify the node ordering.
                // This will automatically create all required node edges to enforce the given ordering.
                (
                    Node3d::Tonemapping,
                    PostProcessLabel,
                    Node3d::EndMainPassPostProcessing,
                ),
            );
    }

    fn finish(&self, app: &mut App) {
        // We need to get the render app from the main app
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            // Initialize the pipeline
            .init_resource::<PostProcessPipeline>();
    }
}

#[derive(Component, Default, Clone, Copy, ExtractComponent, ShaderType)]
struct PostProcessSettings {
    intensity: f32,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct PostProcessLabel;

// The post process node used for the render graph
#[derive(Default)]
struct PostProcessNode;

// The ViewNode trait is required by the ViewNodeRunner
impl ViewNode for PostProcessNode {
    // The node needs a query to gather data from the ECS in order to do its rendering,
    // but it's not a normal system so we need to define it manually.
    //
    // This query will only run on the view entity
    type ViewQuery = (
        &'static ViewTarget,
        // This makes sure the node only runs on cameras with the PostProcessSettings component
        &'static PostProcessSettings,
        // As there could be multiple post processing components sent to the GPU (one per camera),
        // we need to get the index of the one that is associated with the current view.
        &'static DynamicUniformIndex<PostProcessSettings>,
    );

    // Runs the node logic
    // This is where you encode draw commands.
    //
    // This will run on every view on which the graph is running.
    // If you don't want your effect to run on every camera,
    // you'll need to make sure you have a marker component as part of [`ViewQuery`]
    // to identify which camera(s) should run the effect.
    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (view_target, _post_process_settings, settings_index): QueryItem<Self::ViewQuery>,
        world: &World,
    ) -> Result<(), NodeRunError> {
        // Get the pipeline resource that contains the global data we need
        // to create the render pipeline
        let post_process_pipeline = world.resource::<PostProcessPipeline>();

        // The pipeline cache is a cache of all previously created pipelines.
        // It is required to avoid creating a new pipeline each frame,
        // which is expensive due to shader compilation.
        let pipeline_cache = world.resource::<PipelineCache>();

        // Get the pipeline from the cache
        let Some(pipeline) = pipeline_cache.get_render_pipeline(post_process_pipeline.pipeline_id)
        else {
            return Ok(());
        };

        // Get the settings uniform binding
        let settings_uniforms = world.resource::<ComponentUniforms<PostProcessSettings>>();
        let Some(settings_binding) = settings_uniforms.uniforms().binding() else {
            return Ok(());
        };

        // This will start a new "post process write", obtaining two texture
        // views from the view target - a `source` and a `destination`.
        // `source` is the "current" main texture and you _must_ write into
        // `destination` because calling `post_process_write()` on the
        // [`ViewTarget`] will internally flip the [`ViewTarget`]'s main
        // texture to the `destination` texture. Failing to do so will cause
        // the current main texture information to be lost.
        let post_process = view_target.post_process_write();

        // The bind_group gets created each frame.
        //
        // Normally, you would create a bind_group in the Queue set,
        // but this doesn't work with the post_process_write().
        // The reason it doesn't work is because each post_process_write will alternate the source/destination.
        // The only way to have the correct source/destination for the bind_group
        // is to make sure you get it during the node execution.
        let bind_group = render_context.render_device().create_bind_group(
            "post_process_bind_group",
            &post_process_pipeline.layout,
            // It's important for this to match the BindGroupLayout defined in the PostProcessPipeline
            &BindGroupEntries::sequential((
                // Make sure to use the source view
                post_process.source,
                // Use the sampler created for the pipeline
                &post_process_pipeline.sampler,
                // Set the settings binding
                settings_binding.clone(),
            )),
        );

        // Begin the render pass
        let mut render_pass = render_context.begin_tracked_render_pass(RenderPassDescriptor {
            label: Some("post_process_pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                // We need to specify the post process destination view here
                // to make sure we write to the appropriate texture.
                view: post_process.destination,
                resolve_target: None,
                ops: Operations::default(),
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        // This is mostly just wgpu boilerplate for drawing a fullscreen triangle,
        // using the pipeline/bind_group created above
        render_pass.set_render_pipeline(pipeline);
        // By passing in the index of the post process settings on this view, we ensure
        // that in the event that multiple settings were sent to the GPU (as would be the
        // case with multiple cameras), we use the correct one.
        render_pass.set_bind_group(0, &bind_group, &[settings_index.index()]);
        render_pass.draw(0..3, 0..1);

        Ok(())
    }
}

// This contains global data used by the render pipeline. This will be created once on startup.
#[derive(Resource)]
struct PostProcessPipeline {
    layout: BindGroupLayout,
    sampler: Sampler,
    pipeline_id: CachedRenderPipelineId,
}

impl FromWorld for PostProcessPipeline {
    fn from_world(world: &mut World) -> Self {
        let render_device = world.resource::<RenderDevice>();

        // We need to define the bind group layout used for our pipeline
        let layout = render_device.create_bind_group_layout(
            "post_process_bind_group_layout",
            &BindGroupLayoutEntries::sequential(
                // The layout entries will only be visible in the fragment stage
                ShaderStages::FRAGMENT,
                (
                    // The screen texture
                    texture_2d(TextureSampleType::Float { filterable: true }),
                    // The sampler that will be used to sample the screen texture
                    sampler(SamplerBindingType::Filtering),
                    // The settings uniform that will control the effect
                    uniform_buffer::<PostProcessSettings>(true),
                ),
            ),
        );

        // We can create the sampler here since it won't change at runtime and doesn't depend on the view
        let sampler = render_device.create_sampler(&SamplerDescriptor::default());

        // Get the shader handle
        let shader = world.load_asset(SHADER_ASSET_PATH);

        let pipeline_id = world
            .resource_mut::<PipelineCache>()
            // This will add the pipeline to the cache and queue its creation
            .queue_render_pipeline(RenderPipelineDescriptor {
                label: Some("post_process_pipeline".into()),
                layout: vec![layout.clone()],
                // This will setup a fullscreen triangle for the vertex state
                vertex: fullscreen_shader_vertex_state(),
                fragment: Some(FragmentState {
                    shader,
                    shader_defs: vec![],
                    // Make sure this matches the entry point of your shader.
                    // It can be anything as long as it matches here and in the shader.
                    entry_point: "fragment".into(),
                    targets: vec![Some(ColorTargetState {
                        format: TextureFormat::bevy_default(),
                        blend: None,
                        write_mask: ColorWrites::ALL,
                    })],
                }),
                // All of the following properties are not important for this effect so just use the default values.
                // This struct doesn't have the Default trait implemented because not all fields can have a default value.
                primitive: PrimitiveState::default(),
                depth_stencil: None,
                multisample: MultisampleState::default(),
                push_constant_ranges: vec![],
                zero_initialize_workgroup_memory: false,
            });

        Self {
            layout,
            sampler,
            pipeline_id,
        }
    }
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
        PostProcessSettings {
            intensity: 0.002,
            ..default()
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
    spawn_outlined(
        &mut commands,
        step.clone(),
        grass_b.clone(),
        Transform::from_xyz(-2.5, 0.3, 1.0),
        outline_material.clone(),
        0.03,
        "TerraceLow",
    );

    // Mid terrace
    spawn_outlined(
        &mut commands,
        step.clone(),
        grass_a.clone(),
        Transform::from_xyz(1.5, 0.3, -0.5),
        outline_material.clone(),
        0.03,
        "TerraceMid",
    );

    // Tall terrace (stacked)
    let high = spawn_outlined(
        &mut commands,
        step.clone(),
        grass_b.clone(),
        Transform::from_xyz(5.0, 0.3, 3.5),
        outline_material.clone(),
        0.03,
        "TerraceHighBase",
    );
    // Cap (outlined)
    commands.entity(high).with_children(|c| {
        c.spawn((
            Mesh3d(slab.clone()),
            MeshMaterial3d(dirt.clone()),
            Transform::from_xyz(0.0, 0.45, 0.0),
            Name::new("TerraceHighCap"),
        ));
        c.spawn((
            Mesh3d(slab.clone()),
            MeshMaterial3d(outline_material.clone()),
            Transform::from_translation(Vec3::new(0.0, 0.45, 0.0))
                * Transform::from_scale(Vec3::splat(1.03)),
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
            &format!("Stone{}", i),
        );
    }

    // --- Emissive “crystal” on the mid terrace so bloom has a target
    spawn_outlined(
        &mut commands,
        sphere.clone(),
        crystal,
        Transform::from_xyz(1.5, 0.65, -0.5),
        outline_material.clone(),
        0.03,
        "Crystal",
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
        Name::new("Water"),
    ));
}

/// Helper: spawn a mesh with an outline child.
fn spawn_outlined(
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
fn update_outlines(
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

/// egui panel: tune DoF and Outline live.
fn dof_and_outline_panel(
    mut ctxs: EguiContexts,
    mut q_cam: Query<(&mut DepthOfField, &GlobalTransform), With<Camera3d>>,
    mut outline: ResMut<OutlineParams>,
    mut materials: ResMut<Assets<StandardMaterial>>,
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

    egui::Window::new("Depth of Field & Outline")
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

fn tweak_ca_with_keyboard(
    mut settings: Query<&mut PostProcessSettings>,
    kb: Res<ButtonInput<KeyCode>>,
) {
    for mut setting in &mut settings {
        // Quick live-tweaking with keys (optional)
        if kb.just_pressed(KeyCode::KeyV) {
            setting.intensity = (setting.intensity + 0.0005).min(1.0);
        }
        if kb.just_pressed(KeyCode::KeyB) {
            setting.intensity = (setting.intensity - 0.0005).max(0.0);
        }
    }
}
