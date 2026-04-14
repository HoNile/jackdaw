use bevy::prelude::*;

use crate::layout::LayoutState;

pub struct WorkspaceDescriptor {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub accent_color: Color,
    pub layout: LayoutState,
}

#[derive(Resource, Default)]
pub struct WorkspaceRegistry {
    pub workspaces: Vec<WorkspaceDescriptor>,
    pub active: Option<String>,
}

impl WorkspaceRegistry {
    pub fn register(&mut self, descriptor: WorkspaceDescriptor) {
        if self.active.is_none() {
            self.active = Some(descriptor.id.clone());
        }
        self.workspaces.push(descriptor);
    }

    pub fn get(&self, id: &str) -> Option<&WorkspaceDescriptor> {
        self.workspaces.iter().find(|w| w.id == id)
    }

    pub fn active_workspace(&self) -> Option<&WorkspaceDescriptor> {
        self.active.as_ref().and_then(|id| self.get(id))
    }

    pub fn set_active(&mut self, id: &str) {
        if self.workspaces.iter().any(|w| w.id == id) {
            self.active = Some(id.to_string());
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &WorkspaceDescriptor> {
        self.workspaces.iter()
    }
}

#[derive(Component)]
pub struct WorkspaceTabStrip;

#[derive(Component)]
pub struct WorkspaceTab {
    pub workspace_id: String,
}

#[derive(Event, Clone, Debug)]
pub struct WorkspaceChanged {
    pub old: Option<String>,
    pub new: String,
}
