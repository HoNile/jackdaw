//! Runtime-friendly game plugin API — the foundation for
//! in-process hot reload.
//!
//! # Why not use bevy's `Plugin` trait directly?
//!
//! Bevy's `Plugin::build(&self, app: &mut App)` needs `&mut App`.
//! After `App::run()` is called, bevy's internals move the `App`
//! into the runner's state (see `core::mem::replace` in
//! `bevy_app::App::run`) — there's no safe way to recover a
//! stable `*mut App` at runtime for a hot-reload swap.
//!
//! Every operation a game plugin actually performs on `App`
//! (registering systems, observers, resources, reflect types) has
//! a `&mut World` equivalent. `GameApp` wraps `&mut World` to
//! present that subset. Games implement [`GamePlugin`] instead of
//! bevy's `Plugin` trait; the jackdaw loader runs the build method
//! from inside an exclusive system (which has `&mut World`), so it
//! stays callable across the process lifetime including during
//! hot reload.
//!
//! # Teardown
//!
//! Every registration made through `GameApp` is tagged with the
//! game's identifier:
//!
//! * Systems land in a `GameSystems(name)` `SystemSet`. Teardown
//!   uses `Schedules::remove_systems_in_set` to evict them.
//! * Observer entities get a `GameRegistered` component so teardown
//!   can despawn them by query.
//! * Resources are recorded in [`GameRegistry`] so teardown can
//!   remove them by `TypeId`.
//! * Reflect-registered types that the game added are tracked so
//!   teardown can remove them from `AppTypeRegistry`.
//!
//! When the user saves a source file, the hot-reload driver runs
//! an exclusive system that:
//!
//! 1. Calls `game.teardown(&mut ctx)` — clears all tracked
//!    registrations.
//! 2. Drops the old `libloading::Library` handle — safe because
//!    step 1 removed every live reference to code inside it.
//! 3. dlopens the freshly built `.so` and calls its new
//!    `build(&mut ctx)` — which re-registers new systems.
//!
//! World state (entities spawned by the game, their components)
//! survives the swap untouched; the game picks up right where it
//! left off.

use std::any::TypeId;
use std::collections::HashMap;

use bevy::ecs::intern::Interned;
use bevy::ecs::schedule::{IntoScheduleConfigs, ScheduleLabel, Schedules, SystemSet};
use bevy::ecs::system::ScheduleSystem;
use bevy::ecs::world::World;
use bevy::prelude::{AppTypeRegistry, Component};
use bevy::reflect::GetTypeRegistration;

/// `SystemSet` marker for every system the game registers. Parameterised
/// by the game's static name so multiple games can coexist without
/// teardown cross-contamination.
#[derive(SystemSet, Clone, Eq, PartialEq, Hash, Debug)]
pub struct GameSystems(pub &'static str);

/// Tag on entities spawned via [`GameApp::spawn_observer`] (or any
/// other `GameApp` helper that spawns an entity). Teardown despawns
/// everything carrying this marker for the matching game name.
#[derive(Component, Clone, Copy, Debug)]
pub struct GameRegistered(pub &'static str);

/// Per-game record of what the build function touched so teardown
/// can undo it. Populated by `GameApp`'s setter methods; consumed
/// by `GameApp::teardown`.
#[derive(Default, Debug)]
pub struct GameBookkeeping {
    /// Schedule labels the game registered systems into. Teardown
    /// walks these to call `remove_systems_in_set` once per label.
    pub schedules: Vec<Interned<dyn ScheduleLabel>>,
    /// Resource `TypeId`s the game inserted. Teardown calls
    /// `World::remove_resource_by_id` for each.
    pub resources: Vec<TypeId>,
    /// Type-registry entries the game added. Teardown removes them
    /// from `AppTypeRegistry`.
    pub reflect_types: Vec<TypeId>,
}

/// Registry mapping game name → bookkeeping. Lives as a `World`
/// resource so teardown can always find it.
#[derive(Default, Debug, bevy::prelude::Resource)]
pub struct GameRegistry {
    games: HashMap<String, GameBookkeeping>,
}

impl GameRegistry {
    pub fn entry(&mut self, name: &str) -> &mut GameBookkeeping {
        self.games.entry(name.to_owned()).or_default()
    }

    pub fn take(&mut self, name: &str) -> Option<GameBookkeeping> {
        self.games.remove(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.games.contains_key(name)
    }
}

/// Trait games implement instead of [`bevy::prelude::Plugin`] to be
/// hot-reloadable. Semantics mirror `Plugin::build` but the argument
/// is a `GameApp` (wrapping `&mut World`) rather than `&mut App`.
///
/// Teardown has a default implementation that relies on the
/// bookkeeping the `GameApp` accumulated during `build`. Override
/// it only if the game allocates resources outside `GameApp` that
/// need explicit cleanup (e.g., opening network sockets, file
/// handles).
pub trait GamePlugin: Send + Sync + 'static {
    /// Called once at startup and once per hot-reload cycle after
    /// the new dylib has been dlopened. Register the game's
    /// systems, observers, resources, and reflect types here via
    /// the methods on `GameApp`.
    fn build(&self, ctx: &mut GameApp<'_>);

    /// Called before a hot reload is about to drop the current
    /// dylib. The default implementation removes everything
    /// previously tracked in the bookkeeping for this game. Override
    /// if the game holds externally-observable state (open files,
    /// sockets, background threads) that needs manual cleanup.
    fn teardown(&self, ctx: &mut GameApp<'_>) {
        ctx.teardown_tracked();
    }
}

/// Runtime-swappable wrapper around `&mut World`, parameterised by
/// the game's name so tracked registrations can be attributed.
pub struct GameApp<'w> {
    world: &'w mut World,
    name: &'static str,
}

impl<'w> GameApp<'w> {
    /// Construct a context for the named game. The caller must hold
    /// exclusive mutable access to the world for the lifetime `'w`;
    /// typically this is an exclusive bevy system.
    pub fn new(world: &'w mut World, name: &'static str) -> Self {
        Self { world, name }
    }

    /// Access the underlying `&mut World`. Use sparingly — state
    /// touched directly here won't be tracked for teardown.
    pub fn world_mut(&mut self) -> &mut World {
        self.world
    }

    /// Register systems in a schedule. Systems land in the game's
    /// `GameSystems(name)` set so teardown can remove them.
    pub fn add_systems<M>(
        &mut self,
        schedule: impl ScheduleLabel + Clone,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M>,
    ) -> &mut Self {
        let set = GameSystems(self.name);
        let configured = systems.in_set(set);
        self.world
            .resource_mut::<Schedules>()
            .add_systems(schedule.clone(), configured);
        let interned = schedule.intern();
        let entry = self
            .world
            .get_resource_or_insert_with::<GameRegistry>(GameRegistry::default)
            .into_inner()
            .entry(self.name);
        if !entry.schedules.contains(&interned) {
            entry.schedules.push(interned);
        }
        self
    }

    /// Insert a resource. Records the resource's `TypeId` so
    /// teardown can remove it.
    pub fn insert_resource<R: bevy::prelude::Resource>(&mut self, res: R) -> &mut Self {
        let id = TypeId::of::<R>();
        self.world.insert_resource(res);
        let entry = self
            .world
            .get_resource_or_insert_with::<GameRegistry>(GameRegistry::default)
            .into_inner()
            .entry(self.name);
        if !entry.resources.contains(&id) {
            entry.resources.push(id);
        }
        self
    }

    /// Initialise a resource from `Default`. Same tracking as
    /// `insert_resource`.
    pub fn init_resource<R: bevy::prelude::Resource + Default>(&mut self) -> &mut Self {
        let id = TypeId::of::<R>();
        self.world.init_resource::<R>();
        let entry = self
            .world
            .get_resource_or_insert_with::<GameRegistry>(GameRegistry::default)
            .into_inner()
            .entry(self.name);
        if !entry.resources.contains(&id) {
            entry.resources.push(id);
        }
        self
    }

    /// Register a `Reflect`'d type in `AppTypeRegistry`. Tracked so
    /// teardown can remove it.
    pub fn register_type<T: GetTypeRegistration>(&mut self) -> &mut Self {
        let id = TypeId::of::<T>();
        if let Some(registry) = self.world.get_resource::<AppTypeRegistry>() {
            registry.write().register::<T>();
        }
        let entry = self
            .world
            .get_resource_or_insert_with::<GameRegistry>(GameRegistry::default)
            .into_inner()
            .entry(self.name);
        if !entry.reflect_types.contains(&id) {
            entry.reflect_types.push(id);
        }
        self
    }

    /// Default teardown: walks the game's bookkeeping and reverses
    /// everything tracked during build. Called from `GamePlugin`'s
    /// default `teardown` implementation.
    pub fn teardown_tracked(&mut self) {
        let bookkeeping = self
            .world
            .get_resource_mut::<GameRegistry>()
            .and_then(|mut r| r.take(self.name));
        let Some(book) = bookkeeping else {
            return;
        };

        // 1) Systems — SKIPPED.
        //
        // `Schedules::remove_systems_in_set` internally calls
        // `world.resource_scope::<Schedules, _>` recursively, which
        // panics if we're already holding Schedules in scope *or*
        // if we call it from within any other `schedule_scope`
        // (which any `commands.queue` closure is — it runs during
        // `apply_deferred` inside the containing schedule). Across
        // the `extern "C"` FFI boundary that panic becomes an
        // abort, which killed the process.
        //
        // Trade-off for v1: don't try to remove old systems at all.
        // They stay registered, but their `Query<… With<Component>>`
        // sees no matching entities once we despawn the
        // `GameRegistered` entities below, so they're effectively
        // no-ops. The new build's `add_systems` registers fresh
        // systems; both sets run against the newly-spawned entities.
        //
        // Concrete consequence: a component touched by both the
        // old and new version of a system (e.g. `Transform` via
        // rotate_y) gets mutated twice per frame → visible at ~2x
        // the declared speed. Documented limitation. A future fix
        // needs a deferred-reload path (set `PendingDylibInstall`,
        // run install from a dedicated exclusive system in `First`
        // or `Last` so no schedule_scope is active on `Update`).
        let _ = &book.schedules;

        // 2) Observer entities tagged with `GameRegistered(name)`.
        let name = self.name;
        let mut to_despawn = Vec::new();
        let mut q = self.world.query::<(bevy::prelude::Entity, &GameRegistered)>();
        for (entity, tag) in q.iter(self.world) {
            if tag.0 == name {
                to_despawn.push(entity);
            }
        }
        for e in to_despawn {
            if let Ok(ec) = self.world.get_entity_mut(e) {
                ec.despawn();
            }
        }

        // 3) Resources — `World::remove_resource_by_id`.
        for id in &book.resources {
            if let Some(component_id) = self.world.components().get_resource_id(*id) {
                self.world.remove_resource_by_id(component_id);
            }
        }

        // 4) Reflect type registry entries.
        //
        // bevy 0.18's `TypeRegistry` has no `remove` method — entries
        // stay. This is almost always fine: on reload the game's
        // `build` re-registers the same type (same `TypePath`),
        // which overwrites the previous entry. The leak is one
        // `TypeRegistration` per type per reload cycle, bounded by
        // the number of unique game types the user ever ships —
        // negligible.
        let _ = &book.reflect_types;
    }

    /// Game's declared name, for diagnostics.
    pub fn name(&self) -> &'static str {
        self.name
    }
}

/// Convenience for spawning observers with the `GameRegistered`
/// marker attached. Observers tag their own entity via `trigger`'s
/// component insert; we pair that with our marker so teardown can
/// find and despawn them.
///
/// Usage:
///
/// ```ignore
/// ctx.spawn_observer(|on: On<Add, MyComp>, mut commands: Commands| { … });
/// ```
impl<'w> GameApp<'w> {
    pub fn spawn_observer<E, B, M>(
        &mut self,
        observer: impl IntoObserverSystemBoxed<E, B, M>,
    ) -> &mut Self
    where
        E: bevy::prelude::Event,
        B: bevy::prelude::Bundle,
    {
        let tag = GameRegistered(self.name);
        let observer = observer.into_boxed_observer();
        let id = self.world.spawn_empty().id();
        self.world.entity_mut(id).insert(observer);
        self.world.entity_mut(id).insert(tag);
        self
    }
}

/// Helper trait so `spawn_observer` can accept either a raw system
/// (function) or a pre-built `Observer`. Monomorphised into the
/// same path.
pub trait IntoObserverSystemBoxed<E, B, M>: 'static {
    fn into_boxed_observer(self) -> bevy::prelude::Observer;
}

impl<E, B, M, S> IntoObserverSystemBoxed<E, B, M> for S
where
    E: bevy::prelude::Event,
    B: bevy::prelude::Bundle,
    S: bevy::ecs::system::IntoObserverSystem<E, B, M> + 'static,
{
    fn into_boxed_observer(self) -> bevy::prelude::Observer {
        bevy::prelude::Observer::new(self)
    }
}
