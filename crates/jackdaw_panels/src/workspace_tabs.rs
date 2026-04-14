use bevy::prelude::*;

use crate::{
    IconFontHandle,
    workspace::{WorkspaceChanged, WorkspaceRegistry, WorkspaceTab, WorkspaceTabStrip},
};

const TAB_ACTIVE_BG: Color = Color::srgba(1.0, 1.0, 1.0, 0.06);
const TAB_ACTIVE_BORDER: Color = Color::srgba(1.0, 1.0, 1.0, 0.1);
const TAB_ACTIVE_LABEL: Color = Color::srgba(1.0, 1.0, 1.0, 0.9);
const TAB_INACTIVE_LABEL: Color = Color::srgba(1.0, 1.0, 1.0, 0.4);
const ACCENT_SCENE: Color = Color::srgba(0.35, 0.55, 1.0, 0.8);

pub fn populate_workspace_tabs(world: &mut World) {
    let mut strips: Vec<Entity> = Vec::new();
    {
        let mut query =
            world.query_filtered::<Entity, (With<WorkspaceTabStrip>, Without<WorkspaceTab>)>();
        for entity in query.iter(world) {
            if world
                .entity(entity)
                .get::<Children>()
                .map_or(true, |c| c.is_empty())
            {
                strips.push(entity);
            }
        }
    }

    if strips.is_empty() {
        return;
    }

    let registry = world.remove_resource::<WorkspaceRegistry>().unwrap();
    let icon_font = world
        .get_resource::<IconFontHandle>()
        .map(|f| f.0.clone());

    for strip_entity in strips {
        for workspace in registry.iter() {
            let is_active = registry.active.as_ref() == Some(&workspace.id);

            let bg = if is_active { TAB_ACTIVE_BG } else { Color::NONE };
            let border = if is_active {
                TAB_ACTIVE_BORDER
            } else {
                Color::NONE
            };
            let label_color = if is_active {
                TAB_ACTIVE_LABEL
            } else {
                TAB_INACTIVE_LABEL
            };

            let tab_entity = world
                .spawn((
                    WorkspaceTab {
                        workspace_id: workspace.id.clone(),
                    },
                    Interaction::default(),
                    Node {
                        flex_direction: FlexDirection::Row,
                        align_items: AlignItems::Center,
                        padding: UiRect::axes(Val::Px(7.0), Val::Px(4.0)),
                        column_gap: Val::Px(5.0),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(bg),
                    BorderColor::all(border),
                    ChildOf(strip_entity),
                ))
                .id();

            world.spawn((
                Node {
                    width: Val::Px(2.5),
                    height: Val::Px(12.0),
                    border_radius: BorderRadius::all(Val::Px(5.0)),
                    ..default()
                },
                BackgroundColor(workspace.accent_color),
                ChildOf(tab_entity),
            ));

            if let Some(ref icon_char) = workspace.icon {
                let mut font = TextFont {
                    font_size: 12.0,
                    ..default()
                };
                if let Some(ref handle) = icon_font {
                    font.font = handle.clone();
                }
                world.spawn((
                    Text::new(icon_char.clone()),
                    font,
                    TextColor(label_color),
                    ChildOf(tab_entity),
                ));
            }

            world.spawn((
                Text::new(workspace.name.clone()),
                TextFont {
                    font_size: 11.0,
                    ..default()
                },
                TextColor(label_color),
                ChildOf(tab_entity),
            ));
        }
    }

    world.insert_resource(registry);
}

pub fn handle_workspace_tab_clicks(
    tab_query: Query<(&WorkspaceTab, &Interaction), Changed<Interaction>>,
    mut registry: ResMut<WorkspaceRegistry>,
    mut commands: Commands,
) {
    for (tab, interaction) in tab_query.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }

        let new_id = &tab.workspace_id;
        if registry.active.as_ref() == Some(new_id) {
            continue;
        }

        let old = registry.active.clone();
        registry.set_active(new_id);
        commands.trigger(WorkspaceChanged {
            old,
            new: new_id.clone(),
        });
    }
}

pub fn update_workspace_tab_visuals(
    registry: Res<WorkspaceRegistry>,
    tabs: Query<(Entity, &WorkspaceTab)>,
    mut bg_query: Query<&mut BackgroundColor>,
    mut border_query: Query<&mut BorderColor>,
    children_query: Query<&Children>,
    mut text_color_query: Query<&mut TextColor>,
) {
    if !registry.is_changed() {
        return;
    }

    for (tab_entity, tab) in tabs.iter() {
        let is_active = registry.active.as_ref() == Some(&tab.workspace_id);

        if let Ok(mut bg) = bg_query.get_mut(tab_entity) {
            bg.0 = if is_active { TAB_ACTIVE_BG } else { Color::NONE };
        }
        if let Ok(mut bc) = border_query.get_mut(tab_entity) {
            *bc = BorderColor::all(if is_active {
                TAB_ACTIVE_BORDER
            } else {
                Color::NONE
            });
        }

        if let Ok(children) = children_query.get(tab_entity) {
            for child in children.iter() {
                if let Ok(mut tc) = text_color_query.get_mut(child) {
                    tc.0 = if is_active {
                        TAB_ACTIVE_LABEL
                    } else {
                        TAB_INACTIVE_LABEL
                    };
                }
            }
        }
    }
}
