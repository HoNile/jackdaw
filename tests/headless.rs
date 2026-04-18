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
    // todo: maybe do plugin setup in `Startup` so that jackdaw is actually ready in the first frame?
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
    // TODO: why is this panel not spawned? What do I need to do in order to make it spawn?
    assert_eq!(amount_of_panels, 0);
    assert!(!app.world_mut().contains_resource::<Marker>());

    let result = app.world_mut().call_operator(SampleExtension::OP).unwrap();
    assert_eq!(result, OperatorResult::Finished);

    assert!(app.world_mut().contains_resource::<Marker>());
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
    // TODO: replace with `SampleExtension::OP`. The current `operator` macro requires a string here, but an expression would be better
    // see #[require(...)]
    id = "sample.spawn",
    label = "Spawn Marker",
    // TODO: This bit here feels clunky. Could we use `heck` to generate the name in PascalCase for us?
    name = "SpawnMarkerOp"
)]
fn spawn_marker(mut commands: Commands) -> OperatorResult {
    commands.init_resource::<Marker>();
    OperatorResult::Finished
}

#[derive(Resource, Default)]
struct Marker;

#[derive(Component)]
struct Panel;
