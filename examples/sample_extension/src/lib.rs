//! Sample extension demonstrating the jackdaw_api v2 surface.
//!
//! Registers:
//! - A "Hello Extension" dock window (plain UI, no input).
//! - A `HelloOp` operator bound to `F9` via a BEI context owned by this plugin.
//!
//! Toggling this plugin off via `File > Plugins...` should make both the
//! window entry disappear from the add-window popup and the keybind go dead.

use std::sync::Arc;

use bevy::ecs::system::SystemId;
use bevy::prelude::*;
use bevy_enhanced_input::prelude::*;
use jackdaw_api::prelude::*;

pub struct SampleExtension;

impl JackdawExtension for SampleExtension {
    fn name(&self) -> &str {
        "sample"
    }

    fn register_input_contexts(&self, app: &mut App) {
        app.add_input_context::<SampleContext>();
    }

    fn register(&self, ctx: &mut ExtensionContext) {
        // 1. Register the dock window (same behavior as the v1 sample).
        ctx.register_window(WindowDescriptor {
            id: "sample.hello".into(),
            name: "Hello Extension".into(),
            icon: None,
            default_area: None,
            priority: None,
            build: Arc::new(build_hello_panel),
        });

        // 2. Register the operator — internally registers the execute
        // system, stores an OperatorEntity as a child of the extension
        // entity, and spawns a Fire<HelloOp> observer that dispatches it.
        ctx.register_operator::<HelloOp>();

        // 3. Spawn a BEI context entity with an Action<HelloOp> bound to
        // F9. The context *type* was registered once at startup via
        // `register_input_contexts`; this is just the runtime entity that
        // holds the action bindings. Spawned via ctx.spawn so it's a
        // child of the extension entity (cleaned up on disable).
        ctx.spawn((
            SampleContext,
            actions!(SampleContext[
                (Action::<HelloOp>::new(), bindings![KeyCode::F9]),
            ]),
        ));
    }
}

fn build_hello_panel(world: &mut World, parent: Entity) {
    world.spawn((ChildOf(parent), Text::new("Hello from an extension!")));
}

/// BEI input context. One per extension.
#[derive(Component, Default)]
pub struct SampleContext;

/// Operator bound to F9. Emits a log message.
#[derive(Default, InputAction)]
#[action_output(bool)]
pub struct HelloOp;

impl Operator for HelloOp {
    const ID: &'static str = "sample.hello";
    const LABEL: &'static str = "Hello";
    const DESCRIPTION: &'static str = "Logs a hello message";

    fn register_execute(commands: &mut Commands) -> SystemId<(), OperatorResult> {
        commands.register_system(hello_op_system)
    }
}

fn hello_op_system() -> OperatorResult {
    info!("Hello from the sample extension operator!");
    OperatorResult::Finished
}
