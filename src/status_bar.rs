use bevy::prelude::*;
use jackdaw_feathers::status_bar::{StatusBarCenter, StatusBarLeft, StatusBarRight};

use crate::{
    EditorEntity,
    brush::{BrushEditMode, EditMode},
    draw_brush::DrawBrushState,
    gizmos::{GizmoMode, GizmoSpace},
    modal_transform::{ModalOp, ModalTransformState},
    scene_io::{SceneDirtyState, SceneFilePath},
};

/// Git branch + short commit hash, read once at startup.
#[derive(Resource, Default)]
pub struct GitInfo {
    pub display: String,
}

pub struct StatusBarPlugin;

impl Plugin for StatusBarPlugin {
    fn build(&self, app: &mut App) {
        // Read git info once at startup
        let git_display = read_git_info();
        app.insert_resource(GitInfo {
            display: git_display,
        });
        app.add_systems(
            Update,
            (
                update_status_left,
                update_status_center,
                update_status_right,
                update_scene_stats,
            )
                .run_if(in_state(crate::AppState::Editor)),
        );
    }
}

fn read_git_info() -> String {
    let branch = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let hash = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if branch.is_empty() {
        String::new()
    } else {
        format!("{branch} ({hash})")
    }
}

fn update_status_left(
    git_info: Res<GitInfo>,
    mut text_query: Query<&mut Text, With<StatusBarLeft>>,
) {
    // Git info is static, only set once
    let Ok(mut text) = text_query.single_mut() else {
        return;
    };
    if text.0.is_empty() && !git_info.display.is_empty() {
        text.0 = git_info.display.clone();
    }
}

fn update_status_center(
    scene_path: Res<SceneFilePath>,
    scene_dirty: Res<SceneDirtyState>,
    history: Res<jackdaw_commands::CommandHistory>,
    mut text_query: Query<&mut Text, With<StatusBarCenter>>,
) {
    if !scene_path.is_changed() && !scene_dirty.is_changed() && !history.is_changed() {
        return;
    }
    let Ok(mut text) = text_query.single_mut() else {
        return;
    };

    let version = env!("CARGO_PKG_VERSION");
    let dirty = history.undo_stack.len() != scene_dirty.undo_len_at_save;
    let dirty_marker = if dirty { "*" } else { "" };
    let path_str = scene_path
        .path
        .as_deref()
        .map(|p| format!(" | {dirty_marker}{p}"))
        .unwrap_or_else(|| {
            if dirty {
                " | *Unsaved".to_string()
            } else {
                String::new()
            }
        });

    text.0 = format!("Jackdaw v{version}{path_str}");
}

/// Marker for the scene stats text in the hierarchy panel footer.
#[derive(Component)]
pub struct SceneStatsText;

fn update_status_right(
    mode: Res<GizmoMode>,
    space: Res<GizmoSpace>,
    modal: Res<ModalTransformState>,
    edit_mode: Res<EditMode>,
    draw_state: Res<DrawBrushState>,
    mut text_query: Query<&mut Text, With<StatusBarRight>>,
) {
    if !mode.is_changed()
        && !space.is_changed()
        && !modal.is_changed()
        && !edit_mode.is_changed()
        && !draw_state.is_changed()
    {
        return;
    }
    let Ok(mut text) = text_query.single_mut() else {
        return;
    };

    // Show draw brush mode status
    if draw_state.active.is_some() {
        text.0 = "Draw Brush".to_string();
        return;
    }

    // Show brush edit mode info
    if let EditMode::BrushEdit(sub_mode) = *edit_mode {
        let sub_str = match sub_mode {
            BrushEditMode::Face => "Face",
            BrushEditMode::Vertex => "Vertex",
            BrushEditMode::Edge => "Edge",
            BrushEditMode::Clip => "Clip",
        };
        text.0 = format!("Edit: {sub_str}");
        return;
    }

    // Show modal operation info when active
    if let Some(ref active) = modal.active {
        let op_str = match active.op {
            ModalOp::Grab => "Grab",
            ModalOp::Rotate => "Rotate",
            ModalOp::Scale => "Scale",
        };
        text.0 = format!("{op_str} | LMB confirm, RMB cancel");
        return;
    }

    let mode_str = match *mode {
        GizmoMode::Translate => "Translate",
        GizmoMode::Rotate => "Rotate",
        GizmoMode::Scale => "Scale",
    };
    let space_str = match *space {
        GizmoSpace::World => "World",
        GizmoSpace::Local => "Local",
    };

    text.0 = format!("{mode_str} ({space_str})");
}

/// System to update the scene stats text in the hierarchy panel footer.
pub fn update_scene_stats(
    scene_entities: Query<Entity, (With<Transform>, Without<EditorEntity>)>,
    meshes: Query<(), (With<Mesh3d>, Without<EditorEntity>)>,
    point_lights: Query<(), (With<PointLight>, Without<EditorEntity>)>,
    dir_lights: Query<(), (With<DirectionalLight>, Without<EditorEntity>)>,
    spot_lights: Query<(), (With<SpotLight>, Without<EditorEntity>)>,
    cameras: Query<(), (With<Camera3d>, Without<EditorEntity>)>,
    mut text_query: Query<&mut Text, With<SceneStatsText>>,
) {
    let Ok(mut text) = text_query.single_mut() else {
        return;
    };

    let total = scene_entities.iter().count();
    let mesh_count = meshes.iter().count();
    let light_count =
        point_lights.iter().count() + dir_lights.iter().count() + spot_lights.iter().count();
    let camera_count = cameras.iter().count();

    let new_text = format!(
        "{total} entities  {mesh_count} meshes  {light_count} lights  {camera_count} cameras"
    );
    if text.0 != new_text {
        text.0 = new_text;
    }
}
