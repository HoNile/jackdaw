//! Plugin that wires up the extension framework into the editor.
//!
//! Adds BEI, sets up the required resources (`OperatorCommandBuffer`,
//! `OperatorIndex`, `PanelExtensionRegistry`, `ExtensionCatalog`), and
//! registers the cleanup observers that keep non-ECS state in sync when
//! extension entities are despawned.

use bevy::prelude::*;
use bevy_enhanced_input::prelude::EnhancedInputPlugin;
use jackdaw_api::{
    ExtensionCatalog, OperatorCommandBuffer, OperatorIndex, PanelExtensionRegistry,
    lifecycle::{
        cleanup_panel_extension_on_remove, cleanup_window_on_remove, cleanup_workspace_on_remove,
        deindex_and_cleanup_operator_on_remove, index_operator_on_add,
    },
};

pub struct ExtensionLoaderPlugin;

impl Plugin for ExtensionLoaderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EnhancedInputPlugin)
            .init_resource::<ExtensionCatalog>()
            .init_resource::<OperatorCommandBuffer>()
            .init_resource::<OperatorIndex>()
            .init_resource::<PanelExtensionRegistry>()
            .add_observer(index_operator_on_add)
            .add_observer(deindex_and_cleanup_operator_on_remove)
            .add_observer(cleanup_window_on_remove)
            .add_observer(cleanup_workspace_on_remove)
            .add_observer(cleanup_panel_extension_on_remove);
    }
}
