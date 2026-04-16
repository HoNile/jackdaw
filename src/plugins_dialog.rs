//! `File > Plugins...` dialog. Lets the user enable/disable compiled-in
//! extensions at runtime. Changes are applied immediately via
//! `enable_extension` / `disable_extension` and persisted to
//! `~/.config/jackdaw/extensions.json`.

use bevy::prelude::*;
use jackdaw_api::{Extension, ExtensionCatalog};
use jackdaw_feathers::{
    checkbox::{CheckboxCommitEvent, CheckboxProps, checkbox},
    dialog::{CloseDialogEvent, DialogChildrenSlot, OpenDialogEvent},
    icons::{EditorFont, IconFont},
    tokens,
};

use crate::extensions_config;

pub struct PluginsDialogPlugin;

impl Plugin for PluginsDialogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PluginsDialogOpen>()
            .add_systems(Update, populate_plugins_dialog)
            .add_observer(on_plugin_checkbox_commit)
            .add_observer(on_dialog_closed);
    }
}

/// Clear the open flag whenever any dialog closes. Safe because populate
/// also checks for existing plugin checkboxes — it won't run again until
/// the next open.
fn on_dialog_closed(_: On<CloseDialogEvent>, mut open: ResMut<PluginsDialogOpen>) {
    open.0 = false;
}

/// Set to `true` while the dialog is being shown. Used by the populate
/// system to know whether to fill the dialog's children slot.
#[derive(Resource, Default)]
struct PluginsDialogOpen(bool);

/// Marks a checkbox as belonging to the plugins dialog. Stores the
/// extension name so the commit observer knows which one to toggle.
#[derive(Component)]
struct PluginCheckbox {
    extension_name: String,
}

/// Opened from `File > Plugins...`. Called from the menu action handler.
pub fn open_plugins_dialog(world: &mut World) {
    world.resource_mut::<PluginsDialogOpen>().0 = true;
    world.trigger(
        OpenDialogEvent::new("Plugins", "Close")
            .without_cancel()
            .with_max_width(Val::Px(380.0)),
    );
}

/// Populate the dialog's children slot with a row per catalog entry.
/// Runs each frame but short-circuits unless the dialog is currently open
/// and hasn't been populated yet.
///
/// Note: we do NOT filter on `&Children` because a freshly-spawned
/// `DialogChildrenSlot` with no children doesn't have a `Children`
/// component at all — the query would never match. Instead we rely on the
/// `PluginCheckbox` existence check to avoid re-populating.
fn populate_plugins_dialog(
    mut commands: Commands,
    catalog: Res<ExtensionCatalog>,
    open: Res<PluginsDialogOpen>,
    slots: Query<Entity, With<DialogChildrenSlot>>,
    loaded: Query<&Extension>,
    editor_font: Res<EditorFont>,
    icon_font: Res<IconFont>,
    existing: Query<(), With<PluginCheckbox>>,
) {
    if !open.0 {
        return;
    }
    if !existing.is_empty() {
        return;
    }
    let Some(slot_entity) = slots.iter().next() else {
        return;
    };

    let font = editor_font.0.clone();
    let ifont = icon_font.0.clone();

    // Collect (name, is_enabled) for each catalog entry, sorted for
    // stable ordering across runs.
    let enabled_names: std::collections::HashSet<String> =
        loaded.iter().map(|e| e.name.clone()).collect();
    let mut rows: Vec<(String, bool)> = catalog
        .iter()
        .map(|n| {
            let is_enabled = enabled_names.contains(n);
            (n.to_string(), is_enabled)
        })
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));

    let list = commands
        .spawn((
            ChildOf(slot_entity),
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(tokens::SPACING_XS),
                min_width: Val::Px(280.0),
                ..default()
            },
        ))
        .id();

    for (name, checked) in rows {
        let label = prettify(&name);
        commands.spawn((
            ChildOf(list),
            PluginCheckbox {
                extension_name: name.clone(),
            },
            checkbox(CheckboxProps::new(label).checked(checked), &font, &ifont),
        ));
    }
}

/// Observer: when a plugin checkbox commits, enable/disable the matching
/// extension and rewrite the enabled list.
fn on_plugin_checkbox_commit(
    event: On<CheckboxCommitEvent>,
    checkboxes: Query<&PluginCheckbox>,
    mut commands: Commands,
) {
    let Ok(cb) = checkboxes.get(event.entity) else {
        return;
    };
    let name = cb.extension_name.clone();
    let checked = event.checked;

    commands.queue(move |world: &mut World| {
        if checked {
            jackdaw_api::enable_extension(world, &name);
        } else {
            jackdaw_api::disable_extension(world, &name);
        }
        extensions_config::persist_current_enabled(world);
    });
}

/// Convert `"jackdaw.asset_browser"` → `"Asset Browser"`.
fn prettify(name: &str) -> String {
    let stripped = name.strip_prefix("jackdaw.").unwrap_or(name);
    let mut out = String::new();
    for (i, part) in stripped.split(&['_', '.'][..]).enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let mut chars = part.chars();
        if let Some(c) = chars.next() {
            out.extend(c.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}
