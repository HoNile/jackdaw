use std::collections::HashMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::area::{ActiveDockWindow, DockArea, DockTabContent};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct LayoutState {
    pub areas: HashMap<String, AreaState>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AreaState {
    pub windows: Vec<String>,
    pub active: Option<String>,
    pub size_ratio: f32,
}

impl Default for AreaState {
    fn default() -> Self {
        Self {
            windows: Vec::new(),
            active: None,
            size_ratio: 1.0,
        }
    }
}

/// Capture the current live layout as a LayoutState for serialization.
pub fn capture_layout_state(world: &mut World) -> LayoutState {
    let mut state = LayoutState::default();

    let mut area_query = world.query::<(Entity, &DockArea, Option<&ActiveDockWindow>, Option<&crate::Panel>)>();
    let areas: Vec<(Entity, String, Option<String>, f32)> = area_query
        .iter(world)
        .map(|(e, a, active, panel)| {
            (
                e,
                a.id.clone(),
                active.and_then(|a| a.0.clone()),
                panel.map(|p| p.ratio).unwrap_or(1.0),
            )
        })
        .collect();

    let mut content_query = world.query::<(&DockTabContent, &ChildOf)>();
    let all_content: Vec<(String, Entity)> = content_query
        .iter(world)
        .map(|(c, co)| (c.window_id.clone(), co.parent()))
        .collect();

    for (area_entity, area_id, active, ratio) in areas {
        let windows: Vec<String> = all_content
            .iter()
            .filter(|(_, p)| *p == area_entity)
            .map(|(w, _)| w.clone())
            .collect();
        state.areas.insert(
            area_id,
            AreaState {
                windows,
                active,
                size_ratio: ratio,
            },
        );
    }

    state
}
