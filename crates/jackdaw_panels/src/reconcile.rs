//! Materialize a [`DockTree`] into UI entities.
//!
//! The tree is the source of truth for layout. Editor code spawns one
//! `AnchorHost` entity per named anchor (e.g. `"left_top"`), and the
//! reconciler walks the tree from each anchor and shapes the entity
//! sub-tree to match: leaves become `DockArea`s with tab bar + content,
//! splits become flex containers wrapping two child anchor-style entities
//! plus a `PanelHandle` between them.
//!
//! Drag/move/resize operations mutate the tree only; the reconciler
//! rebuilds the affected entity sub-tree on the next frame.

use bevy::prelude::*;
use jackdaw_feathers::tokens;

use crate::area::{
    ActiveDockWindow, DockArea, DockAreaStyle, DockTab, DockTabContent, DockWindow,
};
use crate::drag::DockDragState;
use crate::registry::WindowRegistry;
use crate::sidebar::{self, DockSidebarIcon};
use crate::split::{Panel, PanelGroup, PanelHandle};
use crate::tabs;
use crate::tree::{DockLeaf, DockNode, DockSplit, DockTree, NodeId, SplitAxis};

/// Marker on entities the editor created as anchor host slots.
#[derive(Component, Clone, Debug)]
pub struct AnchorHost {
    pub anchor_id: String,
    pub default_style: DockAreaStyle,
}

/// Binds an entity to a tree node. Present on both leaf-style entities
/// (DockArea) and split wrapper entities (PanelGroup).
#[derive(Component, Copy, Clone, Debug)]
pub struct NodeBinding(pub NodeId);

pub struct ReconcilePlugin;

impl Plugin for ReconcilePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DockTree>().add_systems(
            Update,
            (
                seed_anchors_from_hosts,
                reconcile_tree,
                show_empty_anchors_during_drag,
                sync_leaf_visuals,
            )
                .chain(),
        );
    }
}

/// Public entry point for the editor's `OnEnter(Editor)` chain so
/// anchors + entities exist before saved-layout application runs.
pub fn run_initial_reconcile(world: &mut World) {
    seed_anchors_from_hosts(world);
    reconcile_tree(world);
}

/// Public for editor flows that want to seed before applying defaults
/// then reconcile in a single materialization pass.
pub fn seed_anchors(world: &mut World) {
    seed_anchors_from_hosts(world);
}

/// Public for editor flows that build the final tree shape (saved or
/// defaults) up front and then materialize once.
pub fn reconcile(world: &mut World) {
    reconcile_tree(world);
}

/// Ensure each `AnchorHost` has a tree anchor. New anchors are seeded
/// with the registry's default windows for that area id. Re-running is a
/// no-op once anchors exist (e.g. when restoring saved layout first).
fn seed_anchors_from_hosts(world: &mut World) {
    let hosts: Vec<(String, DockAreaStyle)> = {
        let mut q = world.query::<&AnchorHost>();
        q.iter(world)
            .map(|h| (h.anchor_id.clone(), h.default_style.clone()))
            .collect()
    };

    for (anchor_id, default_style) in hosts {
        if world.resource::<DockTree>().anchor(&anchor_id).is_some() {
            continue;
        }
        let windows: Vec<String> = world
            .resource::<WindowRegistry>()
            .by_area(&anchor_id)
            .iter()
            .map(|d| d.id.clone())
            .collect();
        let leaf = DockLeaf::new(anchor_id.clone(), default_style).with_windows(windows);
        world
            .resource_mut::<DockTree>()
            .set_anchor_leaf(anchor_id, leaf);
    }
}

fn reconcile_tree(world: &mut World) {
    if !world.is_resource_changed::<DockTree>() {
        return;
    }

    let anchors: Vec<(String, NodeId)> = {
        let tree = world.resource::<DockTree>();
        tree.iter_anchors()
            .map(|(k, v)| (k.to_string(), v))
            .collect()
    };

    for (anchor_id, root_node) in anchors {
        let Some(host) = find_anchor_host(world, &anchor_id) else {
            continue;
        };
        reconcile_at(world, host, root_node);
    }
}

fn find_anchor_host(world: &mut World, anchor_id: &str) -> Option<Entity> {
    let mut q = world.query::<(Entity, &AnchorHost)>();
    q.iter(world)
        .find(|(_, h)| h.anchor_id == anchor_id)
        .map(|(e, _)| e)
}

fn reconcile_at(world: &mut World, entity: Entity, node_id: NodeId) {
    let node = world.resource::<DockTree>().get(node_id).cloned();
    let Some(node) = node else {
        return;
    };
    match node {
        DockNode::Leaf(leaf) => reconcile_leaf(world, entity, node_id, &leaf),
        DockNode::Split(split) => reconcile_split(world, entity, node_id, &split),
    }
}

fn reconcile_leaf(world: &mut World, entity: Entity, node_id: NodeId, leaf: &DockLeaf) {
    let current_binding = world
        .entity(entity)
        .get::<NodeBinding>()
        .map(|b| b.0);
    let was_split = world.entity(entity).contains::<PanelGroup>();
    let current_windows = collect_content_window_ids(world, entity);

    let needs_rebuild =
        was_split || current_binding != Some(node_id) || current_windows != leaf.windows;

    if needs_rebuild {
        despawn_children(world, entity);
        world.entity_mut(entity).remove::<PanelGroup>();

        let direction = match leaf.style {
            DockAreaStyle::IconSidebar => FlexDirection::Row,
            DockAreaStyle::TabBar | DockAreaStyle::Headless => FlexDirection::Column,
        };
        if let Some(mut node) = world.entity_mut(entity).get_mut::<Node>() {
            node.flex_direction = direction;
        }

        if let Some(mut area) = world.entity_mut(entity).get_mut::<DockArea>() {
            area.id = leaf.area_id.clone();
            area.style = leaf.style.clone();
        } else {
            world.entity_mut(entity).insert(DockArea {
                id: leaf.area_id.clone(),
                style: leaf.style.clone(),
            });
        }

        spawn_leaf_ui(world, entity, leaf);
    }

    world
        .entity_mut(entity)
        .insert(ActiveDockWindow(leaf.active.clone()));
    world.entity_mut(entity).insert(NodeBinding(node_id));

    // Auto-collapse: when an anchor leaf has no windows, hide the host
    // entity and its adjacent handle so siblings can take the space.
    set_host_visible(world, entity, !leaf.windows.is_empty());
}

fn reconcile_split(world: &mut World, entity: Entity, node_id: NodeId, split: &DockSplit) {
    let current_binding = world
        .entity(entity)
        .get::<NodeBinding>()
        .map(|b| b.0);

    let mut children = collect_split_children(world, entity);
    let needs_rebuild = current_binding != Some(node_id) || children.is_none();

    if needs_rebuild {
        despawn_children(world, entity);
        world.entity_mut(entity).remove::<ActiveDockWindow>();
        world.entity_mut(entity).remove::<DockArea>();

        if let Some(mut node) = world.entity_mut(entity).get_mut::<Node>() {
            node.flex_direction = match split.axis {
                SplitAxis::Horizontal => FlexDirection::Row,
                SplitAxis::Vertical => FlexDirection::Column,
            };
        }
        if !world.entity(entity).contains::<PanelGroup>() {
            world.entity_mut(entity).insert(PanelGroup { min_ratio: 0.05 });
        }

        let child_node = || Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            overflow: Overflow::clip(),
            ..default()
        };

        let child_a = world
            .spawn((
                Panel { ratio: split.fraction },
                child_node(),
                BackgroundColor(tokens::PANEL_BG),
                ChildOf(entity),
            ))
            .id();
        let handle = world
            .spawn((
                PanelHandle,
                Node {
                    min_width: Val::Px(3.0),
                    min_height: Val::Px(3.0),
                    ..default()
                },
                BackgroundColor(Color::NONE),
                NodeBinding(node_id),
                ChildOf(entity),
            ))
            .id();
        let child_b = world
            .spawn((
                Panel { ratio: 1.0 - split.fraction },
                child_node(),
                BackgroundColor(tokens::PANEL_BG),
                ChildOf(entity),
            ))
            .id();
        children = Some((child_a, handle, child_b));
    }

    let (child_a, _handle, child_b) = children.expect("children exist after rebuild");

    if let Some(mut p) = world.entity_mut(child_a).get_mut::<Panel>() {
        if (p.ratio - split.fraction).abs() > f32::EPSILON {
            p.ratio = split.fraction;
        }
    }
    if let Some(mut p) = world.entity_mut(child_b).get_mut::<Panel>() {
        let other = 1.0 - split.fraction;
        if (p.ratio - other).abs() > f32::EPSILON {
            p.ratio = other;
        }
    }

    reconcile_at(world, child_a, split.a);
    reconcile_at(world, child_b, split.b);

    world.entity_mut(entity).insert(NodeBinding(node_id));
}

fn spawn_leaf_ui(world: &mut World, entity: Entity, leaf: &DockLeaf) {
    let snapshot: Vec<(String, String, Option<String>, crate::DockWindowBuildFn)> = {
        let registry = world.resource::<WindowRegistry>();
        leaf.windows
            .iter()
            .filter_map(|wid| {
                let desc = registry.get(wid)?;
                Some((
                    desc.id.clone(),
                    desc.name.clone(),
                    desc.icon.clone(),
                    desc.build.clone(),
                ))
            })
            .collect()
    };

    match leaf.style {
        DockAreaStyle::TabBar => {
            let tabs_data: Vec<(String, String)> = snapshot
                .iter()
                .map(|(id, name, _, _)| (id.clone(), name.clone()))
                .collect();
            tabs::spawn_tab_bar_world(world, entity, &tabs_data);
        }
        DockAreaStyle::IconSidebar => {
            let items: Vec<(String, String, Option<String>)> = snapshot
                .iter()
                .map(|(id, name, icon, _)| (id.clone(), name.clone(), icon.clone()))
                .collect();
            sidebar::spawn_icon_sidebar_world(world, entity, &items);
        }
        DockAreaStyle::Headless => {}
    }

    for (window_id, _name, _icon, build) in &snapshot {
        let is_active = leaf.active.as_deref() == Some(window_id.as_str());
        let content_entity = world
            .spawn((
                DockWindow {
                    descriptor_id: window_id.clone(),
                },
                DockTabContent {
                    window_id: window_id.clone(),
                },
                Node {
                    flex_grow: 1.0,
                    width: Val::Percent(100.0),
                    min_height: Val::Px(0.0),
                    flex_direction: FlexDirection::Column,
                    overflow: Overflow::clip(),
                    display: if is_active {
                        Display::Flex
                    } else {
                        Display::None
                    },
                    ..default()
                },
                ChildOf(entity),
            ))
            .id();
        (build)(world, content_entity);
    }
}

fn collect_content_window_ids(world: &mut World, entity: Entity) -> Vec<String> {
    let children: Vec<Entity> = world
        .entity(entity)
        .get::<Children>()
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    let mut out = Vec::new();
    for child in children {
        if let Some(c) = world.entity(child).get::<DockTabContent>() {
            out.push(c.window_id.clone());
        }
    }
    out
}

/// If `entity` currently looks like a split host (PanelGroup with three
/// children: panel, handle, panel), return them in order.
fn collect_split_children(
    world: &mut World,
    entity: Entity,
) -> Option<(Entity, Entity, Entity)> {
    let children: Vec<Entity> = world
        .entity(entity)
        .get::<Children>()
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    if children.len() != 3 {
        return None;
    }
    let a = children[0];
    let h = children[1];
    let b = children[2];
    if !world.entity(h).contains::<PanelHandle>() {
        return None;
    }
    if !world.entity(a).contains::<Panel>() || !world.entity(b).contains::<Panel>() {
        return None;
    }
    Some((a, h, b))
}

/// While a drag is in progress, force every empty anchor host to be
/// visible so the user can drop a window back into it. On the transition
/// out of dragging, re-hide them (the reconciler also handles this on
/// successful drops, but cancels and no-op drops don't trigger the tree).
fn show_empty_anchors_during_drag(
    drag_state: Res<DockDragState>,
    mut prev_dragging: Local<bool>,
    hosts: Query<(Entity, &NodeBinding, &ChildOf), With<AnchorHost>>,
    children_query: Query<&Children>,
    handle_marker: Query<(), With<PanelHandle>>,
    mut nodes: Query<&mut Node>,
    mut panels: Query<&mut Panel>,
    tree: Res<DockTree>,
) {
    let now_dragging = matches!(*drag_state, DockDragState::Dragging { .. });
    let state_changed = now_dragging != *prev_dragging;
    *prev_dragging = now_dragging;

    if !state_changed && !now_dragging {
        return;
    }

    let target = if now_dragging {
        Display::Flex
    } else {
        Display::None
    };

    for (host, binding, child_of) in &hosts {
        let Some(leaf) = tree.get(binding.0).and_then(|n| n.as_leaf()) else {
            continue;
        };
        if !leaf.windows.is_empty() {
            continue;
        }

        let parent = child_of.parent();
        let adjacent_handle = children_query.get(parent).ok().and_then(|siblings| {
            let idx = siblings.iter().position(|e| e == host)?;
            [idx.checked_sub(1), Some(idx + 1)]
                .into_iter()
                .flatten()
                .filter_map(|i| siblings.get(i).copied())
                .find(|&e| handle_marker.contains(e))
        });

        let mut display_changed = false;
        for e in std::iter::once(host).chain(adjacent_handle) {
            if let Ok(mut node) = nodes.get_mut(e) {
                if node.display != target {
                    node.display = target;
                    display_changed = true;
                }
            }
        }
        if display_changed {
            if let Ok(mut p) = panels.get_mut(host) {
                p.set_changed();
            }
        }
    }
}

/// Show or hide a host entity and its adjacent `PanelHandle` sibling so
/// an empty anchor doesn't leave a stub panel + dangling resize handle.
fn set_host_visible(world: &mut World, entity: Entity, visible: bool) {
    let target = if visible { Display::Flex } else { Display::None };

    let parent = world.entity(entity).get::<ChildOf>().map(|co| co.parent());
    let mut to_toggle: Vec<Entity> = vec![entity];
    if let Some(parent) = parent {
        let siblings: Vec<Entity> = world
            .entity(parent)
            .get::<Children>()
            .map(|c| c.iter().collect())
            .unwrap_or_default();
        if let Some(idx) = siblings.iter().position(|&e| e == entity) {
            for neighbor in [idx.checked_sub(1), Some(idx + 1)]
                .into_iter()
                .flatten()
                .filter_map(|i| siblings.get(i).copied())
            {
                if world.entity(neighbor).contains::<PanelHandle>() {
                    to_toggle.push(neighbor);
                    break;
                }
            }
        }
    }
    let mut display_changed = false;
    for e in to_toggle {
        if let Some(mut node) = world.entity_mut(e).get_mut::<Node>() {
            if node.display != target {
                node.display = target;
                display_changed = true;
            }
        }
    }

    // `recalculate_changed_panels` only watches `Changed<Panel>`, so a
    // `Display` toggle alone won't re-run the percentage math. Bump the
    // host's `Panel` change tick to force a recompute next frame.
    if display_changed {
        if let Some(mut panel) = world.entity_mut(entity).get_mut::<Panel>() {
            panel.set_changed();
        }
    }
}

fn despawn_children(world: &mut World, entity: Entity) {
    let children: Vec<Entity> = world
        .entity(entity)
        .get::<Children>()
        .map(|c| c.iter().collect())
        .unwrap_or_default();
    for child in children {
        if let Ok(em) = world.get_entity_mut(child) {
            em.despawn();
        }
    }
}

/// On every Update, sync visual state (active tab bg/border/text colors,
/// sidebar icon highlight, content `Display`) for leaf entities.
fn sync_leaf_visuals(
    leaves: Query<(Entity, &NodeBinding, &DockArea), Without<PanelGroup>>,
    tree: Res<DockTree>,
    tabs: Query<(Entity, &DockTab, &ChildOf)>,
    icons: Query<(Entity, &DockSidebarIcon, &ChildOf)>,
    contents: Query<(Entity, &DockTabContent, &ChildOf)>,
    parent_query: Query<&ChildOf>,
    children_query: Query<&Children>,
    mut nodes: Query<&mut Node>,
    mut bgs: Query<&mut BackgroundColor>,
    mut borders: Query<&mut BorderColor>,
    mut text_colors: Query<&mut TextColor>,
) {
    if !tree.is_changed() {
        return;
    }

    use std::collections::HashMap;
    let mut tab_to_area: HashMap<Entity, Entity> = HashMap::new();
    for (tab_entity, _, child_of) in &tabs {
        let tab_row = child_of.parent();
        let Ok(row_parent) = parent_query.get(tab_row) else {
            continue;
        };
        let tab_bar = row_parent.parent();
        let Ok(bar_parent) = parent_query.get(tab_bar) else {
            continue;
        };
        tab_to_area.insert(tab_entity, bar_parent.parent());
    }

    let mut icon_to_area: HashMap<Entity, Entity> = HashMap::new();
    for (icon_entity, _, child_of) in &icons {
        let icon_group = child_of.parent();
        let Ok(group_parent) = parent_query.get(icon_group) else {
            continue;
        };
        let sidebar_entity = group_parent.parent();
        let Ok(sidebar_parent) = parent_query.get(sidebar_entity) else {
            continue;
        };
        icon_to_area.insert(icon_entity, sidebar_parent.parent());
    }

    for (area_entity, binding, _) in &leaves {
        let Some(leaf) = tree.get(binding.0).and_then(|n| n.as_leaf()) else {
            continue;
        };

        for (tab_entity, tab, _) in &tabs {
            if tab_to_area.get(&tab_entity) != Some(&area_entity) {
                continue;
            }
            let is_active = leaf.active.as_deref() == Some(tab.window_id.as_str());
            if let Ok(mut bg) = bgs.get_mut(tab_entity) {
                bg.0 = if is_active {
                    tokens::TAB_ACTIVE_BG
                } else {
                    Color::NONE
                };
            }
            if let Ok(mut bc) = borders.get_mut(tab_entity) {
                *bc = BorderColor::all(if is_active {
                    tokens::TAB_ACTIVE_BORDER
                } else {
                    Color::NONE
                });
            }
            if let Ok(mut node) = nodes.get_mut(tab_entity) {
                node.border.top = if is_active { Val::Px(2.0) } else { Val::ZERO };
            }
            if let Ok(tab_children) = children_query.get(tab_entity) {
                for child in tab_children.iter() {
                    if let Ok(mut tc) = text_colors.get_mut(child) {
                        tc.0 = if is_active {
                            tokens::TEXT_PRIMARY
                        } else {
                            tokens::TAB_INACTIVE_TEXT
                        };
                    }
                }
            }
        }

        for (icon_entity, icon, _) in &icons {
            if icon_to_area.get(&icon_entity) != Some(&area_entity) {
                continue;
            }
            let is_active = leaf.active.as_deref() == Some(icon.window_id.as_str());
            if let Ok(mut bc) = borders.get_mut(icon_entity) {
                *bc = BorderColor::all(if is_active {
                    tokens::ACCENT_BLUE
                } else {
                    Color::NONE
                });
            }
            if let Ok(icon_children) = children_query.get(icon_entity) {
                for child in icon_children.iter() {
                    if let Ok(mut tc) = text_colors.get_mut(child) {
                        tc.0 = if is_active {
                            tokens::TEXT_PRIMARY
                        } else {
                            tokens::TAB_INACTIVE_TEXT
                        };
                    }
                }
            }
        }

        for (content_entity, content, child_of) in &contents {
            if child_of.parent() != area_entity {
                continue;
            }
            let should_show = leaf.active.as_deref() == Some(content.window_id.as_str());
            let target = if should_show {
                Display::Flex
            } else {
                Display::None
            };
            if let Ok(mut node) = nodes.get_mut(content_entity) {
                if node.display != target {
                    node.display = target;
                }
            }
        }
    }
}

// Backwards-compat alias: old code used `LeafBinding`.
pub type LeafBinding = NodeBinding;
