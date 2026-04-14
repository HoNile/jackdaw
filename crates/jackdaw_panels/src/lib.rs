pub mod add_window_popup;
pub mod area;
pub mod drag;
pub mod layout;
pub mod populate;
pub mod registry;
pub mod sidebar;
pub mod split;
pub mod tabs;
pub mod workspace;
pub mod workspace_tabs;

pub use area::{
    ActiveDockWindow, DockArea, DockAreaStyle, DockTab, DockTabBar, DockTabContent, DockWindow,
    IconFontHandle,
};
pub use layout::{AreaState, LayoutState};
pub use registry::{DockWindowBuildFn, DockWindowDescriptor, WindowRegistry};
pub use sidebar::{DockSidebarContainer, DockSidebarIcon};
pub use split::{Panel, PanelGroup, PanelHandle, panel, panel_group, panel_handle};
pub use workspace::{
    WorkspaceChanged, WorkspaceDescriptor, WorkspaceRegistry, WorkspaceTab, WorkspaceTabStrip,
};

use bevy::prelude::*;

pub struct DockPlugin;

impl Plugin for DockPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            split::SplitPanelPlugin,
            tabs::DockTabPlugin,
            drag::DockDragPlugin,
            add_window_popup::AddWindowPopupPlugin,
        ))
            .init_resource::<WindowRegistry>()
            .init_resource::<WorkspaceRegistry>()
            .add_observer(populate::on_dock_area_added)
            .add_systems(
                Update,
                (
                    sidebar::handle_sidebar_icon_clicks,
                    workspace_tabs::populate_workspace_tabs,
                    workspace_tabs::handle_workspace_tab_clicks,
                    workspace_tabs::update_workspace_tab_visuals,
                ),
            );
    }
}
