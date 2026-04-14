use bevy::prelude::*;
use jackdaw_feathers::tokens;

use crate::area::{ActiveDockWindow, DockTabContent};

#[derive(Component)]
pub struct DockSidebarContainer;

#[derive(Component)]
pub struct DockSidebarIcon {
    pub window_id: String,
}

pub fn spawn_icon_sidebar_world(
    world: &mut World,
    area_entity: Entity,
    windows: &[(String, String, Option<String>)],
) {
    let first_id = windows.first().map(|(id, _, _)| id.clone());

    let sidebar = world
        .spawn((
            DockSidebarContainer,
            Node {
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                width: Val::Px(30.0),
                padding: UiRect::new(Val::Px(1.0), Val::ZERO, Val::Px(4.0), Val::Px(9.0)),
                flex_shrink: 0.0,
                border: UiRect {
                    left: Val::Px(1.0),
                    top: Val::Px(1.0),
                    bottom: Val::Px(1.0),
                    right: Val::ZERO,
                },
                border_radius: BorderRadius::left(Val::Px(5.0)),
                ..default()
            },
            BackgroundColor(tokens::WINDOW_BG),
            BorderColor::all(tokens::PANEL_BORDER),
            ChildOf(area_entity),
        ))
        .id();

    let icon_group = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            },
            ChildOf(sidebar),
        ))
        .id();

    for (window_id, _name, icon_char) in windows {
        let is_active = Some(window_id) == first_id.as_ref();
        let icon_text = icon_char.as_deref().unwrap_or("?");

        let icon_entity = world
            .spawn((
                DockSidebarIcon {
                    window_id: window_id.clone(),
                },
                Interaction::default(),
                Node {
                    width: Val::Px(29.0),
                    height: Val::Px(30.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::left(Val::Px(2.0)),
                    ..default()
                },
                BorderColor::all(if is_active {
                    tokens::ACCENT_BLUE
                } else {
                    Color::NONE
                }),
                ChildOf(icon_group),
            ))
            .id();

        let mut text_font = TextFont {
            font_size: tokens::ICON_MD,
            ..default()
        };

        if let Some(icon_font_res) = world.get_resource::<crate::IconFontHandle>() {
            text_font.font = icon_font_res.0.clone();
        }

        world.spawn((
            Text::new(icon_text.to_string()),
            text_font,
            TextColor(if is_active {
                tokens::TEXT_PRIMARY
            } else {
                tokens::TAB_INACTIVE_TEXT
            }),
            ChildOf(icon_entity),
        ));
    }

    world
        .entity_mut(area_entity)
        .insert(ActiveDockWindow(first_id));
}

pub fn handle_sidebar_icon_clicks(
    icon_query: Query<(Entity, &DockSidebarIcon, &Interaction, &ChildOf), Changed<Interaction>>,
    all_icons: Query<(Entity, &DockSidebarIcon, &ChildOf)>,
    mut area_query: Query<&mut ActiveDockWindow>,
    content_query: Query<(Entity, &DockTabContent, &ChildOf)>,
    mut node_query: Query<&mut Node>,
    mut border_query: Query<&mut BorderColor>,
    children_query: Query<&Children>,
    mut text_color_query: Query<&mut TextColor>,
    parent_query: Query<&ChildOf>,
) {
    for (_entity, icon, interaction, icon_parent) in icon_query.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }

        let new_id = &icon.window_id;
        let icon_group_entity = icon_parent.parent();

        let Ok(sidebar_parent) = parent_query.get(icon_group_entity) else {
            continue;
        };
        let sidebar_entity = sidebar_parent.parent();

        let Ok(area_parent) = parent_query.get(sidebar_entity) else {
            continue;
        };
        let area_entity = area_parent.parent();

        let Ok(mut active) = area_query.get_mut(area_entity) else {
            continue;
        };

        if active.0.as_ref() == Some(new_id) {
            continue;
        }

        active.0 = Some(new_id.clone());

        for (icon_entity, sib_icon, sib_parent) in all_icons.iter() {
            if sib_parent.parent() != icon_group_entity {
                continue;
            }
            let is_active = &sib_icon.window_id == new_id;
            if let Ok(mut bc) = border_query.get_mut(icon_entity) {
                *bc = BorderColor::all(if is_active {
                    tokens::ACCENT_BLUE
                } else {
                    Color::NONE
                });
            }
            if let Ok(icon_children) = children_query.get(icon_entity) {
                for child in icon_children.iter() {
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

        for (content_entity, content, content_parent) in content_query.iter() {
            if content_parent.parent() != area_entity {
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
