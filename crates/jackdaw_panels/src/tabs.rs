use bevy::prelude::*;
use jackdaw_feathers::{icons::IconFont, tokens};
use lucide_icons::Icon;

use crate::area::{ActiveDockWindow, DockArea, DockTab, DockTabBar, DockTabContent};

#[derive(Component)]
pub struct DockTabAddButton {
    pub area_id: String,
}

#[derive(Component)]
pub struct DockTabGrip;

#[derive(Component)]
pub struct DockTabRow;

pub struct DockTabPlugin;

impl Plugin for DockTabPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (handle_dock_tab_clicks, show_close_on_hover))
            .add_observer(on_close_button_click);
    }
}

pub fn spawn_tab_bar_world(
    world: &mut World,
    area_entity: Entity,
    tabs: &[(String, String)],
) {
    let first_id = tabs.first().map(|(id, _)| id.clone());

    let tab_bar = world
        .spawn((
            DockTabBar,
            Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                width: Val::Percent(100.0),
                height: Val::Px(tokens::PANEL_TAB_HEIGHT),
                padding: UiRect::new(
                    Val::Px(tokens::SPACING_MD),
                    Val::Px(tokens::SPACING_MD),
                    Val::Px(1.0),
                    Val::ZERO,
                ),
                flex_shrink: 0.0,
                border: UiRect {
                    left: Val::Px(1.0),
                    right: Val::Px(1.0),
                    top: Val::Px(1.0),
                    bottom: Val::ZERO,
                },
                border_radius: BorderRadius::top(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(tokens::PANEL_HEADER_BG),
            BorderColor::all(tokens::PANEL_BORDER),
            ChildOf(area_entity),
        ))
        .id();

    let tab_row = world
        .spawn((
            DockTabRow,
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(tokens::SPACING_XS),
                height: Val::Percent(100.0),
                overflow: Overflow::scroll_x(),
                flex_shrink: 1.0,
                min_width: Val::Px(0.0),
                ..default()
            },
            ChildOf(tab_bar),
        ))
        .id();

    for (window_id, label) in tabs {
        let is_active = Some(window_id) == first_id.as_ref();
        spawn_tab(world, tab_row, window_id, label, is_active);
    }

    let area_id = world
        .entity(area_entity)
        .get::<DockArea>()
        .map(|a| a.id.clone())
        .unwrap_or_default();

    let icon_font = world
        .get_resource::<IconFont>()
        .map(|f| f.0.clone());

    let right_row = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: Val::Px(tokens::SPACING_SM),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(tab_bar),
        ))
        .id();

    if let Some(ref font_handle) = icon_font {
        world.spawn((
            DockTabAddButton {
                area_id: area_id.clone(),
            },
            Interaction::default(),
            Node {
                width: Val::Px(15.0),
                height: Val::Px(15.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            ChildOf(right_row),
            children![(
                Text::new(String::from(Icon::Plus.unicode())),
                TextFont {
                    font: font_handle.clone(),
                    font_size: tokens::ICON_SM,
                    ..default()
                },
                TextColor(tokens::TAB_INACTIVE_TEXT),
            )],
        ));

        world.spawn((
            DockTabGrip,
            Interaction::default(),
            Node {
                width: Val::Px(15.0),
                height: Val::Px(15.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            ChildOf(right_row),
            children![(
                Text::new(String::from(Icon::GripVertical.unicode())),
                TextFont {
                    font: font_handle.clone(),
                    font_size: tokens::ICON_SM,
                    ..default()
                },
                TextColor(tokens::TAB_INACTIVE_TEXT),
            )],
        ));
    }
}

pub fn spawn_tab_in_world(
    world: &mut World,
    tab_row: Entity,
    window_id: &str,
    label: &str,
    is_active: bool,
) {
    spawn_tab(world, tab_row, window_id, label, is_active);
}

fn spawn_tab(
    world: &mut World,
    tab_row: Entity,
    window_id: &str,
    label: &str,
    is_active: bool,
) {
    let tab_bg = if is_active { tokens::TAB_ACTIVE_BG } else { Color::NONE };
    let border_top = if is_active { Val::Px(2.0) } else { Val::ZERO };
    let border_color = if is_active { tokens::TAB_ACTIVE_BORDER } else { Color::NONE };
    let text_color = if is_active { tokens::TEXT_PRIMARY } else { tokens::TAB_INACTIVE_TEXT };

    let tab_entity = world
        .spawn((
            DockTab {
                window_id: window_id.to_string(),
            },
            Interaction::default(),
            Node {
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: Val::Px(tokens::SPACING_XS),
                padding: UiRect::horizontal(Val::Px(8.0)),
                height: Val::Percent(100.0),
                flex_shrink: 0.0,
                border: UiRect {
                    top: border_top,
                    ..default()
                },
                border_radius: BorderRadius::top(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(tab_bg),
            BorderColor::all(border_color),
            ChildOf(tab_row),
        ))
        .id();

    world.spawn((
        Text::new(label.to_string()),
        TextLayout::new_with_linebreak(LineBreak::NoWrap),
        TextFont {
            font_size: tokens::TEXT_SIZE_LG,
            ..default()
        },
        TextColor(text_color),
        ChildOf(tab_entity),
    ));

    let icon_font = world
        .get_resource::<IconFont>()
        .map(|f| f.0.clone());

    if let Some(font_handle) = icon_font {
        world.spawn((
            crate::area::DockTabCloseButton {
                window_id: window_id.to_string(),
            },
            Interaction::default(),
            Node {
                width: Val::Px(14.0),
                height: Val::Px(14.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border_radius: BorderRadius::all(Val::Px(2.0)),
                display: Display::None,
                ..default()
            },
            ChildOf(tab_entity),
            children![(
                Text::new(String::from(Icon::X.unicode())),
                TextFont {
                    font: font_handle,
                    font_size: 10.0,
                    ..default()
                },
                TextColor(tokens::TAB_INACTIVE_TEXT),
            )],
        ));
    }
}

fn handle_dock_tab_clicks(
    tab_query: Query<(Entity, &DockTab, &Interaction, &ChildOf), Changed<Interaction>>,
    mut area_query: Query<&mut ActiveDockWindow>,
    all_tabs: Query<(Entity, &DockTab, &ChildOf)>,
    mut bg_query: Query<&mut BackgroundColor>,
    mut border_query: Query<&mut BorderColor>,
    mut node_query: Query<&mut Node>,
    children_query: Query<&Children>,
    mut text_color_query: Query<&mut TextColor>,
    content_query: Query<(Entity, &DockTabContent, &ChildOf)>,
    parent_query: Query<&ChildOf>,
) {
    for (_clicked_entity, clicked_tab, interaction, tab_child_of) in tab_query.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }

        let new_id = &clicked_tab.window_id;
        let tab_row_entity = tab_child_of.parent();

        // Walk up: tab → tab_row → tab_bar → area
        let Ok(tab_row_parent) = parent_query.get(tab_row_entity) else {
            continue;
        };
        let tab_bar_entity = tab_row_parent.parent();

        let Ok(tab_bar_parent) = parent_query.get(tab_bar_entity) else {
            continue;
        };
        let area_entity = tab_bar_parent.parent();

        let Ok(mut active) = area_query.get_mut(area_entity) else {
            continue;
        };

        if active.0.as_ref() == Some(new_id) {
            continue;
        }

        active.0 = Some(new_id.clone());

        // Update all sibling tabs' visuals
        for (tab_entity, tab, tab_co) in all_tabs.iter() {
            if tab_co.parent() != tab_row_entity {
                continue;
            }

            let is_active = &tab.window_id == new_id;

            if let Ok(mut bg) = bg_query.get_mut(tab_entity) {
                bg.0 = if is_active {
                    tokens::TAB_ACTIVE_BG
                } else {
                    Color::NONE
                };
            }
            if let Ok(mut bc) = border_query.get_mut(tab_entity) {
                *bc = BorderColor::all(if is_active {
                    tokens::TAB_ACTIVE_BORDER
                } else {
                    Color::NONE
                });
            }
            if let Ok(mut node) = node_query.get_mut(tab_entity) {
                node.border.top = if is_active { Val::Px(2.0) } else { Val::ZERO };
            }
            if let Ok(tab_children) = children_query.get(tab_entity) {
                for child in tab_children.iter() {
                    if let Ok(mut tc) = text_color_query.get_mut(child) {
                        tc.0 = if is_active {
                            tokens::TEXT_PRIMARY
                        } else {
                            tokens::TAB_INACTIVE_TEXT
                        };
                    }
                }
            }
        }

        // Toggle content visibility
        for (content_entity, content, content_co) in content_query.iter() {
            if content_co.parent() != area_entity {
                continue;
            }
            if let Ok(mut node) = node_query.get_mut(content_entity) {
                node.display = if &content.window_id == new_id {
                    Display::Flex
                } else {
                    Display::None
                };
            }
        }
    }
}

fn show_close_on_hover(
    tabs: Query<(Entity, &Interaction, &Children), (Changed<Interaction>, With<DockTab>)>,
    mut close_buttons: Query<&mut Node, With<crate::area::DockTabCloseButton>>,
) {
    for (_tab_entity, interaction, children) in tabs.iter() {
        let show = *interaction == Interaction::Hovered || *interaction == Interaction::Pressed;
        for child in children.iter() {
            if let Ok(mut node) = close_buttons.get_mut(child) {
                node.display = if show { Display::Flex } else { Display::None };
            }
        }
    }
}

fn on_close_button_click(
    trigger: On<Pointer<Click>>,
    close_buttons: Query<&crate::area::DockTabCloseButton>,
    mut commands: Commands,
) {
    let entity = trigger.event_target();
    let Ok(close_btn) = close_buttons.get(entity) else {
        return;
    };

    let window_id = close_btn.window_id.clone();
    info!("Close tab: {}", window_id);

    commands.queue(move |world: &mut World| {
        remove_window_from_area(world, &window_id);
    });
}

fn remove_window_from_area(world: &mut World, window_id: &str) {
    // Find and remove the tab
    let tab_entity = {
        let mut query = world.query::<(Entity, &DockTab)>();
        query
            .iter(world)
            .find(|(_, tab)| tab.window_id == window_id)
            .map(|(e, _)| e)
    };
    if let Some(tab) = tab_entity {
        world.entity_mut(tab).despawn();
    }

    // Find the content entity and its parent area
    let (content_entity, area_entity) = {
        let mut query = world.query::<(Entity, &DockTabContent, &ChildOf)>();
        let found = query
            .iter(world)
            .find(|(_, c, _)| c.window_id == window_id)
            .map(|(e, _, co)| (e, co.parent()));
        match found {
            Some(pair) => pair,
            None => return,
        }
    };

    world.entity_mut(content_entity).despawn();

    // Activate another tab if the closed one was active
    let remaining: Vec<String> = {
        let mut query = world.query::<(&DockTabContent, &ChildOf)>();
        query
            .iter(world)
            .filter(|(_, co)| co.parent() == area_entity)
            .map(|(c, _)| c.window_id.clone())
            .collect()
    };

    if let Some(first) = remaining.first() {
        let first_id = first.clone();
        if let Some(mut active) = world.entity_mut(area_entity).get_mut::<ActiveDockWindow>() {
            if active.0.as_deref() == Some(window_id) {
                active.0 = Some(first_id.clone());
            }
        }
        let mut query = world.query::<(Entity, &DockTabContent, &ChildOf)>();
        let to_show: Vec<Entity> = query
            .iter(world)
            .filter(|(_, c, co)| co.parent() == area_entity && c.window_id == first_id)
            .map(|(e, _, _)| e)
            .collect();
        for entity in to_show {
            if let Some(mut node) = world.entity_mut(entity).get_mut::<Node>() {
                node.display = Display::Flex;
            }
        }

        // Update tab visuals
        let mut tab_query = world.query::<(Entity, &DockTab, &Children)>();
        let tab_updates: Vec<(Entity, bool, Vec<Entity>)> = tab_query
            .iter(world)
            .map(|(e, t, children)| (e, t.window_id == first_id, children.iter().collect()))
            .collect();
        for (tab_entity, is_active, children) in tab_updates {
            if let Some(mut bg) = world.entity_mut(tab_entity).get_mut::<BackgroundColor>() {
                bg.0 = if is_active { tokens::TAB_ACTIVE_BG } else { Color::NONE };
            }
            if let Some(mut bc) = world.entity_mut(tab_entity).get_mut::<BorderColor>() {
                *bc = BorderColor::all(if is_active { tokens::TAB_ACTIVE_BORDER } else { Color::NONE });
            }
            if let Some(mut node) = world.entity_mut(tab_entity).get_mut::<Node>() {
                node.border.top = if is_active { Val::Px(2.0) } else { Val::ZERO };
            }
            for child in children {
                if let Some(mut tc) = world.entity_mut(child).get_mut::<TextColor>() {
                    tc.0 = if is_active { tokens::TEXT_PRIMARY } else { tokens::TAB_INACTIVE_TEXT };
                }
            }
        }
    }
}
