use bevy::{
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

use super::{BrushFaceEntity, BrushMaterialPalette, BrushMeshCache, BrushPreview};
use crate::NonSerializable;
use crate::draw_brush::DrawBrushState;
use jackdaw_geometry::{
    compute_brush_geometry, compute_face_tangent_axes, compute_face_uvs, triangulate_face,
};

pub(super) fn setup_default_materials(
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut palette: ResMut<BrushMaterialPalette>,
) {
    let defaults = [
        Color::srgb(0.7, 0.7, 0.7), // default grey (matches Cube mesh)
        Color::srgb(0.5, 0.5, 0.5), // gray
        Color::srgb(0.3, 0.3, 0.3), // dark gray
        Color::srgb(0.7, 0.3, 0.2), // brick red
        Color::srgb(0.3, 0.5, 0.7), // steel blue
        Color::srgb(0.4, 0.6, 0.3), // mossy green
        Color::srgb(0.6, 0.5, 0.3), // sandy tan
        Color::srgb(0.5, 0.3, 0.5), // purple
    ];
    for color in defaults {
        palette.materials.push(materials.add(StandardMaterial {
            base_color: color.with_alpha(1.0),
            ..default()
        }));
        palette
            .preview_materials
            .push(materials.add(StandardMaterial {
                base_color: color.with_alpha(0.75),
                alpha_mode: AlphaMode::Blend,
                ..default()
            }));
    }
}

pub fn regenerate_brush_meshes(
    mut commands: Commands,
    changed_brushes: Query<
        (
            Entity,
            &super::Brush,
            Option<&Children>,
            Option<&super::BrushPreview>,
        ),
        Changed<super::Brush>,
    >,
    mesh3d_query: Query<(), With<Mesh3d>>,
    mut meshes: ResMut<Assets<Mesh>>,
    palette: Res<BrushMaterialPalette>,
) {
    for (entity, brush, children, preview) in &changed_brushes {
        // Despawn all Mesh3d children — covers both BrushFaceEntity children
        // from previous regen cycles and the runtime mesh child from JsnPlugin.
        if let Some(children) = children {
            for child in children.iter() {
                if mesh3d_query.get(child).is_ok() {
                    if let Ok(mut ec) = commands.get_entity(child) {
                        ec.despawn();
                    }
                }
            }
        }

        let (vertices, face_polygons) = compute_brush_geometry(&brush.faces);

        let mut face_entities = Vec::with_capacity(brush.faces.len());

        for (face_idx, face_data) in brush.faces.iter().enumerate() {
            let indices = &face_polygons[face_idx];
            if indices.len() < 3 {
                face_entities.push(Entity::PLACEHOLDER);
                continue;
            }

            // Build per-face mesh with local vertex positions
            let positions: Vec<[f32; 3]> =
                indices.iter().map(|&vi| vertices[vi].to_array()).collect();
            let normals: Vec<[f32; 3]> = vec![face_data.plane.normal.to_array(); indices.len()];
            let (u_axis, v_axis) = if face_data.uv_u_axis != Vec3::ZERO
                && face_data.uv_v_axis != Vec3::ZERO
            {
                (face_data.uv_u_axis, face_data.uv_v_axis)
            } else {
                compute_face_tangent_axes(face_data.plane.normal)
            };
            let uvs = compute_face_uvs(
                &vertices,
                indices,
                u_axis,
                v_axis,
                face_data.uv_offset,
                face_data.uv_scale,
                face_data.uv_rotation,
            );
            let w = face_data.plane.normal.dot(u_axis.cross(v_axis)).signum();
            let tangent = [u_axis.x, u_axis.y, u_axis.z, w];
            let tangents: Vec<[f32; 4]> = vec![tangent; indices.len()];

            // Fan triangulate — local indices (0..positions.len())
            let local_tris = triangulate_face(&(0..indices.len()).collect::<Vec<_>>());
            let flat_indices: Vec<u32> =
                local_tris.iter().flat_map(|t| t.iter().copied()).collect();

            let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, default());
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
            mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
            mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents);
            mesh.insert_indices(Indices::U32(flat_indices));

            let mesh_handle = meshes.add(mesh);

            // Use the face's material handle if set, otherwise fall back to palette
            let material = if face_data.material != Handle::default() {
                face_data.material.clone()
            } else {
                let mats = if preview.is_some() {
                    &palette.preview_materials
                } else {
                    &palette.materials
                };
                mats.first().cloned().unwrap_or_default()
            };

            let face_entity = commands
                .spawn((
                    BrushFaceEntity {
                        brush_entity: entity,
                        face_index: face_idx,
                    },
                    Mesh3d(mesh_handle),
                    MeshMaterial3d(material),
                    Transform::default(),
                    ChildOf(entity),
                    NonSerializable,
                ))
                .id();

            face_entities.push(face_entity);
        }

        commands.entity(entity).insert(BrushMeshCache {
            vertices,
            face_polygons,
            face_entities,
        });
    }
}

/// Reads interaction state each frame and inserts/removes `BrushPreview` on the
/// appropriate brush entity so downstream systems can swap materials.
pub(super) fn sync_brush_preview(
    mut commands: Commands,
    face_drag: Res<super::BrushDragState>,
    vertex_drag: Res<super::VertexDragState>,
    edge_drag: Res<super::EdgeDragState>,
    draw_state: Res<DrawBrushState>,
    selection: Res<super::BrushSelection>,
    existing: Query<Entity, With<BrushPreview>>,
) {
    let preview_entity = if face_drag.active || vertex_drag.active || edge_drag.active {
        selection.entity
    } else if let Some(ref active) = draw_state.active {
        active.append_target
    } else {
        None
    };

    for entity in &existing {
        if Some(entity) != preview_entity {
            commands.entity(entity).remove::<BrushPreview>();
        }
    }

    if let Some(entity) = preview_entity {
        if existing.get(entity).is_err() {
            commands.entity(entity).insert(BrushPreview);
        }
    }
}

/// When `BrushPreview` is added or removed, swap materials on existing face entities
/// without requiring a full mesh regeneration.
pub(super) fn apply_brush_preview_materials(
    mut commands: Commands,
    palette: Res<BrushMaterialPalette>,
    added: Query<(Entity, &BrushMeshCache), Added<BrushPreview>>,
    mut removed: RemovedComponents<BrushPreview>,
    brush_query: Query<&BrushMeshCache>,
    face_query: Query<&BrushFaceEntity>,
    brush_data: Query<&super::Brush>,
) {
    for (entity, cache) in &added {
        swap_face_materials(
            &mut commands,
            entity,
            cache,
            &palette.preview_materials,
            &face_query,
            &brush_data,
        );
    }

    for entity in removed.read() {
        if let Ok(cache) = brush_query.get(entity) {
            swap_face_materials(
                &mut commands,
                entity,
                cache,
                &palette.materials,
                &face_query,
                &brush_data,
            );
        }
    }
}

fn swap_face_materials(
    commands: &mut Commands,
    brush_entity: Entity,
    cache: &BrushMeshCache,
    target_materials: &[Handle<StandardMaterial>],
    face_query: &Query<&BrushFaceEntity>,
    brush_data: &Query<&super::Brush>,
) {
    let Ok(brush) = brush_data.get(brush_entity) else {
        return;
    };

    for &face_entity in &cache.face_entities {
        if face_entity == Entity::PLACEHOLDER {
            continue;
        }
        let Ok(face) = face_query.get(face_entity) else {
            continue;
        };
        let Some(face_data) = brush.faces.get(face.face_index) else {
            continue;
        };
        // Only swap faces that use the default palette (no explicit material)
        if face_data.material != Handle::default() {
            continue;
        }
        let mat = target_materials.first().cloned().unwrap_or_default();
        commands.entity(face_entity).insert(MeshMaterial3d(mat));
    }
}
