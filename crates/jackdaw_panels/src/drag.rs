use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;
use jackdaw_feathers::tokens;

use crate::area::{DockArea, DockTab};
use crate::sidebar::DockSidebarIcon;
use crate::tabs::DockTabGrip;

const DRAG_THRESHOLD: f32 = 5.0;

#[derive(Resource, Default, Debug)]
pub enum DockDragState {
    #[default]
    Idle,
    PendingDrag {
        source_tab: Entity,
        window_id: String,
        window_name: String,
        start_pos: Vec2,
    },
    Dragging {
        source_tab: Entity,
        window_id: String,
        window_name: String,
        source_area: Entity,
        ghost_entity: Entity,
        cursor_pos: Vec2,
        drop_target: Option<DropTarget>,
        overlay_entity: Option<Entity>,
    },
}

#[derive(Clone, Debug)]
pub enum DropTarget {
    TabBar(Entity),
    AreaEdge { area: Entity, edge: DropEdge },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropEdge {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Component)]
pub struct DragGhost;

#[derive(Component)]
pub struct DropOverlay;

pub struct DockDragPlugin;

impl Plugin for DockDragPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DockDragState>()
            .add_observer(on_tab_drag_start)
            .add_observer(on_sidebar_icon_drag_start)
            .add_observer(on_grip_drag_start)
            .add_observer(on_drag_move)
            .add_observer(on_drag_end)
            .add_systems(Update, cancel_drag_on_escape);
    }
}

fn on_tab_drag_start(
    trigger: On<Pointer<DragStart>>,
    tabs: Query<&DockTab>,
    mut drag_state: ResMut<DockDragState>,
    registry: Res<crate::WindowRegistry>,
) {
    let entity = trigger.event_target();
    let Ok(tab) = tabs.get(entity) else { return };

    let display_name = registry
        .get(&tab.window_id)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| tab.window_id.clone());

    *drag_state = DockDragState::PendingDrag {
        source_tab: entity,
        window_id: tab.window_id.clone(),
        window_name: display_name,
        start_pos: Vec2::new(
            trigger.event().pointer_location.position.x,
            trigger.event().pointer_location.position.y,
        ),
    };
}

fn on_sidebar_icon_drag_start(
    trigger: On<Pointer<DragStart>>,
    icons: Query<&DockSidebarIcon>,
    mut drag_state: ResMut<DockDragState>,
    registry: Res<crate::WindowRegistry>,
) {
    let entity = trigger.event_target();
    let Ok(icon) = icons.get(entity) else { return };

    let display_name = registry
        .get(&icon.window_id)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| icon.window_id.clone());

    *drag_state = DockDragState::PendingDrag {
        source_tab: entity,
        window_id: icon.window_id.clone(),
        window_name: display_name,
        start_pos: Vec2::new(
            trigger.event().pointer_location.position.x,
            trigger.event().pointer_location.position.y,
        ),
    };
}

fn on_grip_drag_start(
    trigger: On<Pointer<DragStart>>,
    grips: Query<(), With<DockTabGrip>>,
    dock_areas: Query<&crate::ActiveDockWindow, With<DockArea>>,
    parent_query: Query<&ChildOf>,
    mut drag_state: ResMut<DockDragState>,
    registry: Res<crate::WindowRegistry>,
) {
    let entity = trigger.event_target();
    if grips.get(entity).is_err() {
        return;
    }

    let mut current = entity;
    let mut active_window_id = None;
    loop {
        if let Ok(active) = dock_areas.get(current) {
            active_window_id = active.0.clone();
            break;
        }
        let Ok(parent) = parent_query.get(current) else {
            break;
        };
        current = parent.parent();
    }

    let Some(window_id) = active_window_id else {
        return;
    };

    let window_name = registry
        .get(&window_id)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| window_id.clone());

    *drag_state = DockDragState::PendingDrag {
        source_tab: entity,
        window_id,
        window_name,
        start_pos: Vec2::new(
            trigger.event().pointer_location.position.x,
            trigger.event().pointer_location.position.y,
        ),
    };
}

fn on_drag_move(
    mut trigger: On<Pointer<Drag>>,
    mut drag_state: ResMut<DockDragState>,
    mut commands: Commands,
    areas: Query<(Entity, &ComputedNode, &UiGlobalTransform), With<DockArea>>,
    parent_query: Query<&ChildOf>,
) {
    let drag_event = trigger.event();
    let cursor = Vec2::new(
        drag_event.pointer_location.position.x,
        drag_event.pointer_location.position.y,
    );

    match &*drag_state {
        DockDragState::PendingDrag {
            source_tab,
            window_id,
            window_name,
            start_pos,
        } => {
            if cursor.distance(*start_pos) < DRAG_THRESHOLD {
                return;
            }

            let source_tab = *source_tab;
            let window_id = window_id.clone();
            let window_name = window_name.clone();

            let source_area = find_parent_area(source_tab, &parent_query, &areas);

            let ghost = commands
                .spawn((
                    DragGhost,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(cursor.x - 40.0),
                        top: Val::Px(cursor.y - 12.0),
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        border_radius: BorderRadius::all(Val::Px(4.0)),
                        ..default()
                    },
                    BackgroundColor(tokens::MENU_BG),
                    BorderColor::all(tokens::ACCENT_BLUE),
                    GlobalZIndex(200),
                    children![(
                        Text::new(window_name.clone()),
                        TextFont {
                            font_size: 11.0,
                            ..default()
                        },
                        TextColor(tokens::TEXT_PRIMARY),
                    )],
                ))
                .id();

            *drag_state = DockDragState::Dragging {
                source_tab,
                window_id,
                window_name,
                source_area: source_area.unwrap_or(Entity::PLACEHOLDER),
                ghost_entity: ghost,
                cursor_pos: cursor,
                drop_target: None,
                overlay_entity: None,
            };

            trigger.propagate(false);
        }
        DockDragState::Dragging {
            ghost_entity,
            overlay_entity,
            ..
        } => {
            let ghost = *ghost_entity;
            let old_overlay = *overlay_entity;

            commands.entity(ghost).insert(Node {
                position_type: PositionType::Absolute,
                left: Val::Px(cursor.x - 40.0),
                top: Val::Px(cursor.y - 12.0),
                padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                ..default()
            });

            if let Some(old) = old_overlay {
                commands.entity(old).despawn();
            }

            let mut new_target = None;
            let mut new_overlay = None;

            for (area_entity, computed, ui_transform) in &areas {
                if !computed.contains_point(*ui_transform, cursor) {
                    continue;
                }

                let size = computed.size() * computed.inverse_scale_factor();
                let (_scale, _angle, center) =
                    ui_transform.to_scale_angle_translation();
                let top_left = center - size / 2.0;

                let rel = cursor - top_left;
                let frac_x = rel.x / size.x;
                let frac_y = rel.y / size.y;

                let edge = if frac_y < 0.25 {
                    Some(DropEdge::Top)
                } else if frac_y > 0.75 {
                    Some(DropEdge::Bottom)
                } else if frac_x < 0.25 {
                    Some(DropEdge::Left)
                } else if frac_x > 0.75 {
                    Some(DropEdge::Right)
                } else {
                    None
                };

                if let Some(edge) = edge {
                    new_target = Some(DropTarget::AreaEdge {
                        area: area_entity,
                        edge,
                    });

                    let (overlay_pos, overlay_size) =
                        edge_overlay_rect(top_left, size, edge);
                    let overlay = commands
                        .spawn((
                            DropOverlay,
                            Node {
                                position_type: PositionType::Absolute,
                                left: Val::Px(overlay_pos.x),
                                top: Val::Px(overlay_pos.y),
                                width: Val::Px(overlay_size.x),
                                height: Val::Px(overlay_size.y),
                                border: UiRect::all(Val::Px(2.0)),
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(
                                Color::srgba(0.126, 0.431, 0.784, 0.25),
                            ),
                            BorderColor::all(tokens::ACCENT_BLUE),
                            GlobalZIndex(150),
                        ))
                        .id();
                    new_overlay = Some(overlay);
                } else {
                    new_target = Some(DropTarget::TabBar(area_entity));

                    let overlay = commands
                        .spawn((
                            DropOverlay,
                            Node {
                                position_type: PositionType::Absolute,
                                left: Val::Px(top_left.x),
                                top: Val::Px(top_left.y),
                                width: Val::Px(size.x),
                                height: Val::Px(size.y),
                                border: UiRect::all(Val::Px(2.0)),
                                border_radius: BorderRadius::all(Val::Px(4.0)),
                                ..default()
                            },
                            BackgroundColor(
                                Color::srgba(0.126, 0.431, 0.784, 0.12),
                            ),
                            BorderColor::all(tokens::ACCENT_BLUE),
                            GlobalZIndex(150),
                        ))
                        .id();
                    new_overlay = Some(overlay);
                }

                break;
            }

            if let DockDragState::Dragging {
                drop_target,
                overlay_entity,
                cursor_pos,
                ..
            } = &mut *drag_state
            {
                *drop_target = new_target;
                *overlay_entity = new_overlay;
                *cursor_pos = cursor;
            }

            trigger.propagate(false);
        }
        _ => {}
    }
}

fn on_drag_end(
    _trigger: On<Pointer<DragEnd>>,
    mut drag_state: ResMut<DockDragState>,
    mut commands: Commands,
) {
    let state = std::mem::take(&mut *drag_state);
    match state {
        DockDragState::Dragging {
            ghost_entity,
            overlay_entity,
            drop_target,
            window_id,
            source_area,
            ..
        } => {
            commands.entity(ghost_entity).despawn();
            if let Some(overlay) = overlay_entity {
                commands.entity(overlay).despawn();
            }

            if let Some(target) = drop_target {
                match target {
                    DropTarget::TabBar(target_area) => {
                        if target_area != source_area {
                            info!(
                                "Dock drag: move '{}' to area {:?} as tab",
                                window_id, target_area
                            );
                            let wid = window_id.clone();
                            commands.queue(move |world: &mut World| {
                                move_window_to_area(world, &wid, source_area, target_area);
                            });
                        }
                    }
                    DropTarget::AreaEdge { area, edge } => {
                        info!(
                            "Dock drag: split area {:?} at {:?} with '{}'",
                            area, edge, window_id
                        );
                        let wid = window_id.clone();
                        let src = source_area;
                        commands.queue(move |world: &mut World| {
                            split_area_with_window(world, area, edge, &wid, src);
                        });
                    }
                }
            }
        }
        DockDragState::PendingDrag { .. } => {}
        DockDragState::Idle => {}
    }

    *drag_state = DockDragState::Idle;
}

fn cancel_drag_on_escape(
    keys: Res<ButtonInput<KeyCode>>,
    mut drag_state: ResMut<DockDragState>,
    mut commands: Commands,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }

    let state = std::mem::take(&mut *drag_state);
    if let DockDragState::Dragging {
        ghost_entity,
        overlay_entity,
        ..
    } = state
    {
        commands.entity(ghost_entity).despawn();
        if let Some(overlay) = overlay_entity {
            commands.entity(overlay).despawn();
        }
    }

    *drag_state = DockDragState::Idle;
}

fn move_window_to_area(world: &mut World, window_id: &str, source_area: Entity, target_area: Entity) {
    // 1. Find and remove the tab from the source area's tab row
    let source_tab_entity = {
        let mut found = None;
        let mut query = world.query::<(Entity, &DockTab, &ChildOf)>();
        for (entity, tab, _) in query.iter(world) {
            if tab.window_id == window_id {
                // Check this tab is in the source area's hierarchy
                let mut current = entity;
                loop {
                    if current == source_area {
                        found = Some(entity);
                        break;
                    }
                    let Some(parent) = world.entity(current).get::<ChildOf>() else {
                        break;
                    };
                    current = parent.parent();
                }
                if found.is_some() {
                    break;
                }
            }
        }
        found
    };

    if let Some(tab_entity) = source_tab_entity {
        world.entity_mut(tab_entity).despawn();
    }

    // 2. Find and reparent the content entity from source to target area
    let content_entity = {
        let mut found = None;
        let mut query = world.query::<(Entity, &crate::DockTabContent, &ChildOf)>();
        for (entity, content, child_of) in query.iter(world) {
            if content.window_id == window_id && child_of.parent() == source_area {
                found = Some(entity);
                break;
            }
        }
        found
    };

    if let Some(content_entity) = content_entity {
        world
            .entity_mut(content_entity)
            .insert(ChildOf(target_area));
        world.entity_mut(content_entity).insert(Node {
            flex_grow: 1.0,
            width: Val::Percent(100.0),
            min_height: Val::Px(0.0),
            flex_direction: FlexDirection::Column,
            overflow: Overflow::clip(),
            display: Display::None,
            ..default()
        });
    }

    // 3. Add a new tab in the target area's tab row (may be nested: area → tab_bar → tab_row)
    let target_tab_row = find_tab_row_recursive(world, target_area);

    let window_name = world
        .get_resource::<crate::WindowRegistry>()
        .and_then(|r| r.get(window_id))
        .map(|d| d.name.clone())
        .unwrap_or_else(|| window_id.to_string());

    if let Some(tab_row) = target_tab_row {
        crate::tabs::spawn_tab_in_world(world, tab_row, window_id, &window_name, false);
    }

    // 4. If the source area now has no active window, activate the first remaining one
    let source_remaining: Vec<String> = {
        let mut query = world.query::<(&crate::DockTabContent, &ChildOf)>();
        query
            .iter(world)
            .filter(|(_, co)| co.parent() == source_area)
            .map(|(c, _)| c.window_id.clone())
            .collect()
    };

    if let Some(first) = source_remaining.first() {
        let first_id = first.clone();
        if let Some(mut active) = world.entity_mut(source_area).get_mut::<crate::ActiveDockWindow>() {
            if active.0.as_deref() == Some(window_id) {
                active.0 = Some(first_id.clone());
            }
        }
        // Show the first remaining content
        let mut query = world.query::<(Entity, &crate::DockTabContent, &ChildOf)>();
        let to_show: Vec<Entity> = query
            .iter(world)
            .filter(|(_, c, co)| co.parent() == source_area && c.window_id == first_id)
            .map(|(e, _, _)| e)
            .collect();
        for entity in to_show {
            if let Some(mut node) = world.entity_mut(entity).get_mut::<Node>() {
                node.display = Display::Flex;
            }
        }
    }

    info!("Moved window '{}' from {:?} to {:?}", window_id, source_area, target_area);
}

fn split_area_with_window(
    world: &mut World,
    target_area: Entity,
    edge: DropEdge,
    window_id: &str,
    source_area: Entity,
) {
    use crate::split::{Panel, PanelGroup, PanelHandle};

    // 1. Remove the window from its source area (tab + content)
    let source_tab = {
        let mut query = world.query::<(Entity, &DockTab, &ChildOf)>();
        let mut found = None;
        for (entity, tab, _) in query.iter(world) {
            if tab.window_id == window_id {
                let mut current = entity;
                loop {
                    if current == source_area {
                        found = Some(entity);
                        break;
                    }
                    let Some(parent) = world.entity(current).get::<ChildOf>() else {
                        break;
                    };
                    current = parent.parent();
                }
                if found.is_some() {
                    break;
                }
            }
        }
        found
    };
    if let Some(tab) = source_tab {
        world.entity_mut(tab).despawn();
    }

    let content_entity = {
        let mut query = world.query::<(Entity, &crate::DockTabContent, &ChildOf)>();
        query
            .iter(world)
            .find(|(_, c, co)| c.window_id == window_id && co.parent() == source_area)
            .map(|(e, _, _)| e)
    };
    let content_entity = match content_entity {
        Some(e) => e,
        None => return,
    };

    // Activate next tab in source if needed
    {
        let remaining: Vec<String> = {
            let mut query = world.query::<(&crate::DockTabContent, &ChildOf)>();
            query
                .iter(world)
                .filter(|(_, co)| co.parent() == source_area)
                .filter(|(c, _)| c.window_id != window_id)
                .map(|(c, _)| c.window_id.clone())
                .collect()
        };
        if let Some(first) = remaining.first() {
            let first_id = first.clone();
            if let Some(mut active) = world.entity_mut(source_area).get_mut::<crate::ActiveDockWindow>() {
                if active.0.as_deref() == Some(window_id) {
                    active.0 = Some(first_id.clone());
                }
            }
            let mut query = world.query::<(Entity, &crate::DockTabContent, &ChildOf)>();
            let to_show: Vec<Entity> = query
                .iter(world)
                .filter(|(_, c, co)| co.parent() == source_area && c.window_id == first_id)
                .map(|(e, _, _)| e)
                .collect();
            for entity in to_show {
                if let Some(mut node) = world.entity_mut(entity).get_mut::<Node>() {
                    node.display = Display::Flex;
                }
            }
        }
    }

    // 2. Detach content from source
    world.entity_mut(content_entity).remove::<ChildOf>();

    // 3. Capture target's original parent, index in parent's children, and Panel ratio.
    let target_parent = world
        .entity(target_area)
        .get::<ChildOf>()
        .map(|co| co.parent());
    let target_index = target_parent.and_then(|parent| {
        world
            .entity(parent)
            .get::<Children>()
            .and_then(|ch| ch.iter().position(|e| e == target_area))
    });
    let ratio = world
        .entity(target_area)
        .get::<Panel>()
        .map(|p| p.ratio)
        .unwrap_or(1.0);

    // Detach target from its parent and clear its outer Panel ratio.
    // target_area will become a child of the wrapper with Panel { ratio: 1.0 }.
    world.entity_mut(target_area).remove::<Panel>();
    world.entity_mut(target_area).remove::<ChildOf>();

    // 4. Determine split direction.
    let flex_direction = match edge {
        DropEdge::Top | DropEdge::Bottom => FlexDirection::Column,
        DropEdge::Left | DropEdge::Right => FlexDirection::Row,
    };

    // 5. Spawn wrapper PanelGroup with the ratio target_area originally held.
    let wrapper = world
        .spawn((
            PanelGroup { min_ratio: 0.15 },
            Panel { ratio },
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction,
                ..default()
            },
        ))
        .id();

    // 6. Insert wrapper at target's original slot in the parent's children list.
    if let (Some(parent), Some(index)) = (target_parent, target_index) {
        world
            .entity_mut(parent)
            .insert_children(index, &[wrapper]);
    }

    // 7. Create the new DockArea for the dropped window.
    let window_name = world
        .get_resource::<crate::WindowRegistry>()
        .and_then(|r| r.get(window_id))
        .map(|d| d.name.clone())
        .unwrap_or_else(|| window_id.to_string());

    let new_area = world
        .spawn((
            DockArea {
                id: format!("split_{}", window_id),
                style: crate::DockAreaStyle::TabBar,
            },
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                overflow: Overflow::clip(),
                border_radius: BorderRadius::all(Val::Px(5.0)),
                ..default()
            },
            BackgroundColor(tokens::PANEL_BG),
        ))
        .id();

    // 8. Spawn a tab bar for the new area with just the dropped window as its only tab.
    //    populate_dock_areas normally handles this at startup by reading the
    //    WindowRegistry, but runtime-split areas have a synthetic id not in the
    //    registry, so we spawn the tab bar inline here.
    crate::tabs::spawn_tab_bar_world(
        world,
        new_area,
        &[(window_id.to_string(), window_name.clone())],
    );
    world
        .entity_mut(new_area)
        .insert(crate::ActiveDockWindow(Some(window_id.to_string())));

    // 9. Reparent the moved content to the new area.
    world.entity_mut(content_entity).insert(ChildOf(new_area));
    world.entity_mut(content_entity).insert(Node {
        flex_grow: 1.0,
        width: Val::Percent(100.0),
        min_height: Val::Px(0.0),
        flex_direction: FlexDirection::Column,
        overflow: Overflow::clip(),
        display: Display::Flex,
        ..default()
    });

    // 10. Create panel handle.
    let handle = world
        .spawn((
            PanelHandle,
            Node {
                min_width: Val::Px(3.0),
                min_height: Val::Px(3.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
        ))
        .id();

    // 11. Existing area keeps the bulk of the space; the dropped window gets ~26%.
    //     This matches VS Code / Dockview conventions — dropping a panel shouldn't
    //     cut the existing content in half.
    world.entity_mut(target_area).insert(Panel { ratio: 1.0 });
    world.entity_mut(new_area).insert(Panel { ratio: 0.35 });

    // 12. Add all three children to the wrapper in one ordered call.
    //     For Top/Left, the new area goes first; for Bottom/Right, it goes last.
    match edge {
        DropEdge::Top | DropEdge::Left => {
            world
                .entity_mut(wrapper)
                .add_children(&[new_area, handle, target_area]);
        }
        DropEdge::Bottom | DropEdge::Right => {
            world
                .entity_mut(wrapper)
                .add_children(&[target_area, handle, new_area]);
        }
    }

    info!(
        "Split complete: created new area for '{}' at {:?} of {:?}",
        window_name, edge, target_area
    );
}

fn find_tab_row_recursive(world: &World, root: Entity) -> Option<Entity> {
    if world.entity(root).contains::<crate::tabs::DockTabRow>() {
        return Some(root);
    }
    let children: Vec<Entity> = world
        .entity(root)
        .get::<Children>()
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    for child in children {
        if let Some(found) = find_tab_row_recursive(world, child) {
            return Some(found);
        }
    }
    None
}

fn find_parent_area(
    entity: Entity,
    parents: &Query<&ChildOf>,
    areas: &Query<(Entity, &ComputedNode, &UiGlobalTransform), With<DockArea>>,
) -> Option<Entity> {
    let mut current = entity;
    loop {
        if areas.contains(current) {
            return Some(current);
        }
        let Ok(parent) = parents.get(current) else {
            return None;
        };
        current = parent.parent();
    }
}

fn edge_overlay_rect(top_left: Vec2, size: Vec2, edge: DropEdge) -> (Vec2, Vec2) {
    match edge {
        DropEdge::Top => (top_left, Vec2::new(size.x, size.y * 0.5)),
        DropEdge::Bottom => (
            Vec2::new(top_left.x, top_left.y + size.y * 0.5),
            Vec2::new(size.x, size.y * 0.5),
        ),
        DropEdge::Left => (top_left, Vec2::new(size.x * 0.5, size.y)),
        DropEdge::Right => (
            Vec2::new(top_left.x + size.x * 0.5, top_left.y),
            Vec2::new(size.x * 0.5, size.y),
        ),
    }
}
