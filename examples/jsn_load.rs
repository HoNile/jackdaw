//! Minimal example: load a `.jsn` scene exported from the Jackdaw editor.
//!
//! Place a scene file at `assets/examples/scenes/scene.jsn` (use the editor's
//! Ctrl+S to export one), then run:
//!
//! ```sh
//! cargo run --example jsn_load
//! ```

use bevy::prelude::*;
use jackdaw_runtime::{JackdawPlugin, JackdawSceneRoot};

fn main() -> AppExit {
    App::new()
        .add_plugins((DefaultPlugins, JackdawPlugin))
        .add_systems(Startup, setup)
        .run()
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn(JackdawSceneRoot(
        asset_server.load("examples/scenes/scene.jsn"),
    ));

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(5.0, 5.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}
