use bevy::prelude::*;

#[derive(Resource, Clone, Default)]
pub struct IconFontHandle(pub Handle<Font>);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum DockAreaStyle {
    #[default]
    TabBar,
    IconSidebar,
    /// No tab bar — the panel content provides its own header.
    /// Used for single-window areas or panels with internal tabs.
    Headless,
}

#[derive(Component, Clone, Debug)]
pub struct DockArea {
    pub id: String,
    pub style: DockAreaStyle,
}

#[derive(Component, Clone, Debug)]
pub struct DockWindow {
    pub descriptor_id: String,
}

#[derive(Component, Clone, Debug, Default)]
pub struct ActiveDockWindow(pub Option<String>);

#[derive(Component)]
pub struct DockTabBar;

#[derive(Component)]
pub struct DockTab {
    pub window_id: String,
}

#[derive(Component)]
pub struct DockTabCloseButton {
    pub window_id: String,
}

#[derive(Component)]
pub struct DockTabContent {
    pub window_id: String,
}
