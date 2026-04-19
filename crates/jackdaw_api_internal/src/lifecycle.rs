//! Entity-based lifecycle primitives for extensions.
//!
//! An extension is represented as an [`Entity`] carrying an [`Extension`]
//! component. Everything it registers (operators, BEI context entities,
//! dock windows, workspaces) is spawned as a child of that entity.
//! Unloading is `world.entity_mut(ext).despawn()`; Bevy cascades through
//! the children. A small set of observers in `ExtensionLoaderPlugin`
//! handles cleanup that can't be expressed purely as entity despawn:
//! unregistering stored `SystemId`s, removing entries from the dock
//! `WindowRegistry`, and so on.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use bevy::ecs::system::SystemId;
use bevy::prelude::*;
use jackdaw_commands::{CommandHistory, EditorCommand};

use crate::operator::OperatorResult;
use crate::snapshot::{ActiveSnapshotter, SceneSnapshot};

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

/// Child of an [`Extension`]; represents a single operator.
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
    /// Optional system that returns whether the operator can run in
    /// the current editor state. Equivalent to Blender's `poll`.
    pub availability_check: Option<SystemId<(), bool>>,
    /// Mirrors [`crate::Operator::MODAL`]. Set at registration so the
    /// dispatcher can enter modal mode without re-resolving the generic
    /// operator type.
    pub modal: bool,
}

/// Tracks the currently-active modal operator. Exactly zero or one is
/// active at any time; starting a second modal while one is running is
/// refused.
///
/// The `before_snapshot` is captured when the modal begins; on commit
/// the dispatcher diffs it against a fresh snapshot and pushes a single
/// undo entry, so the entire modal session rolls up into one Ctrl+Z.
#[derive(Resource, Default)]
pub struct ActiveModalOperator {
    pub(crate) id: Option<&'static str>,
    pub(crate) operator_entity: Option<Entity>,
    pub(crate) invoke_system: Option<SystemId<(), OperatorResult>>,
    pub(crate) label: Option<String>,
    pub(crate) before_snapshot: Option<Box<dyn SceneSnapshot>>,
}

impl ActiveModalOperator {
    pub fn is_active(&self) -> bool {
        self.id.is_some()
    }

    pub fn id(&self) -> Option<&'static str> {
        self.id
    }
}

/// Counts how deeply operators are nested. The outermost operator in
/// a call chain takes the snapshot; inner operators' mutations roll
/// into that outer diff.
#[derive(Resource, Default)]
pub struct OperatorSession {
    depth: u32,
}

impl OperatorSession {
    pub fn is_outermost(&self) -> bool {
        self.depth == 0
    }
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

/// An extension-contributed entry in the editor menu bar.
///
/// Spawned as a child of the [`Extension`] entity via
/// [`crate::ExtensionContext::register_menu_entry`]. The editor's
/// `populate_menu` system queries these and inserts them into the right
/// menu. Clicking one dispatches the referenced operator.
///
/// `menu` is the top-level menu name (`"Add"`, `"Tools"`, etc.). The
/// menu system is flat today; using a path-like string here leaves room
/// for nested menus later without breaking callers.
#[derive(Component, Clone, Debug)]
pub struct RegisteredMenuEntry {
    pub menu: String,
    pub label: String,
    pub operator_id: &'static str,
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
    entries: HashMap<String, CatalogEntry>,
}

struct CatalogEntry {
    ctor: ExtensionCtor,
    kind: ExtensionKind,
}

/// Classifies an entry in the catalog. Surfaced in toggle UIs so
/// Jackdaw-shipped feature areas and third-party extensions can be
/// presented separately. Extensions declare their own kind via
/// [`crate::JackdawExtension::kind`]; registration captures it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtensionKind {
    /// Ships with Jackdaw as a core feature area (scene tree, inspector,
    /// asset browser, etc.). Present in every build.
    Builtin,
    /// Everything else: example extensions bundled for demonstration,
    /// third-party extensions loaded from disk, user-authored addons.
    Custom,
}

impl ExtensionCatalog {
    /// Register a constructor with its declared kind. Most callers
    /// should use [`register_extension`] instead, which handles BEI
    /// context registration.
    pub fn register<F>(&mut self, name: impl Into<String>, kind: ExtensionKind, ctor: F)
    where
        F: Fn() -> Box<dyn crate::JackdawExtension> + Send + Sync + 'static,
    {
        self.entries.insert(
            name.into(),
            CatalogEntry {
                ctor: Arc::new(ctor),
                kind,
            },
        );
    }

    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(|s| s.as_str())
    }

    /// Iterate names with their declared [`ExtensionKind`]. Useful for
    /// grouping the Extensions dialog into Built-in and Custom sections.
    pub fn iter_with_kind(&self) -> impl Iterator<Item = (&str, ExtensionKind)> {
        self.entries
            .iter()
            .map(|(name, entry)| (name.as_str(), entry.kind))
    }

    /// Look up the declared [`ExtensionKind`] for a registered name.
    pub fn kind(&self, name: &str) -> Option<ExtensionKind> {
        self.entries.get(name).map(|e| e.kind)
    }

    /// Whether the named extension is a Jackdaw-shipped built-in.
    /// Returns `false` for unknown names.
    pub fn is_builtin(&self, name: &str) -> bool {
        self.kind(name) == Some(ExtensionKind::Builtin)
    }

    /// Construct a fresh instance of the named extension, if registered.
    pub fn construct(&self, name: &str) -> Option<Box<dyn crate::JackdawExtension>> {
        self.entries.get(name).map(|e| (e.ctor)())
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
    // Construct a throwaway instance to (a) register context types and
    // (b) read the extension's declared `kind`. Doing both against the
    // same instance avoids a second construction just to classify.
    let sample = ctor();
    sample.register_input_contexts(app);
    let kind = sample.kind();
    drop(sample);

    app.world_mut()
        .resource_mut::<ExtensionCatalog>()
        .register(name, kind, ctor);
}

/// Extension trait on [`World`] for calling operators by id.
///
/// Usage:
///
/// ```ignore
/// use jackdaw_api::prelude::*;
///
/// fn my_button(mut commands: Commands) {
///     commands.queue(|world: &mut World| {
///         let _ = world.call_operator("avian.add_rigid_body");
///     });
/// }
/// ```
pub trait OperatorWorldExt {
    /// Call an operator by id. The availability check runs before the
    /// invoke system, so validation logic lives only on the operator
    /// itself. Equivalent to
    /// `call_operator_with(id, &CallOperatorSettings::default())`.
    fn call_operator(
        &mut self,
        id: impl Into<Cow<'static, str>>,
    ) -> Result<OperatorResult, CallOperatorError>;

    /// Call an operator with explicit settings.
    fn call_operator_with(
        &mut self,
        id: impl Into<Cow<'static, str>>,
        settings: &CallOperatorSettings,
    ) -> Result<OperatorResult, CallOperatorError>;

    /// Whether the operator would run in the current editor state.
    /// `Ok(true)` if it's ready, `Ok(false)` if not, `Err` for unknown
    /// ids.
    fn is_operator_available(
        &mut self,
        id: impl Into<Cow<'static, str>>,
    ) -> Result<bool, CallOperatorError>;
}

/// Knobs passed to [`OperatorWorldExt::call_operator_with`].
#[derive(Clone, Debug)]
pub struct CallOperatorSettings {
    /// Whether a successful call should push an undo entry. Default
    /// `true`. Set `false` for view-local effects (camera moves,
    /// preview toggles) that should not be undoable.
    pub creates_history_entry: bool,
}

impl Default for CallOperatorSettings {
    fn default() -> Self {
        Self {
            creates_history_entry: true,
        }
    }
}

#[derive(Clone, Debug)]
pub enum CallOperatorError {
    UnknownId(Cow<'static, str>),
    ModalAlreadyActive(&'static str),
    AvailabilityCheckFailed,
    ExecuteFailed,
}

impl std::fmt::Display for CallOperatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownId(id) => write!(f, "unknown operator: {id}"),
            Self::ModalAlreadyActive(id) => {
                write!(f, "modal operator '{id}' is currently active")
            }
            Self::AvailabilityCheckFailed => f.write_str("operator's availability check failed"),
            Self::ExecuteFailed => f.write_str("operator's execute system failed"),
        }
    }
}

impl std::error::Error for CallOperatorError {}

impl OperatorWorldExt for World {
    fn call_operator(
        &mut self,
        id: impl Into<Cow<'static, str>>,
    ) -> Result<OperatorResult, CallOperatorError> {
        self.call_operator_with(id, &CallOperatorSettings::default())
    }

    fn call_operator_with(
        &mut self,
        id: impl Into<Cow<'static, str>>,
        settings: &CallOperatorSettings,
    ) -> Result<OperatorResult, CallOperatorError> {
        let id = id.into();
        dispatch_operator(self, id, settings.creates_history_entry)
    }

    fn is_operator_available(
        &mut self,
        id: impl Into<Cow<'static, str>>,
    ) -> Result<bool, CallOperatorError> {
        let id = id.into();
        let Some(op_entity) = self
            .resource::<OperatorIndex>()
            .by_id
            .get(id.as_ref())
            .copied()
        else {
            return Err(CallOperatorError::UnknownId(id));
        };
        let Some(op) = self.get::<OperatorEntity>(op_entity).cloned() else {
            return Err(CallOperatorError::UnknownId(id));
        };
        let Some(check) = op.availability_check else {
            return Ok(true);
        };
        self.run_system(check)
            .map_err(|_| CallOperatorError::AvailabilityCheckFailed)
    }
}

fn dispatch_operator(
    world: &mut World,
    id: Cow<'static, str>,
    creates_history_entry: bool,
) -> Result<OperatorResult, CallOperatorError> {
    if let Some(active_id) = world.resource::<ActiveModalOperator>().id {
        return Err(CallOperatorError::ModalAlreadyActive(active_id));
    }

    let Some(op_entity) = world
        .resource::<OperatorIndex>()
        .by_id
        .get(id.as_ref())
        .copied()
    else {
        return Err(CallOperatorError::UnknownId(id));
    };
    let Some(op) = world.get::<OperatorEntity>(op_entity).cloned() else {
        return Err(CallOperatorError::UnknownId(id));
    };

    if let Some(check) = op.availability_check {
        let available = world
            .run_system(check)
            .map_err(|_| CallOperatorError::AvailabilityCheckFailed)?;
        if !available {
            return Ok(OperatorResult::Cancelled);
        }
    }

    // Only the outermost operator in a nesting chain captures the
    // snapshot. Inner `call_operator` calls mutate inside the outer's
    // span and their changes roll into the outer's diff.
    let is_outermost = world.resource::<OperatorSession>().depth == 0;
    let before = (is_outermost && creates_history_entry)
        .then(|| world.resource::<ActiveSnapshotter>().0.capture(world));

    world.resource_mut::<OperatorSession>().depth += 1;
    let result = world.run_system(op.invoke);
    world.resource_mut::<OperatorSession>().depth -= 1;

    let result = result.map_err(|_| CallOperatorError::ExecuteFailed)?;

    match result {
        OperatorResult::Running if op.modal => {
            let mut active = world.resource_mut::<ActiveModalOperator>();
            active.id = Some(op.id);
            active.operator_entity = Some(op_entity);
            active.invoke_system = Some(op.invoke);
            active.label = Some(op.label.to_string());
            active.before_snapshot = before;
        }
        OperatorResult::Running | OperatorResult::Finished => {
            finalize(world, op.label, before);
        }
        OperatorResult::Cancelled => {
            // Drop the snapshot without pushing history.
            drop(before);
        }
    }

    Ok(result)
}

/// Capture the current state, diff against `before`, and push a
/// `SnapshotDiff` onto [`CommandHistory`] if the scene changed.
fn finalize(world: &mut World, label: &str, before: Option<Box<dyn SceneSnapshot>>) {
    let Some(before) = before else { return };
    let after = world.resource::<ActiveSnapshotter>().0.capture(world);
    if before.equals(&*after) {
        return;
    }
    world
        .resource_mut::<CommandHistory>()
        .push_executed(Box::new(SnapshotDiff {
            before,
            after,
            label: label.to_string(),
        }));
}

/// One undo entry. Swaps the active scene snapshot on execute / undo.
struct SnapshotDiff {
    before: Box<dyn SceneSnapshot>,
    after: Box<dyn SceneSnapshot>,
    label: String,
}

impl EditorCommand for SnapshotDiff {
    fn execute(&mut self, world: &mut World) {
        self.after.apply(world);
    }
    fn undo(&mut self, world: &mut World) {
        self.before.apply(world);
    }
    fn description(&self) -> &str {
        &self.label
    }
}

/// Tick system added to Update by `ExtensionLoaderPlugin`. Re-runs the
/// active modal operator's invoke system each frame; exits modal on
/// `Finished` (committing) or `Cancelled` (discarding).
pub fn tick_modal_operator(world: &mut World) {
    let Some(invoke) = world.resource::<ActiveModalOperator>().invoke_system else {
        return;
    };
    let result = match world.run_system(invoke) {
        Ok(r) => r,
        Err(err) => {
            error!("Modal operator's invoke system failed: {err:?}; cancelling");
            finalize_modal(world, false);
            return;
        }
    };
    match result {
        OperatorResult::Running => {}
        OperatorResult::Finished => finalize_modal(world, true),
        OperatorResult::Cancelled => finalize_modal(world, false),
    }
}

/// Exit modal mode. Commits the before-snapshot diff as a history entry
/// if `commit`, otherwise discards it.
fn finalize_modal(world: &mut World, commit: bool) {
    let (label, before) = {
        let mut active = world.resource_mut::<ActiveModalOperator>();
        let label = active.label.take().unwrap_or_default();
        let before = active.before_snapshot.take();
        active.id = None;
        active.operator_entity = None;
        active.invoke_system = None;
        (label, before)
    };
    if commit {
        finalize(world, &label, before);
    }
}

/// Unload an extension. Despawns the root entity; the cascade and
/// cleanup observers handle the rest.
pub fn unload_extension(world: &mut World, ext_entity: Entity) {
    let ext_name = world
        .get::<Extension>(ext_entity)
        .map(|e| e.name.clone())
        .unwrap_or_default();
    info!("Unloading extension: {}", ext_name);

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
/// entity, or `None` if the name is unknown or already loaded.
pub fn enable_extension(world: &mut World, name: &str) -> Option<Entity> {
    {
        let mut query = world.query::<&Extension>();
        if query.iter(world).any(|e| e.name == name) {
            return None;
        }
    }

    let extension = world.resource::<ExtensionCatalog>().construct(name)?;
    Some(crate::load_static_extension(world, extension))
}

/// Disable a named extension by despawning its root entity.
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

/// Keep [`OperatorIndex`] in sync when an operator entity is spawned.
pub fn index_operator_on_add(
    trigger: On<Add, OperatorEntity>,
    operators: Query<&OperatorEntity>,
    mut index: ResMut<OperatorIndex>,
) {
    if let Ok(op) = operators.get(trigger.event_target()) {
        index.by_id.insert(op.id, trigger.event_target());
    }
}

/// Keep [`OperatorIndex`] in sync and free the operator's `SystemId`s
/// when its entity is removed, so they don't leak across enable /
/// disable cycles.
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
    let (exec, inv, check) = (op.execute, op.invoke, op.availability_check);
    commands.queue(move |world: &mut World| {
        let _ = world.unregister_system(exec);
        if exec != inv {
            let _ = world.unregister_system(inv);
        }
        if let Some(c) = check {
            let _ = world.unregister_system(c);
        }
    });
}

/// Unregister a dock window from [`jackdaw_panels::WindowRegistry`] and
/// purge it from the live dock tree and every stored workspace tree when
/// its marker entity despawns, so disabling an extension visibly removes
/// its windows.
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
    dock_tree.remove_window(&w.id);
    for workspace in workspaces.workspaces.iter_mut() {
        workspace.tree.remove_window(&w.id);
    }
}

/// Unregister a workspace when its marker entity despawns.
pub fn cleanup_workspace_on_remove(
    trigger: On<Remove, RegisteredWorkspace>,
    workspaces: Query<&RegisteredWorkspace>,
    mut registry: ResMut<jackdaw_panels::WorkspaceRegistry>,
) {
    if let Ok(w) = workspaces.get(trigger.event_target()) {
        registry.unregister(&w.id);
    }
}

/// Remove a panel extension section from the registry when its marker
/// entity despawns.
pub fn cleanup_panel_extension_on_remove(
    trigger: On<Remove, RegisteredPanelExtension>,
    registrations: Query<&RegisteredPanelExtension>,
    mut registry: ResMut<crate::PanelExtensionRegistry>,
) {
    if let Ok(r) = registrations.get(trigger.event_target()) {
        registry.remove(&r.panel_id, r.section_index);
    }
}

/// Log menu-entry registrations. Actual menu rebuilds are driven by
/// the main crate's `MenuBarDirty` flag because this crate doesn't know
/// the menu-bar implementation.
pub fn log_menu_entry_on_add(
    trigger: On<Add, RegisteredMenuEntry>,
    entries: Query<&RegisteredMenuEntry>,
) {
    if let Ok(entry) = entries.get(trigger.event_target()) {
        info!(
            "Registered menu entry: {} > {} -> {}",
            entry.menu, entry.label, entry.operator_id
        );
    }
}
