//! Source-change → rebuild → install → restart loop.
//!
//! Watches `<ProjectRoot>/src/**/*.rs` and `Cargo.toml` inside the
//! active editor project. On save (800 ms debounce), runs
//! [`ext_build::build_extension_project_with_progress`] in an
//! [`AsyncComputeTaskPool`] task, installs the produced `.so` via
//! the atomic-rename install path, and triggers
//! [`restart::restart_jackdaw`].
//!
//! # What this is (and isn't)
//!
//! This is **fast rebuild + exec-based respawn**, not true
//! in-process hot reload. The respawn uses `exec` on Unix so the
//! PID and terminal attachment survive; cargo-run's wait loop stays
//! happy and the child inherits the parent's env. But bevy tears
//! down winit and starts over, so there's a brief window flicker.
//!
//! True in-process reload (zero window flicker, preserved
//! `PlayState::Playing`, preserved game-spawned entities across
//! edits) requires either:
//!
//! 1. Exposing `&mut App` at runtime — bevy's `App::run` consumes
//!    the App so we don't have access after startup.
//! 2. Restructuring PIE to run the game inside a `SubApp` so we
//!    can tear down and rebuild the SubApp in place without
//!    touching the main editor App.
//!
//! Both are larger architectural changes tracked separately. The
//! exec-respawn flow here closes 90% of the iteration gap today.
//!
//! # Toggle
//!
//! `HotReloadEnabled` resource is flipped via the File menu. When
//! off, source changes don't trigger anything.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, futures_lite::future};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::ext_build::{BuildError, BuildProgress, build_extension_project_with_progress};
use crate::project::ProjectRoot;

/// Installs the hot-reload watcher. Active only in `Editor` state
/// where `ProjectRoot` is present.
pub struct HotReloadPlugin;

impl Plugin for HotReloadPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HotReloadEnabled>()
            .init_resource::<HotReloadState>()
            .add_systems(OnEnter(crate::AppState::Editor), start_watcher)
            .add_systems(OnExit(crate::AppState::Editor), stop_watcher)
            .add_systems(
                Update,
                (drain_source_changes, poll_reload_build)
                    .run_if(in_state(crate::AppState::Editor)),
            );
    }
}

/// User-facing toggle. Default `true` — auto-rebuild on save feels
/// natural for anyone who just scaffolded a game, and a stray
/// editor change never hurts (the debounce and in-flight guard
/// prevent runaway builds). File-menu entry flips this.
#[derive(Resource)]
pub struct HotReloadEnabled(pub bool);

impl Default for HotReloadEnabled {
    fn default() -> Self {
        Self(true)
    }
}

/// Debounce + in-flight tracking for the watcher.
#[derive(Resource, Default)]
struct HotReloadState {
    /// The live watcher handle. Dropping it stops the notify
    /// thread. Kept as `Option` so we can start/stop on
    /// `AppState::Editor` enter/exit.
    watcher: Option<RecommendedWatcher>,
    /// Latest relevant-event timestamp. `drain_source_changes`
    /// reads this and, after the debounce window, kicks off a
    /// build.
    pending: Arc<Mutex<Option<Instant>>>,
    /// In-flight build task. While `Some`, further source events
    /// update `pending` but don't spawn new builds.
    build_task: Option<Task<Result<PathBuf, BuildError>>>,
    /// Shared progress sink the task writes into. Future work:
    /// render this in the status bar while a hot rebuild runs.
    #[allow(dead_code)]
    build_progress: Option<Arc<Mutex<BuildProgress>>>,
    /// Project being rebuilt. Held so the finish handler can pass
    /// the right path into the install step.
    project_dir: Option<PathBuf>,
}

const DEBOUNCE_WINDOW: Duration = Duration::from_millis(800);

fn start_watcher(
    mut state: ResMut<HotReloadState>,
    project: Option<Res<ProjectRoot>>,
    enabled: Res<HotReloadEnabled>,
) {
    if !enabled.0 {
        return;
    }
    let Some(project) = project else {
        return;
    };
    let project_root = project.root.clone();

    let pending = Arc::clone(&state.pending);
    let pending_for_cb = Arc::clone(&pending);
    let root_for_filter = project_root.clone();

    let watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        let Ok(event) = res else { return };
        if !is_relevant_event(&event, &root_for_filter) {
            return;
        }
        if let Ok(mut slot) = pending_for_cb.lock() {
            *slot = Some(Instant::now());
        }
    });

    let Ok(mut watcher) = watcher else {
        warn!("HotReload: failed to create notify watcher");
        return;
    };

    let src_dir = project_root.join("src");
    if src_dir.is_dir() {
        if let Err(e) = watcher.watch(&src_dir, RecursiveMode::Recursive) {
            warn!("HotReload: failed to watch {}: {e}", src_dir.display());
        } else {
            info!("HotReload: watching {}", src_dir.display());
        }
    }
    let cargo_toml = project_root.join("Cargo.toml");
    if cargo_toml.is_file() {
        if let Err(e) = watcher.watch(&cargo_toml, RecursiveMode::NonRecursive) {
            warn!("HotReload: failed to watch {}: {e}", cargo_toml.display());
        }
    }

    state.watcher = Some(watcher);
    state.project_dir = Some(project_root);
}

fn stop_watcher(mut state: ResMut<HotReloadState>) {
    // Dropping the watcher stops the notify thread.
    state.watcher = None;
    state.project_dir = None;
    if let Ok(mut slot) = state.pending.lock() {
        *slot = None;
    }
}

/// Decide whether a filesystem event from the watcher should
/// trigger a rebuild. Accepts Modify(Data) and Create on `.rs`
/// files under `src/` or on `Cargo.toml`. Rejects everything inside
/// `target/` (notify's recursive mode would otherwise re-fire on
/// every cargo write and loop us).
fn is_relevant_event(event: &Event, project_root: &Path) -> bool {
    let relevant_kind = matches!(
        event.kind,
        EventKind::Create(_)
            | EventKind::Modify(notify::event::ModifyKind::Data(_))
            | EventKind::Modify(notify::event::ModifyKind::Any)
    );
    if !relevant_kind {
        return false;
    }
    for path in &event.paths {
        if path_is_relevant(path, project_root) {
            return true;
        }
    }
    false
}

fn path_is_relevant(path: &Path, project_root: &Path) -> bool {
    // Anything inside target/ is cargo's own output — ignore.
    if path.starts_with(project_root.join("target")) {
        return false;
    }
    // Anything inside .git or other dotdirs — ignore.
    if path
        .components()
        .any(|c| c.as_os_str().to_str().is_some_and(|s| s.starts_with(".git")))
    {
        return false;
    }
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    // Editor swap/temp files.
    if name.ends_with('~') || name.starts_with("#") || name.starts_with(".#") {
        return false;
    }
    if name == "Cargo.toml" {
        return true;
    }
    path.extension().and_then(|e| e.to_str()) == Some("rs")
}

/// Polled each frame in `Update`. If the debounce window has
/// elapsed since the last relevant event and no build is in
/// flight, kick off a new build task.
fn drain_source_changes(
    mut state: ResMut<HotReloadState>,
    enabled: Res<HotReloadEnabled>,
    mut install_status: ResMut<crate::extensions_dialog::InstallStatus>,
) {
    if !enabled.0 {
        return;
    }
    if state.build_task.is_some() {
        return;
    }
    let Some(project_dir) = state.project_dir.clone() else {
        return;
    };

    let should_build = {
        let Ok(mut slot) = state.pending.lock() else {
            return;
        };
        match *slot {
            Some(t) if t.elapsed() >= DEBOUNCE_WINDOW => {
                *slot = None;
                true
            }
            _ => false,
        }
    };
    if !should_build {
        return;
    }

    let project_name = project_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("project")
        .to_owned();
    install_status.message = Some(format!("Rebuilding `{project_name}` after source change…"));
    info!(
        "HotReload: source changed, rebuilding {}",
        project_dir.display()
    );

    let progress = Arc::new(Mutex::new(BuildProgress::default()));
    state.build_progress = Some(Arc::clone(&progress));

    let project_for_task = project_dir;
    let progress_for_task = Arc::clone(&progress);
    let task = AsyncComputeTaskPool::get().spawn(async move {
        build_extension_project_with_progress(&project_for_task, Some(progress_for_task))
    });
    state.build_task = Some(task);
}

/// Poll the in-flight build. On success: install the new `.so`
/// (atomic rename) and respawn jackdaw via `restart_jackdaw`. On
/// failure: surface the error via `InstallStatus` and leave the
/// current (old) dylib in place so the user can keep working.
fn poll_reload_build(
    mut state: ResMut<HotReloadState>,
    mut install_status: ResMut<crate::extensions_dialog::InstallStatus>,
    mut commands: Commands,
) {
    let Some(task) = state.build_task.as_mut() else {
        return;
    };
    let Some(result) = future::block_on(future::poll_once(task)) else {
        return;
    };
    state.build_task = None;
    state.build_progress = None;

    match result {
        Ok(artifact) => {
            info!(
                "HotReload: build complete, installing {}",
                artifact.display()
            );
            commands.queue(move |world: &mut World| {
                let kind =
                    crate::extensions_dialog::handle_install_from_path(world, artifact.clone());
                if matches!(kind, Some(jackdaw_loader::LoadedKind::Game(_))) {
                    info!(
                        "HotReload: new game dylib installed; respawning to pick it up."
                    );
                    crate::restart::restart_jackdaw();
                }
                // Extensions live-load and don't need a restart.
            });
        }
        Err(err) => {
            warn!("HotReload: build failed: {err}");
            install_status.message = Some(format!("Hot reload build failed: {err}"));
        }
    }
}
