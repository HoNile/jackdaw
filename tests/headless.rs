use std::sync::Arc;

use bevy::{
    prelude::*,
    render::{
        RenderPlugin,
        settings::{RenderCreation, WgpuSettings},
    },
    winit::WinitPlugin,
};
use jackdaw::prelude::*;
use jackdaw_api::prelude::*;

fn headless_app() -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(RenderPlugin {
                render_creation: RenderCreation::Automatic(WgpuSettings {
                    backends: None,
                    ..default()
                }),
                ..default()
            })
            .disable::<WinitPlugin>(),
    )
    .add_plugins(EditorPlugin);
    app
}

#[test]
fn smoke_test_headless_update() {
    let mut app = headless_app();
    app.finish();

    for _ in 0..10 {
        app.update();
    }
}

#[test]
fn can_run_extension() {
    let mut app = headless_app();
    app.register_extension::<SampleExtension>();
    app.finish();
    // first update sets the extension up
    // todo: maybe do this in `Startup`?
    app.update();
    for _ in 0..10 {
        let result = app.world_mut().call_operator(SampleExtension::OP).unwrap();
        assert_eq!(result, OperatorResult::Finished);
        app.update();
    }
}

#[test]
fn can_call_operator() {
    let mut app = headless_app();
    app.register_extension::<SampleExtension>();
    app.finish();
    app.update();

    let amount_of_panels = app
        .world_mut()
        .query_filtered::<(), With<Panel>>()
        .iter(app.world())
        .count();
    // TODO: why is this panel not spawned?
    assert_eq!(amount_of_panels, 0);
    let amount_of_markers = app
        .world_mut()
        .query_filtered::<(), With<Marker>>()
        .iter(app.world())
        .count();
    assert_eq!(amount_of_markers, 0);

    let result = app.world_mut().call_operator(SampleExtension::OP).unwrap();
    assert_eq!(result, OperatorResult::Finished);

    let amount_of_markers = app
        .world_mut()
        .query_filtered::<(), With<Marker>>()
        .iter(app.world())
        .count();
    assert_eq!(amount_of_markers, 1);
}

#[derive(Default)]
pub struct SampleExtension;

impl SampleExtension {
    const OP: &'static str = "sample.spawn";
}

impl JackdawExtension for SampleExtension {
    fn name(&self) -> &str {
        "sample"
    }

    fn register(&self, ctx: &mut ExtensionContext) {
        ctx.register_window(WindowDescriptor {
            id: Self::OP.into(),
            name: "Hello Extension".into(),
            build: Arc::new(build_panel),
            default_area: Some("left".into()),
            ..default()
        });
        ctx.register_operator::<SpawnMarkerOp>();
    }
}

fn build_panel(world: &mut World, parent: Entity) {
    world.spawn((ChildOf(parent), Panel, Text::new("Some panel")));
}

#[derive(Component, Default)]
pub struct SampleContext;

#[operator(
    // TODO: replace with `SampleExtension::OP`
    id = "sample.spawn",
    label = "Spawn Marker",
    name = "SpawnMarkerOp"
)]
fn spawn_marker(mut commands: Commands) -> OperatorResult {
    commands.spawn(Marker);
    OperatorResult::Finished
}

#[derive(Component)]
struct Marker;

#[derive(Component)]
struct Panel;
