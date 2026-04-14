use bevy::prelude::*;

use crate::{DockArea, DockAreaStyle, DockTabContent, DockWindow, WindowRegistry, sidebar, tabs};

pub fn on_dock_area_added(trigger: On<Add, DockArea>, mut commands: Commands) {
    let area_entity = trigger.event_target();
    commands.queue(move |world: &mut World| {
        populate_single_area(world, area_entity);
    });
}

fn populate_single_area(world: &mut World, area_entity: Entity) {
    let (area_id, area_style) = {
        let Some(area) = world.entity(area_entity).get::<DockArea>() else {
            return;
        };
        (area.id.clone(), area.style.clone())
    };

    struct WindowInfo {
        id: String,
        name: String,
        icon: Option<String>,
        build: crate::DockWindowBuildFn,
    }

    let windows: Vec<WindowInfo> = {
        let Some(registry) = world.get_resource::<WindowRegistry>() else {
            return;
        };
        registry
            .by_area(&area_id)
            .iter()
            .map(|w| WindowInfo {
                id: w.id.clone(),
                name: w.name.clone(),
                icon: w.icon.clone(),
                build: w.build.clone(),
            })
            .collect()
    };

    if windows.is_empty() {
        return;
    }

    match area_style {
        DockAreaStyle::TabBar => {
            let tab_data: Vec<(String, String)> = windows
                .iter()
                .map(|w| (w.id.clone(), w.name.clone()))
                .collect();
            tabs::spawn_tab_bar_world(world, area_entity, &tab_data);
        }
        DockAreaStyle::IconSidebar => {
            let items: Vec<(String, String, Option<String>)> = windows
                .iter()
                .map(|w| (w.id.clone(), w.name.clone(), w.icon.clone()))
                .collect();
            sidebar::spawn_icon_sidebar_world(world, area_entity, &items);
        }
        DockAreaStyle::Headless => {}
    }

    let first_id = windows.first().map(|w| w.id.clone());

    for (i, window) in windows.iter().enumerate() {
        let content_entity = world
            .spawn((
                DockWindow {
                    descriptor_id: window.id.clone(),
                },
                DockTabContent {
                    window_id: window.id.clone(),
                },
                Node {
                    flex_grow: 1.0,
                    width: Val::Percent(100.0),
                    min_height: Val::Px(0.0),
                    flex_direction: FlexDirection::Column,
                    overflow: Overflow::clip(),
                    display: if i == 0 { Display::Flex } else { Display::None },
                    ..default()
                },
                ChildOf(area_entity),
            ))
            .id();

        (window.build)(world, content_entity);
    }

    world
        .entity_mut(area_entity)
        .insert(crate::ActiveDockWindow(first_id));
}
