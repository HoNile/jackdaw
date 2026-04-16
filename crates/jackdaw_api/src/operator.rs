use bevy::ecs::system::SystemId;
use bevy::prelude::*;
use bevy_enhanced_input::prelude::InputAction;
use jackdaw_commands::EditorCommand;

/// A Blender-style operator.
///
/// The trait is bounded on [`InputAction`] so the operator type itself can be
/// used as a BEI action:
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
/// Extensions then bind the operator to a key via pure BEI syntax:
///
/// ```ignore
/// ctx.spawn((
///     MyPluginContext,
///     actions!(MyPluginContext[
///         Action::<PlaceCube>::new(),
///         bindings![KeyCode::C],
///     ]),
/// ));
/// ```
pub trait Operator: InputAction + 'static {
    const ID: &'static str;
    const LABEL: &'static str;
    const DESCRIPTION: &'static str = "";

    /// Register the primary execute system. Called once during
    /// `ExtensionContext::register_operator::<Self>()`. The returned
    /// `SystemId` is stored on the operator entity and unregistered on
    /// despawn.
    fn register_execute(commands: &mut Commands) -> SystemId<(), OperatorResult>;

    /// Register an optional poll system. Returns `true` if the operator is
    /// currently callable; `false` skips execution. Default: always callable.
    fn register_poll(_commands: &mut Commands) -> Option<SystemId<(), bool>> {
        None
    }

    /// Register an optional invoke system. Invoke is what UI/keybind/F3
    /// trigger — it may differ from execute (e.g. opens a modal dialog,
    /// starts a drag). Default: identical to execute.
    fn register_invoke(commands: &mut Commands) -> SystemId<(), OperatorResult> {
        Self::register_execute(commands)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorResult {
    /// Operator finished successfully. Any recorded commands are grouped and
    /// pushed to `CommandHistory` as a single undo entry.
    Finished,
    /// Operator explicitly cancelled. Recorded commands are dropped.
    Cancelled,
    /// Operator is in a modal state (drag, dialog). The dispatcher re-runs
    /// the invoke system next frame until `Finished` or `Cancelled`.
    /// Modal support is future work; first pass treats `Running` like
    /// `Finished` to simplify the dispatcher.
    Running,
}

/// Resource operator systems use to record `EditorCommand`s for undo.
///
/// The dispatcher calls [`Self::prepare`] before running the operator and
/// [`Self::take`] after. During the operator's execute/invoke system, the
/// operator pushes commands via [`Self::record`]; the command's `execute`
/// has already been run by the caller (or is implicit in how the system
/// modified state).
///
/// All scene mutations should go through an `EditorCommand`. Operators never
/// mutate `SceneJsnAst` directly.
#[derive(Resource, Default)]
pub struct OperatorCommandBuffer {
    pub(crate) recorded: Vec<Box<dyn EditorCommand>>,
    pub(crate) creates_history_entry: bool,
}

impl OperatorCommandBuffer {
    /// Record an already-executed command for undo. Use this when your
    /// operator system constructs commands that have already been applied
    /// to the world (e.g. by using `cmd.execute(world)` before calling
    /// record, or by doing the mutation directly and then recording a
    /// command that can reverse it on undo).
    pub fn record(&mut self, cmd: Box<dyn EditorCommand>) {
        self.recorded.push(cmd);
    }

    /// Execute a command and record it. Convenience for operators that
    /// have `&mut World` (exclusive systems).
    pub fn execute_and_record(&mut self, mut cmd: Box<dyn EditorCommand>, world: &mut World) {
        cmd.execute(world);
        self.recorded.push(cmd);
    }

    /// Called by the dispatcher before running the operator's invoke system.
    pub(crate) fn prepare(&mut self, creates_history_entry: bool) {
        self.recorded.clear();
        self.creates_history_entry = creates_history_entry;
    }

    /// Called by the dispatcher after the operator finishes. Returns the
    /// recorded commands and whether they should be turned into a history
    /// entry.
    pub(crate) fn take(&mut self) -> (Vec<Box<dyn EditorCommand>>, bool) {
        let recorded = std::mem::take(&mut self.recorded);
        let creates_history = self.creates_history_entry;
        self.creates_history_entry = false;
        (recorded, creates_history)
    }

    /// Whether the current operator run will create a history entry. Useful
    /// if an operator's execute system wants to behave differently when
    /// called from a nested context (e.g. skip dialog prompts).
    pub fn creates_history_entry(&self) -> bool {
        self.creates_history_entry
    }
}
