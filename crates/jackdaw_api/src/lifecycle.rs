//! Entity-based lifecycle primitives for extensions.
//!
//! An extension is represented as an `Entity` with an [`Extension`] component.
//! Everything it registers — operators, BEI context entities, dock windows,
//! workspaces — is spawned as a child of that entity. Unloading is just
//! `world.entity_mut(ext).despawn()`, and Bevy cascades through the children.
//! A small set of observers in the `ExtensionLoaderPlugin` handles the cleanup
//! that can't be expressed purely as entity despawn (unregistering stored
//! `SystemId`s, removing from the dock `WindowRegistry`, etc.).

use std::collections::HashMap;
use std::sync::Arc;

use bevy::ecs::system::SystemId;
use bevy::prelude::*;

use crate::operator::OperatorResult;

/// Root component for an extension.
///
/// Despawning this entity tears down all of the extension's child entities:
/// operators, BEI context/action entities, registered windows/workspaces, and
/// observer entities. Non-ECS cleanup (unregistering `SystemId`s, removing
/// entries from `WindowRegistry`) is handled by observers reacting to the
/// child-entity despawns.
#[derive(Component, Debug)]
pub struct Extension {
    pub name: String,
}

/// An operator — child of an [`Extension`].
///
/// Holds the `SystemId`s that the dispatcher runs. An observer on
/// `On<Remove, OperatorEntity>` unregisters those systems when this entity
/// despawns, and keeps the [`OperatorIndex`] in sync.
#[derive(Component, Clone)]
pub struct OperatorEntity {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub execute: SystemId<(), OperatorResult>,
    pub invoke: SystemId<(), OperatorResult>,
    pub poll: Option<SystemId<(), bool>>,
}

/// Marks an entity as tracking a dock window registration.
///
/// Spawned as a child of the [`Extension`] entity when `register_window` is
/// called. An observer on `On<Remove, RegisteredWindow>` calls
/// `WindowRegistry::unregister(id)` so the window disappears from the
/// add-window popup when the extension unloads.
#[derive(Component, Clone, Debug)]
pub struct RegisteredWindow {
    pub id: String,
}

/// Marks an entity as tracking a workspace registration.
#[derive(Component, Clone, Debug)]
pub struct RegisteredWorkspace {
    pub id: String,
}

/// Marks an entity as tracking a panel-extension registration (a section
/// injected into an existing panel via `ExtensionContext::extend_window`).
#[derive(Component, Clone, Debug)]
pub struct RegisteredPanelExtension {
    pub panel_id: String,
    pub section_index: usize,
}

/// Reactive index from operator id → operator entity. Maintained by the
/// `index_operator_on_add` / `deindex_operator_on_remove` observers.
/// Lets the dispatcher resolve an id to a `SystemId` in O(1).
#[derive(Resource, Default)]
pub struct OperatorIndex {
    pub(crate) by_id: HashMap<&'static str, Entity>,
}

impl OperatorIndex {
    pub fn get(&self, id: &str) -> Option<Entity> {
        self.by_id.get(id).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&'static str, Entity)> + '_ {
        self.by_id.iter().map(|(k, v)| (*k, *v))
    }
}

/// Constructor function for an extension. Stored in [`ExtensionCatalog`].
pub type ExtensionCtor = Arc<dyn Fn() -> Box<dyn crate::JackdawExtension> + Send + Sync>;

/// Registry of all extensions compiled into this build of Jackdaw.
///
/// Populated once during startup by calling `ExtensionCatalog::register` for
/// each built-in extension. External extensions (if/when dylib loading lands)
/// would register themselves here too. Toggle UIs read the catalog to list
/// available extensions.
#[derive(Resource, Default)]
pub struct ExtensionCatalog {
    constructors: HashMap<String, ExtensionCtor>,
}

impl ExtensionCatalog {
    pub fn register<F>(&mut self, name: impl Into<String>, ctor: F)
    where
        F: Fn() -> Box<dyn crate::JackdawExtension> + Send + Sync + 'static,
    {
        self.constructors.insert(name.into(), Arc::new(ctor));
    }

    pub fn contains(&self, name: &str) -> bool {
        self.constructors.contains_key(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.constructors.keys().map(|s| s.as_str())
    }

    /// Construct a fresh instance of the named extension, if registered.
    pub fn construct(&self, name: &str) -> Option<Box<dyn crate::JackdawExtension>> {
        self.constructors.get(name).map(|f| f())
    }
}

/// Register an extension into the catalog and perform its one-time BEI
/// input-context registration.
///
/// Call this once per extension during app setup. Registering the constructor
/// lets the Plugins dialog list the extension; running
/// `register_input_contexts` ensures its BEI context types are known to the
/// framework. Enabling and disabling the extension later only re-runs
/// `register()`, never `register_input_contexts()` (BEI panics on duplicate
/// registrations).
pub fn register_extension<F>(app: &mut App, name: &str, ctor: F)
where
    F: Fn() -> Box<dyn crate::JackdawExtension> + Send + Sync + 'static,
{
    // Construct a throwaway instance just to register context types.
    let sample = ctor();
    sample.register_input_contexts(app);
    drop(sample);

    // Store the constructor in the catalog for runtime enable/disable.
    app.world_mut()
        .resource_mut::<ExtensionCatalog>()
        .register(name, ctor);
}

// ============================================================================
// Dispatch
// ============================================================================

use crate::operator::OperatorCommandBuffer;
use jackdaw_commands::{CommandGroup, CommandHistory};

/// Dispatch an operator by id. Used by the BEI `Fire<O>` observers spawned
/// in `ExtensionContext::register_operator`.
pub fn dispatch_operator_by_id(world: &mut World, id: &'static str, creates_history_entry: bool) {
    // Resolve operator via the reactive index.
    let Some(op_entity) = world.resource::<OperatorIndex>().by_id.get(id).copied() else {
        warn!("Tried to dispatch unknown operator: {}", id);
        return;
    };
    let Some(op) = world.get::<OperatorEntity>(op_entity).cloned() else {
        return;
    };

    // Poll (optional).
    if let Some(poll) = op.poll {
        if !world.run_system(poll).unwrap_or(false) {
            return;
        }
    }

    // Prep the command buffer, run the invoke system, drain the buffer.
    world
        .resource_mut::<OperatorCommandBuffer>()
        .prepare(creates_history_entry);

    let result = match world.run_system(op.invoke) {
        Ok(r) => r,
        Err(err) => {
            error!("Failed to run operator {}: {:?}", op.id, err);
            return;
        }
    };

    // First pass: treat `Running` like `Finished`. Modal support is future.
    let finished = matches!(result, OperatorResult::Finished | OperatorResult::Running);
    if !finished {
        // Cancelled — drop the recorded commands.
        world.resource_mut::<OperatorCommandBuffer>().take();
        return;
    }

    let (recorded, creates_history) = world.resource_mut::<OperatorCommandBuffer>().take();

    if creates_history && !recorded.is_empty() {
        let group = Box::new(CommandGroup {
            commands: recorded,
            label: op.label.to_string(),
        });
        world.resource_mut::<CommandHistory>().push_executed(group);
    }
}

// ============================================================================
// Loading / unloading / enable / disable
// ============================================================================

/// Unload an extension. Just despawns the root entity; the cascade + cleanup
/// observers take care of the rest.
pub fn unload_extension(world: &mut World, ext_entity: Entity) {
    let ext_name = world
        .get::<Extension>(ext_entity)
        .map(|e| e.name.clone())
        .unwrap_or_default();
    info!("Unloading extension: {}", ext_name);

    // Invoke the optional `unregister` hook before despawning.
    if let Some(stored) = world
        .entity_mut(ext_entity)
        .take::<crate::StoredExtension>()
    {
        stored.0.unregister(world, ext_entity);
    }
    if let Ok(ec) = world.get_entity_mut(ext_entity) {
        ec.despawn();
    }
}

/// Enable a named extension via the catalog. Returns the new extension
/// entity if the extension existed in the catalog and wasn't already loaded.
pub fn enable_extension(world: &mut World, name: &str) -> Option<Entity> {
    // Short-circuit if already loaded.
    {
        let mut query = world.query::<&Extension>();
        if query.iter(world).any(|e| e.name == name) {
            return None;
        }
    }

    let extension = world.resource::<ExtensionCatalog>().construct(name)?;

    Some(crate::load_static_extension(world, extension))
}

/// Disable a named extension. Finds the matching `Extension` entity and
/// despawns it.
pub fn disable_extension(world: &mut World, name: &str) -> bool {
    let mut query = world.query::<(Entity, &Extension)>();
    let Some(ext_entity) = query
        .iter(world)
        .find(|(_, e)| e.name == name)
        .map(|(e, _)| e)
    else {
        return false;
    };
    unload_extension(world, ext_entity);
    true
}

// ============================================================================
// Cleanup observers — added by ExtensionLoaderPlugin
// ============================================================================

/// Observer: keep `OperatorIndex` in sync on add.
pub fn index_operator_on_add(
    trigger: On<Add, OperatorEntity>,
    operators: Query<&OperatorEntity>,
    mut index: ResMut<OperatorIndex>,
) {
    if let Ok(op) = operators.get(trigger.event_target()) {
        index.by_id.insert(op.id, trigger.event_target());
    }
}

/// Observer: keep `OperatorIndex` in sync on remove. Also unregister the
/// operator's Bevy `SystemId`s so they don't leak across enable/disable
/// cycles.
pub fn deindex_and_cleanup_operator_on_remove(
    trigger: On<Remove, OperatorEntity>,
    operators: Query<&OperatorEntity>,
    mut index: ResMut<OperatorIndex>,
    mut commands: Commands,
) {
    let Ok(op) = operators.get(trigger.event_target()) else {
        return;
    };
    info!("Unregistering operator: {}", op.id);
    index.by_id.remove(op.id);
    let (exec, inv, poll) = (op.execute, op.invoke, op.poll);
    commands.queue(move |world: &mut World| {
        let _ = world.unregister_system(exec);
        if exec != inv {
            let _ = world.unregister_system(inv);
        }
        if let Some(p) = poll {
            let _ = world.unregister_system(p);
        }
    });
}

/// Observer: unregister a dock window from `WindowRegistry` when its
/// `RegisteredWindow` marker entity despawns. Also removes any docked
/// instances of the window from the live `DockTree` and every workspace's
/// stored tree so the UI actually reflects the disable.
pub fn cleanup_window_on_remove(
    trigger: On<Remove, RegisteredWindow>,
    windows: Query<&RegisteredWindow>,
    mut registry: ResMut<jackdaw_panels::WindowRegistry>,
    mut dock_tree: ResMut<jackdaw_panels::tree::DockTree>,
    mut workspaces: ResMut<jackdaw_panels::WorkspaceRegistry>,
) {
    let Ok(w) = windows.get(trigger.event_target()) else {
        return;
    };
    info!("Unregistering window: {}", w.id);
    registry.unregister(&w.id);
    // Remove from the live tree so any currently-docked instance vanishes.
    dock_tree.remove_window(&w.id);
    // And from each stored workspace tree so switching workspaces doesn't
    // resurrect it.
    for workspace in workspaces.workspaces.iter_mut() {
        workspace.tree.remove_window(&w.id);
    }
}

/// Observer: unregister a workspace when its `RegisteredWorkspace` marker
/// entity despawns.
pub fn cleanup_workspace_on_remove(
    trigger: On<Remove, RegisteredWorkspace>,
    workspaces: Query<&RegisteredWorkspace>,
    mut registry: ResMut<jackdaw_panels::WorkspaceRegistry>,
) {
    if let Ok(w) = workspaces.get(trigger.event_target()) {
        registry.unregister(&w.id);
    }
}

/// Observer: remove a panel extension section from the registry when its
/// marker entity despawns.
pub fn cleanup_panel_extension_on_remove(
    trigger: On<Remove, RegisteredPanelExtension>,
    registrations: Query<&RegisteredPanelExtension>,
    mut registry: ResMut<crate::PanelExtensionRegistry>,
) {
    if let Ok(r) = registrations.get(trigger.event_target()) {
        registry.remove(&r.panel_id, r.section_index);
    }
}
