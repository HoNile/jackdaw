//! Hammer-style physics placement tool.
//!
//! When `EditMode::Physics` is active, only selected entities simulate.
//! Non-selected `RigidBody` entities get `RigidBodyDisabled` so they act as
//! static collision geometry. The first drag on a selected entity unpauses
//! `Time<Physics>`. Space/Escape/switching tools commits transform changes
//! and exits.

use avian3d::prelude::*;
use bevy::{
    ecs::reflect::AppTypeRegistry,
    feathers::cursor::{EntityCursor, OverrideCursor},
    picking::mesh_picking::ray_cast::{MeshRayCast, MeshRayCastSettings, RayCastVisibility},
    prelude::*,
    window::SystemCursorIcon,
};

use crate::brush::EditMode;
use crate::commands::{CommandGroup, CommandHistory, EditorCommand, SetJsnField};
use crate::selection::Selection;
use jackdaw_avian_integration::simulation::{PhysicsDrag, PhysicsToolState};

pub struct PhysicsToolPlugin;

impl Plugin for PhysicsToolPlugin {
    fn build(&self, app: &mut App) {
        // Exclusive-world system runs separately (can't chain with normal systems
        // reliably in Bevy 0.18).
        app.add_systems(
            Update,
            on_edit_mode_transition.run_if(in_state(crate::AppState::Editor)),
        );
        app.add_systems(
            Update,
            (
                sync_selection_disable_state,
                physics_tool_drag,
                physics_tool_keys,
            )
                .chain()
                .after(on_edit_mode_transition)
                .run_if(in_state(crate::AppState::Editor)),
        );
    }
}

/// Track the previous EditMode to detect transitions into/out of Physics.
#[derive(Resource, Default)]
struct PreviousEditMode(EditMode);

/// Detect when EditMode changes to/from Physics and run entry/exit logic.
fn on_edit_mode_transition(world: &mut World) {
    let edit_mode = *world.resource::<EditMode>();
    let prev = world.get_resource_or_init::<PreviousEditMode>().0;

    if edit_mode == prev {
        return;
    }

    // Entering Physics mode
    if edit_mode == EditMode::Physics && prev != EditMode::Physics {
        enter_physics_tool(world);
    }

    // Exiting Physics mode
    if prev == EditMode::Physics && edit_mode != EditMode::Physics {
        exit_physics_tool(world);
    }

    world.resource_mut::<PreviousEditMode>().0 = edit_mode;
}

fn enter_physics_tool(world: &mut World) {
    let selection = world.resource::<Selection>();
    let selected: Vec<Entity> = selection.entities.clone();

    let mut state = world.resource_mut::<PhysicsToolState>();
    state.snapshots.clear();
    state.disabled_by_us.clear();
    state.sim_active = false;
    state.drag = None;

    // Log ColliderAabb values to diagnose stale-state fall-through issues
    {
        let mut aabb_query = world.query::<(Entity, &avian3d::prelude::ColliderAabb, &RigidBody)>();
        for (entity, aabb, rb) in aabb_query.iter(world) {
            info!(
                "Physics tool entry: entity {entity} ({rb:?}) — ColliderAabb min={:?} max={:?}",
                aabb.min, aabb.max
            );
        }
    }

    // Snapshot all RigidBody transforms; disable non-selected Dynamic/Kinematic
    // bodies. Static bodies are NEVER disabled — they need to remain as solid
    // collision surfaces for the simulated objects to land on.
    let mut bodies: Vec<(Entity, Transform, RigidBody, bool)> = Vec::new();
    let mut query = world.query_filtered::<(Entity, &Transform, &RigidBody), With<RigidBody>>();
    for (entity, tf, rb) in query.iter(world) {
        let is_selected = selected.contains(&entity);
        bodies.push((entity, *tf, *rb, is_selected));
    }

    let mut state = world.resource_mut::<PhysicsToolState>();
    for &(entity, tf, _, _) in &bodies {
        state.snapshots.insert(entity, tf);
    }

    for &(entity, _, rb, is_selected) in &bodies {
        if rb == RigidBody::Static {
            // Static bodies are always solid — never disable them
            continue;
        }
        if !is_selected {
            world
                .resource_mut::<PhysicsToolState>()
                .disabled_by_us
                .insert(entity);
            if let Ok(mut ec) = world.get_entity_mut(entity) {
                ec.insert(RigidBodyDisabled);
            }
        } else {
            // Ensure selected entities are NOT disabled
            if let Ok(mut ec) = world.get_entity_mut(entity) {
                ec.remove::<RigidBodyDisabled>();
            }
        }
    }
}

fn exit_physics_tool(world: &mut World) {
    // Pause physics
    world.resource_mut::<Time<Physics>>().pause();

    // Restore cursor
    let mut cursor = world.resource_mut::<OverrideCursor>();
    if cursor.0 == Some(EntityCursor::System(SystemCursorIcon::Grabbing)) {
        cursor.0 = None;
    }

    // Restore disabled state
    let disabled: Vec<Entity> = world
        .resource::<PhysicsToolState>()
        .disabled_by_us
        .iter()
        .copied()
        .collect();
    for entity in disabled {
        if let Ok(mut ec) = world.get_entity_mut(entity) {
            ec.remove::<RigidBodyDisabled>();
        }
    }

    // Commit transform diffs
    commit_physics_transforms(world);

    // Clear tool state
    let mut state = world.resource_mut::<PhysicsToolState>();
    state.snapshots.clear();
    state.disabled_by_us.clear();
    state.sim_active = false;
    state.drag = None;
}

/// Toggle `RigidBodyDisabled` when selection changes during Physics mode.
/// Static bodies are never disabled — they must remain as solid surfaces.
fn sync_selection_disable_state(
    edit_mode: Res<EditMode>,
    selection: Res<Selection>,
    mut commands: Commands,
    mut tool_state: ResMut<PhysicsToolState>,
    bodies: Query<(Entity, &RigidBody)>,
) {
    if *edit_mode != EditMode::Physics || !selection.is_changed() {
        return;
    }

    for (entity, rb) in bodies.iter() {
        if *rb == RigidBody::Static {
            continue;
        }
        let is_selected = selection.entities.contains(&entity);
        let was_disabled = tool_state.disabled_by_us.contains(&entity);

        if is_selected && was_disabled {
            // Newly selected → enable physics
            commands.entity(entity).remove::<RigidBodyDisabled>();
            commands.queue(WakeBody(entity));
            tool_state.disabled_by_us.remove(&entity);
        } else if !is_selected && !was_disabled {
            // Newly deselected → freeze
            commands.entity(entity).insert((
                RigidBodyDisabled,
                LinearVelocity(Vec3::ZERO),
                AngularVelocity(Vec3::ZERO),
            ));
            tool_state.disabled_by_us.insert(entity);
        }
    }
}

/// Raycast-based drag interaction: click and drag selected `RigidBody`
/// entities to move them on a camera-facing plane. First drag unpauses
/// `Time<Physics>`.
fn physics_tool_drag(
    edit_mode: Res<EditMode>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform), With<crate::viewport::MainViewportCamera>>,
    viewport_query: Query<
        (&ComputedNode, &UiGlobalTransform),
        With<crate::viewport::SceneViewport>,
    >,
    selection: Res<Selection>,
    parents: Query<&ChildOf>,
    mut tool_state: ResMut<PhysicsToolState>,
    mut ray_cast: MeshRayCast,
    mut physics_time: ResMut<Time<Physics>>,
    mut transforms: Query<&mut Transform>,
    mut velocities: Query<(&mut LinearVelocity, &mut AngularVelocity)>,
    rb_check: Query<(), With<RigidBody>>,
    mut override_cursor: ResMut<OverrideCursor>,
) {
    if *edit_mode != EditMode::Physics {
        return;
    }

    let Ok(window) = windows.single() else {
        warn!("Physics drag: no single window");
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    }; // silent — cursor off window
    let Ok((camera, cam_tf)) = camera_query.single() else {
        warn!("Physics drag: no single Camera3d");
        return;
    };

    let Some(viewport_cursor) =
        crate::viewport_util::window_to_viewport_cursor(cursor_pos, camera, &viewport_query)
    else {
        return; // cursor outside viewport — normal, don't log
    };

    let Ok(ray) = camera.viewport_to_world(cam_tf, viewport_cursor) else {
        warn!("Physics drag: viewport_to_world failed");
        return;
    };

    // --- Start drag ---
    if mouse.just_pressed(MouseButton::Left) && tool_state.drag.is_none() {
        let settings = MeshRayCastSettings::default().with_visibility(RayCastVisibility::Any);
        let hits = ray_cast.cast_ray(ray, &settings);

        info!(
            "Physics tool: click detected, {} raycast hits, selection has {} entities",
            hits.len(),
            selection.entities.len()
        );

        for (hit_entity, hit_data) in hits {
            // Walk up ChildOf to find the root entity
            let mut root = *hit_entity;
            loop {
                if rb_check.contains(root) && selection.entities.contains(&root) {
                    break;
                }
                if let Ok(child_of) = parents.get(root) {
                    root = child_of.0;
                } else {
                    break;
                }
            }

            let has_rb = rb_check.contains(root);
            let is_selected = selection.entities.contains(&root);
            info!(
                "Physics tool: hit entity {:?}, walked to root {:?}, has_rb={}, is_selected={}",
                hit_entity, root, has_rb, is_selected
            );

            if !has_rb || !is_selected {
                continue;
            }

            let hit_point = hit_data.point;
            let Ok(entity_tf) = transforms.get(root) else {
                continue;
            };

            let plane_normal = cam_tf.forward().as_vec3();
            let grab_offset = entity_tf.translation - hit_point;

            tool_state.drag = Some(PhysicsDrag {
                entity: root,
                plane_origin: hit_point,
                plane_normal,
                grab_offset,
            });

            // Activate simulation on first drag
            if !tool_state.sim_active {
                physics_time.unpause();
                tool_state.sim_active = true;
            }
            override_cursor.0 = Some(EntityCursor::System(SystemCursorIcon::Grabbing));
            break;
        }
    }

    // --- Update drag ---
    if let Some(drag) = tool_state.drag {
        if mouse.pressed(MouseButton::Left) {
            // Project cursor ray onto the drag plane
            let denom = ray.direction.dot(drag.plane_normal);
            if denom.abs() > 1e-6 {
                let t = (drag.plane_origin - ray.origin).dot(drag.plane_normal) / denom;
                if t > 0.0 {
                    let target = ray.origin + ray.direction * t + drag.grab_offset;
                    if let Ok(mut tf) = transforms.get_mut(drag.entity) {
                        tf.translation = target;
                    }
                    // Zero velocities so the entity doesn't fly off when released
                    if let Ok((mut lv, mut av)) = velocities.get_mut(drag.entity) {
                        lv.0 = Vec3::ZERO;
                        av.0 = Vec3::ZERO;
                    }
                }
            }
        } else {
            // Released
            tool_state.drag = None;
            if override_cursor.0 == Some(EntityCursor::System(SystemCursorIcon::Grabbing)) {
                override_cursor.0 = None;
            }
        }
    }
}

/// Space/Escape: commit & exit to Object mode.
fn physics_tool_keys(mut edit_mode: ResMut<EditMode>, keyboard: Res<ButtonInput<KeyCode>>) {
    if *edit_mode != EditMode::Physics {
        return;
    }
    if keyboard.just_pressed(KeyCode::Space) || keyboard.just_pressed(KeyCode::Escape) {
        *edit_mode = EditMode::Object;
    }
}

/// Diff snapshots vs current transforms, push one undoable `CommandGroup`
/// of `SetJsnField` commands.
fn commit_physics_transforms(world: &mut World) {
    let snapshots = world.resource::<PhysicsToolState>().snapshots.clone();

    if snapshots.is_empty() {
        return;
    }

    let registry_res = world.resource::<AppTypeRegistry>().clone();
    let processor = crate::scene_io::AstSerializerProcessor;
    let type_path = "bevy_transform::components::transform::Transform";

    let mut sub_commands: Vec<Box<dyn EditorCommand>> = Vec::new();

    for (entity, old_tf) in &snapshots {
        let Ok(entity_ref) = world.get_entity(*entity) else {
            continue;
        };
        let Some(new_tf) = entity_ref.get::<Transform>().copied() else {
            continue;
        };

        // Skip if unchanged
        let pos_diff = (old_tf.translation - new_tf.translation).length_squared();
        let rot_diff = old_tf.rotation.angle_between(new_tf.rotation);
        if pos_diff < 0.000001 && rot_diff < 0.001 {
            continue;
        }

        let registry = registry_res.read();
        let old_ser = bevy::reflect::serde::TypedReflectSerializer::with_processor(
            old_tf, &registry, &processor,
        );
        let Ok(old_json) = serde_json::to_value(&old_ser) else {
            continue;
        };
        let new_ser = bevy::reflect::serde::TypedReflectSerializer::with_processor(
            &new_tf, &registry, &processor,
        );
        let Ok(new_json) = serde_json::to_value(&new_ser) else {
            continue;
        };
        drop(registry);

        sub_commands.push(Box::new(SetJsnField {
            entity: *entity,
            type_path: type_path.to_string(),
            field_path: String::new(),
            old_value: old_json,
            new_value: new_json,
        }));
    }

    if sub_commands.is_empty() {
        return;
    }

    let mut cmd: Box<dyn EditorCommand> = if sub_commands.len() == 1 {
        sub_commands.pop().unwrap()
    } else {
        Box::new(CommandGroup {
            label: "Physics tool".to_string(),
            commands: sub_commands,
        })
    };
    cmd.execute(world);
    let mut history = world.resource_mut::<CommandHistory>();
    history.undo_stack.push(cmd);
    history.redo_stack.clear();
}
