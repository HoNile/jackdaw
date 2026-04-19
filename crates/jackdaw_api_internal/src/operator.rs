use bevy::ecs::system::SystemId;
use bevy::prelude::*;
use bevy_enhanced_input::prelude::InputAction;

/// A Blender-style operator.
///
/// The trait is bounded on [`InputAction`] so the operator type itself
/// can be used as a BEI action:
///
/// ```ignore
/// use bevy_enhanced_input::prelude::*;
///
/// #[derive(Default, InputAction)]
/// #[action_output(bool)]
/// struct PlaceCube;
///
/// impl Operator for PlaceCube {
///     const ID: &'static str = "sample.place_cube";
///     const LABEL: &'static str = "Place Cube";
///
///     fn register_execute(commands: &mut Commands) -> SystemId<(), OperatorResult> {
///         commands.register_system(place_cube_system)
///     }
/// }
/// ```
///
/// Extensions then bind the operator to a key via pure BEI syntax. Use
/// BEI binding modifiers (`Press`, `Release`, `Hold`) when specific
/// input timing is needed:
///
/// ```ignore
/// ctx.spawn((
///     MyPluginContext,
///     actions!(MyPluginContext[
///         (Action::<PlaceCube>::new(), bindings![KeyCode::C]),
///     ]),
/// ));
/// ```
pub trait Operator: InputAction + 'static {
    const ID: &'static str;
    const LABEL: &'static str;
    const DESCRIPTION: &'static str = "";

    /// Whether an observer should be auto-wired to call this operator.
    ///
    /// When `false` (default), registration spawns a `Fire<Self>`
    /// observer that dispatches the operator whenever any bound input
    /// fires it. Authors shape timing via BEI binding modifiers
    /// (`Press`, `Release`, `Hold`, etc.) on the binding.
    ///
    /// When `true`, no observer is spawned. The operator is invocable
    /// only through `World::call_operator(Self::ID)`. Useful for
    /// operators driven by menus, UI buttons, or F3-search without
    /// a keybind.
    const MANUAL: bool = false;

    /// Modal operators stay active across frames.
    ///
    /// When `MODAL = true` and the invoke system returns
    /// [`OperatorResult::Running`], the dispatcher re-runs the invoke
    /// system every frame until it returns `Finished` or `Cancelled`.
    /// The scene snapshot captured at `Start` is diffed against the
    /// state at `Finished`, so the whole session commits as one undo
    /// entry.
    ///
    /// When `MODAL = false` (default), `Running` is treated like
    /// `Finished` and one invoke runs to completion.
    const MODAL: bool = false;

    /// Register the primary execute system. Called once during
    /// `ExtensionContext::register_operator::<Self>()`. The returned
    /// `SystemId` is stored on the operator entity and unregistered
    /// on despawn.
    fn register_execute(commands: &mut Commands) -> SystemId<(), OperatorResult>;

    /// Register an optional availability check. Returns `true` if the
    /// operator can run in the current editor state, `false` if it
    /// should be skipped. Default: always callable.
    fn register_availability_check(_commands: &mut Commands) -> Option<SystemId<(), bool>> {
        None
    }

    /// Register an optional invoke system. `invoke` is what UI,
    /// keybinds, and F3 search run; it can differ from `execute`
    /// when the caller wants to open a dialog or start a drag before
    /// the primary work happens. Defaults to `execute`.
    fn register_invoke(commands: &mut Commands) -> SystemId<(), OperatorResult> {
        Self::register_execute(commands)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorResult {
    /// Operator finished successfully. The dispatcher captures the
    /// resulting scene diff as a single undo entry.
    Finished,
    /// Operator explicitly cancelled. No history entry is pushed.
    Cancelled,
    /// Operator is in a modal session (drag, dialog, multi-frame
    /// edit). The dispatcher re-runs the invoke system every frame
    /// until it returns `Finished` or `Cancelled`. Non-modal
    /// operators that return `Running` collapse to `Finished`.
    Running,
}
