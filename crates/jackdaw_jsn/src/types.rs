use std::collections::{BTreeMap, HashMap};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

// Re-export geometry types so consumers see them from jackdaw_jsn
pub use jackdaw_geometry::{BrushFaceData, BrushPlane, compute_face_tangent_axes};

/// Canonical brush data. Serialized. Geometry derived from this.
#[derive(Component, Reflect, Clone, Debug, Default)]
#[reflect(Component, Default)]
pub struct Brush {
    pub faces: Vec<BrushFaceData>,
}

impl Brush {
    /// Create a cuboid brush from 6 axis-aligned face planes.
    pub fn cuboid(half_x: f32, half_y: f32, half_z: f32) -> Self {
        let normals = [
            Vec3::X,
            Vec3::NEG_X,
            Vec3::Y,
            Vec3::NEG_Y,
            Vec3::Z,
            Vec3::NEG_Z,
        ];
        let distances = [half_x, half_x, half_y, half_y, half_z, half_z];
        Self {
            faces: normals
                .iter()
                .zip(distances.iter())
                .map(|(&normal, &distance)| {
                    let (u, v) = compute_face_tangent_axes(normal);
                    BrushFaceData {
                        plane: BrushPlane { normal, distance },
                        uv_scale: Vec2::ONE,
                        uv_u_axis: u,
                        uv_v_axis: v,
                        ..default()
                    }
                })
                .collect(),
        }
    }

    /// Create a prism brush from a polygon base and extrusion depth along a normal.
    ///
    /// `vertices` are the polygon vertices in local space (must be coplanar, convex, ≥3).
    /// `normal` is the extrusion direction (unit vector, perpendicular to the polygon plane).
    /// `depth` is the total extrusion distance (can be negative; absolute value is used).
    ///
    /// The brush is centered at the origin: the polygon base sits at -|depth|/2 along the normal,
    /// and the top cap sits at +|depth|/2.
    ///
    /// Returns `None` if fewer than 3 vertices or zero depth.
    pub fn prism(vertices: &[Vec3], normal: Vec3, depth: f32) -> Option<Self> {
        if vertices.len() < 3 || depth.abs() < 1e-6 {
            return None;
        }

        let half_depth = depth.abs() / 2.0;
        let mut faces = Vec::new();

        // Top cap: faces outward along +normal
        let (top_u, top_v) = compute_face_tangent_axes(normal);
        faces.push(BrushFaceData {
            plane: BrushPlane {
                normal,
                distance: half_depth,
            },
            uv_scale: Vec2::ONE,
            uv_u_axis: top_u,
            uv_v_axis: top_v,
            ..default()
        });

        // Bottom cap: faces outward along -normal
        let (bot_u, bot_v) = compute_face_tangent_axes(-normal);
        faces.push(BrushFaceData {
            plane: BrushPlane {
                normal: -normal,
                distance: half_depth,
            },
            uv_scale: Vec2::ONE,
            uv_u_axis: bot_u,
            uv_v_axis: bot_v,
            ..default()
        });

        // Side planes: one for each edge of the polygon
        let centroid: Vec3 = vertices.iter().sum::<Vec3>() / vertices.len() as f32;
        let n = vertices.len();
        for i in 0..n {
            let a = vertices[i];
            let b = vertices[(i + 1) % n];
            let edge = b - a;
            let side_normal = edge.cross(normal).normalize_or_zero();
            if side_normal.length_squared() < 0.5 {
                continue;
            }

            // Ensure outward-facing: dot with (vertex - centroid) should be positive
            let side_normal = if side_normal.dot(a - centroid) < 0.0 {
                -side_normal
            } else {
                side_normal
            };
            let distance = side_normal.dot(a);
            let (su, sv) = compute_face_tangent_axes(side_normal);
            faces.push(BrushFaceData {
                plane: BrushPlane {
                    normal: side_normal,
                    distance,
                },
                uv_scale: Vec2::ONE,
                uv_u_axis: su,
                uv_v_axis: sv,
                ..default()
            });
        }

        if faces.len() < 4 {
            return None;
        }

        Some(Self { faces })
    }

    /// Create a sphere brush approximated as an icosahedron (20 triangular faces).
    pub fn sphere(radius: f32) -> Self {
        let phi = (1.0 + 5.0_f32.sqrt()) / 2.0;
        let raw = [
            Vec3::new(-1.0, phi, 0.0),
            Vec3::new(1.0, phi, 0.0),
            Vec3::new(-1.0, -phi, 0.0),
            Vec3::new(1.0, -phi, 0.0),
            Vec3::new(0.0, -1.0, phi),
            Vec3::new(0.0, 1.0, phi),
            Vec3::new(0.0, -1.0, -phi),
            Vec3::new(0.0, 1.0, -phi),
            Vec3::new(phi, 0.0, -1.0),
            Vec3::new(phi, 0.0, 1.0),
            Vec3::new(-phi, 0.0, -1.0),
            Vec3::new(-phi, 0.0, 1.0),
        ];
        let verts: Vec<Vec3> = raw.iter().map(|v| v.normalize() * radius).collect();

        // 20 triangular faces (standard icosahedron topology)
        let tris: [[usize; 3]; 20] = [
            [0, 11, 5],
            [0, 5, 1],
            [0, 1, 7],
            [0, 7, 10],
            [0, 10, 11],
            [1, 5, 9],
            [5, 11, 4],
            [11, 10, 2],
            [10, 7, 6],
            [7, 1, 8],
            [3, 9, 4],
            [3, 4, 2],
            [3, 2, 6],
            [3, 6, 8],
            [3, 8, 9],
            [4, 9, 5],
            [2, 4, 11],
            [6, 2, 10],
            [8, 6, 7],
            [9, 8, 1],
        ];

        let faces = tris
            .iter()
            .map(|&[a, b, c]| {
                let normal = (verts[b] - verts[a]).cross(verts[c] - verts[a]).normalize();
                let distance = normal.dot(verts[a]);
                // Ensure outward-facing
                let (normal, distance) = if distance < 0.0 {
                    (-normal, -distance)
                } else {
                    (normal, distance)
                };
                let (u, v) = compute_face_tangent_axes(normal);
                BrushFaceData {
                    plane: BrushPlane { normal, distance },
                    uv_scale: Vec2::ONE,
                    uv_u_axis: u,
                    uv_v_axis: v,
                    ..default()
                }
            })
            .collect();

        Self { faces }
    }
}

#[derive(Component, Reflect, Default, Clone, Debug)]
#[reflect(Component, Default)]
pub struct CustomProperties {
    pub properties: BTreeMap<String, PropertyValue>,
}

#[derive(Reflect, Clone, Debug, PartialEq)]
pub enum PropertyValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Vec2(Vec2),
    Vec3(Vec3),
    Color(Color),
}

impl PropertyValue {
    /// Human-readable type label for display in UI.
    pub fn type_label(&self) -> &'static str {
        match self {
            Self::Bool(_) => "Bool",
            Self::Int(_) => "Int",
            Self::Float(_) => "Float",
            Self::String(_) => "String",
            Self::Vec2(_) => "Vec2",
            Self::Vec3(_) => "Vec3",
            Self::Color(_) => "Color",
        }
    }

    /// Create a default value for a given type name.
    pub fn default_for_type(name: &str) -> Option<Self> {
        match name {
            "Bool" => Some(Self::Bool(false)),
            "Int" => Some(Self::Int(0)),
            "Float" => Some(Self::Float(0.0)),
            "String" => Some(Self::String(String::new())),
            "Vec2" => Some(Self::Vec2(Vec2::ZERO)),
            "Vec3" => Some(Self::Vec3(Vec3::ZERO)),
            "Color" => Some(Self::Color(Color::WHITE)),
            _ => None,
        }
    }

    /// All available type names for the UI picker.
    pub fn all_type_names() -> &'static [&'static str] {
        &["Bool", "Int", "Float", "String", "Vec2", "Vec3", "Color"]
    }
}

#[derive(Component, Reflect, Clone)]
#[reflect(Component)]
pub struct GltfSource {
    pub path: String,
    pub scene_index: usize,
}

/// Tracks the source `.jsn` file for a prefab instance.
#[derive(Component, Reflect, Clone, Debug, Default, Serialize, Deserialize)]
#[reflect(Component, Default)]
pub struct JsnPrefab {
    pub path: String,
}

/// Stores the original serialized component values from a prefab at instantiation time.
/// Used to detect overrides and support per-component revert.
#[derive(Component, Clone, Debug, Default)]
pub struct JsnPrefabBaseline {
    pub components: HashMap<String, serde_json::Value>,
}

#[derive(Component, Reflect, Clone, Debug)]
#[reflect(Component, Default)]
pub struct NavmeshRegion {
    pub agent_radius: f32,
    pub agent_height: f32,
    pub walkable_climb: f32,
    pub walkable_slope_degrees: f32,
    pub cell_size_fraction: f32,
    pub cell_height_fraction: f32,
    pub min_region_size: u16,
    pub merge_region_size: u16,
    pub max_simplification_error: f32,
    pub max_vertices_per_polygon: u16,
    pub edge_max_len_factor: u16,
    pub detail_sample_dist: f32,
    pub detail_sample_max_error: f32,
    pub tiling: bool,
    pub tile_size: u16,
    pub connection_url: String,
}

/// Terrain heightmap component. Stores all data needed for serialization.
#[derive(Component, Reflect, Clone, Debug)]
#[reflect(Component, Default)]
pub struct Terrain {
    /// Vertices per edge.
    pub resolution: u32,
    /// World-space XZ dimensions.
    pub size: Vec2,
    /// Maximum height value for normalization.
    pub max_height: f32,
    /// Row-major height data, length = resolution^2.
    pub heights: Vec<f32>,
}

impl Default for Terrain {
    fn default() -> Self {
        let resolution = 256;
        Self {
            resolution,
            size: Vec2::new(100.0, 100.0),
            max_height: 50.0,
            heights: vec![0.0; (resolution * resolution) as usize],
        }
    }
}

impl Default for NavmeshRegion {
    fn default() -> Self {
        Self {
            agent_radius: 0.6,
            agent_height: 2.0,
            walkable_climb: 0.9,
            walkable_slope_degrees: 45.0,
            cell_size_fraction: 2.0,
            cell_height_fraction: 4.0,
            min_region_size: 8,
            merge_region_size: 20,
            max_simplification_error: 1.3,
            max_vertices_per_polygon: 6,
            edge_max_len_factor: 8,
            detail_sample_dist: 6.0,
            detail_sample_max_error: 1.0,
            tiling: false,
            tile_size: 32,
            connection_url: "http://127.0.0.1:15702".to_string(),
        }
    }
}
