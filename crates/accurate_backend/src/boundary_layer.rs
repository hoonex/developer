use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_geometry_core::SurfaceMesh;
use aeroforge_volume_core::{
    BoundaryMarkerId, BoundaryTriangle, Tetrahedron, VolumeMesh,
};

use crate::imported_surface::AuditedImportedSurfaceBody;
use crate::tetra_overlap::{
    validate_tetrahedral_interior_overlaps, TetrahedralOverlapError,
    TetrahedralOverlapPolicy, TetrahedralOverlapReport,
};

/// Fail-closed upper bound for how far an angle-weighted vertex-normal direction may be amplified
/// to preserve the requested face-normal spacing at sharp vertices. This is an algorithmic safety
/// guard, not an engineering mesh-quality threshold.
const MAX_VERTEX_NORMAL_MITER_AMPLIFICATION: f64 = 4.0;

/// Explicit geometric policy for constructing a tetrahedralized wall-normal layer block around
/// one already-audited closed surface body.
///
/// Layer thicknesses follow a geometric progression. Vertices use angle-weighted outward normal
/// directions, then those directions are miter-scaled so the requested offset is preserved against
/// every incident source-face normal. Excessive or non-positive miter projection fails closed.
/// Each triangular shell prism is split deterministically into three tetrahedra using globally
/// sorted source vertex ids, which keeps shared prism side diagonals conforming between neighboring
/// source triangles.
///
/// This policy does not claim engineering near-wall adequacy. In particular it does not infer y+,
/// a turbulence-model target, a dimensional unit system, CAD feature semantics, or a safe layer
/// thickness from the source geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TetrahedralBoundaryLayerPolicy {
    pub first_layer_thickness: f64,
    pub growth_ratio: f64,
    pub layer_count: usize,
    pub maximum_total_thickness: f64,
    pub maximum_adjacent_face_normal_angle_radians: f64,
    pub minimum_tetrahedron_volume: f64,
    pub max_generated_tetrahedra: usize,
    pub overlap_geometric_epsilon: f64,
    pub max_overlap_pair_tests: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TetrahedralBoundaryLayerReport {
    pub scene_object_id: u64,
    pub layer_count: usize,
    pub source_vertices: usize,
    pub source_triangles: usize,
    pub generated_points: usize,
    pub generated_tetrahedra: usize,
    pub first_layer_thickness: f64,
    pub growth_ratio: f64,
    pub total_thickness: f64,
    pub maximum_adjacent_face_normal_angle_radians: f64,
    /// Smallest cosine projection from a unit angle-weighted vertex normal onto any incident face
    /// normal before miter scaling. Values near zero require large amplification and are rejected by
    /// the bounded miter guard.
    pub minimum_vertex_face_normal_projection: f64,
    /// Largest miter amplification actually applied to any source vertex.
    pub maximum_vertex_normal_amplification: f64,
    pub minimum_tetrahedron_volume: f64,
    pub maximum_tetrahedron_volume: f64,
    pub overlap: TetrahedralOverlapReport,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedTetrahedralBoundaryLayer {
    pub mesh: VolumeMesh,
    pub outer_surface: SurfaceMesh,
    /// Cumulative face-normal target offsets from the source wall. Entry zero is always `0.0`.
    pub layer_offsets: Vec<f64>,
    pub wall_marker: BoundaryMarkerId,
    pub interface_marker: BoundaryMarkerId,
    pub report: TetrahedralBoundaryLayerReport,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TetrahedralBoundaryLayerError {
    InvalidPolicy { field: &'static str, value: f64 },
    ZeroLayerCount,
    ZeroBudget { field: &'static str },
    InvalidMarker { marker: u32 },
    DuplicateMarkers { marker: u32 },
    SourceTopology(String),
    SourceNotClosedPositiveManifold,
    DegenerateSourceTriangle { triangle: usize },
    InvalidVertexNormal { vertex: usize },
    InvalidVertexFaceNormalProjection { vertex: usize, projection: f64 },
    VertexNormalMiterAmplificationExceeded {
        vertex: usize,
        amplification: f64,
        maximum: f64,
    },
    AdjacentFaceNormalAngleExceeded {
        edge: [u32; 2],
        angle_radians: f64,
        maximum_radians: f64,
    },
    LayerThicknessOverflow,
    TotalThicknessExceeded { total: f64, maximum: f64 },
    PointIndexOverflow,
    TetrahedronBudgetExceeded { requested: usize, limit: usize },
    NonFiniteGeneratedPoint { layer: usize, vertex: usize },
    DegenerateGeneratedTetrahedron {
        layer: usize,
        triangle: usize,
        local_tetrahedron: usize,
        absolute_volume: f64,
        minimum_volume: f64,
    },
    VolumeAudit(String),
    Overlap(TetrahedralOverlapError),
}

impl Display for TetrahedralBoundaryLayerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPolicy { field, value } => {
                write!(f, "boundary-layer policy field `{field}` is invalid: {value}")
            }
            Self::ZeroLayerCount => write!(f, "boundary-layer generation requires at least one layer"),
            Self::ZeroBudget { field } => {
                write!(f, "boundary-layer policy budget `{field}` must be non-zero")
            }
            Self::InvalidMarker { marker } => write!(
                f,
                "boundary-layer wall/interface markers must be non-zero; got {marker}"
            ),
            Self::DuplicateMarkers { marker } => write!(
                f,
                "boundary-layer wall and outer-interface markers must differ; both were {marker}"
            ),
            Self::SourceTopology(message) => {
                write!(f, "boundary-layer source topology audit failed: {message}")
            }
            Self::SourceNotClosedPositiveManifold => write!(
                f,
                "boundary-layer source must be one watertight consistently-oriented positive-volume manifold"
            ),
            Self::DegenerateSourceTriangle { triangle } => write!(
                f,
                "boundary-layer source triangle {triangle} has no finite non-zero normal"
            ),
            Self::InvalidVertexNormal { vertex } => write!(
                f,
                "boundary-layer source vertex {vertex} has no finite angle-weighted outward normal"
            ),
            Self::InvalidVertexFaceNormalProjection { vertex, projection } => write!(
                f,
                "boundary-layer source vertex {vertex} angle-weighted normal has non-positive/non-finite incident-face projection {projection}; safe outward miter extrusion is not established"
            ),
            Self::VertexNormalMiterAmplificationExceeded {
                vertex,
                amplification,
                maximum,
            } => write!(
                f,
                "boundary-layer source vertex {vertex} requires miter amplification {amplification}, above fail-closed maximum {maximum}"
            ),
            Self::AdjacentFaceNormalAngleExceeded {
                edge,
                angle_radians,
                maximum_radians,
            } => write!(
                f,
                "boundary-layer source edge {edge:?} has adjacent-face normal turn {angle_radians} rad above configured maximum {maximum_radians} rad"
            ),
            Self::LayerThicknessOverflow => write!(
                f,
                "boundary-layer geometric thickness schedule overflowed or became non-finite"
            ),
            Self::TotalThicknessExceeded { total, maximum } => write!(
                f,
                "boundary-layer total thickness {total} exceeds configured maximum {maximum}"
            ),
            Self::PointIndexOverflow => write!(
                f,
                "boundary-layer point count exceeds u32 volume-mesh indexing"
            ),
            Self::TetrahedronBudgetExceeded { requested, limit } => write!(
                f,
                "boundary-layer generation requires {requested} tetrahedra, above configured limit {limit}"
            ),
            Self::NonFiniteGeneratedPoint { layer, vertex } => write!(
                f,
                "boundary-layer generated point for layer {layer}, source vertex {vertex} is non-finite"
            ),
            Self::DegenerateGeneratedTetrahedron {
                layer,
                triangle,
                local_tetrahedron,
                absolute_volume,
                minimum_volume,
            } => write!(
                f,
                "boundary-layer layer {layer}, source triangle {triangle}, local tetrahedron {local_tetrahedron} has absolute volume {absolute_volume}, below required {minimum_volume}"
            ),
            Self::VolumeAudit(message) => {
                write!(f, "generated boundary-layer volume audit failed: {message}")
            }
            Self::Overlap(error) => write!(
                f,
                "generated boundary-layer tetrahedral overlap validation failed: {error}"
            ),
        }
    }
}

impl Error for TetrahedralBoundaryLayerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Overlap(error) => Some(error),
            _ => None,
        }
    }
}

impl From<TetrahedralOverlapError> for TetrahedralBoundaryLayerError {
    fn from(value: TetrahedralOverlapError) -> Self {
        Self::Overlap(value)
    }
}

/// Generates one standalone closed tetrahedral layer block between the original body surface and
/// an offset outer interface.
///
/// The original triangulated body is retained exactly as the inner wall. The outer interface uses
/// the same triangle connectivity on the final offset shell. Every intermediate triangular prism
/// is tetrahedralized deterministically and the resulting closed block must pass both
/// `VolumeMesh::audit()` and the bounded positive-volume overlap gate before it is returned.
///
/// Passing establishes a conforming, non-overlapping tetrahedral shell block under the caller's
/// explicit geometric policy plus the bounded internal miter guard. It does not yet establish
/// integration with the external TetGen far-field mesh, post-extrusion inter-body/domain clearance,
/// CAD sharp-feature semantics, solver/model-specific y+ adequacy, or engineering CFD accuracy.
pub fn generate_tetrahedral_boundary_layer(
    body: &AuditedImportedSurfaceBody,
    wall_marker: BoundaryMarkerId,
    interface_marker: BoundaryMarkerId,
    policy: TetrahedralBoundaryLayerPolicy,
) -> Result<GeneratedTetrahedralBoundaryLayer, TetrahedralBoundaryLayerError> {
    validate_policy(policy)?;
    validate_markers(wall_marker, interface_marker)?;

    let topology = body
        .mesh
        .topology_report()
        .map_err(|error| TetrahedralBoundaryLayerError::SourceTopology(error.to_string()))?;
    if !topology.watertight_two_manifold
        || !topology.consistently_oriented
        || topology.connected_components != 1
        || !topology
            .signed_volume
            .is_some_and(|volume| volume.is_finite() && volume > 0.0)
    {
        return Err(TetrahedralBoundaryLayerError::SourceNotClosedPositiveManifold);
    }

    let source_vertices = body.mesh.positions.len();
    let source_triangles = body.mesh.triangles.len();
    let point_count = source_vertices
        .checked_mul(policy.layer_count + 1)
        .ok_or(TetrahedralBoundaryLayerError::PointIndexOverflow)?;
    if point_count > u32::MAX as usize {
        return Err(TetrahedralBoundaryLayerError::PointIndexOverflow);
    }
    let tetrahedron_count = source_triangles
        .checked_mul(policy.layer_count)
        .and_then(|count| count.checked_mul(3))
        .ok_or(TetrahedralBoundaryLayerError::TetrahedronBudgetExceeded {
            requested: usize::MAX,
            limit: policy.max_generated_tetrahedra,
        })?;
    if tetrahedron_count > policy.max_generated_tetrahedra {
        return Err(TetrahedralBoundaryLayerError::TetrahedronBudgetExceeded {
            requested: tetrahedron_count,
            limit: policy.max_generated_tetrahedra,
        });
    }

    let (face_normals, observed_maximum_angle) = source_face_normals_and_turns(
        &body.mesh,
        policy.maximum_adjacent_face_normal_angle_radians,
    )?;
    let vertex_normals = angle_weighted_vertex_normals(&body.mesh, &face_normals)?;
    let (
        vertex_extrusions,
        minimum_vertex_face_normal_projection,
        maximum_vertex_normal_amplification,
    ) = face_spacing_preserving_vertex_extrusions(&body.mesh, &face_normals, &vertex_normals)?;
    let layer_offsets = build_layer_offsets(policy)?;
    let total_thickness = *layer_offsets
        .last()
        .expect("validated non-zero layer count must produce one outer offset");

    let mut points = Vec::with_capacity(point_count);
    for (layer, &offset) in layer_offsets.iter().enumerate() {
        for (vertex, (&point, &extrusion)) in body
            .mesh
            .positions
            .iter()
            .zip(vertex_extrusions.iter())
            .enumerate()
        {
            let generated = [
                point[0] + extrusion[0] * offset,
                point[1] + extrusion[1] * offset,
                point[2] + extrusion[2] * offset,
            ];
            if !generated.iter().all(|value| value.is_finite()) {
                return Err(TetrahedralBoundaryLayerError::NonFiniteGeneratedPoint {
                    layer,
                    vertex,
                });
            }
            points.push(generated);
        }
    }

    let point_index = |layer: usize, source_vertex: u32| -> u32 {
        (layer * source_vertices + source_vertex as usize) as u32
    };
    let mut cells = Vec::with_capacity(tetrahedron_count);
    let mut minimum_tetrahedron_volume = f64::INFINITY;
    let mut maximum_tetrahedron_volume = 0.0_f64;

    for layer in 0..policy.layer_count {
        for (triangle_index, triangle) in body.mesh.triangles.iter().copied().enumerate() {
            let mut ordered = triangle;
            ordered.sort_unstable();
            let [a, b, c] = ordered;
            let lower = [
                point_index(layer, a),
                point_index(layer, b),
                point_index(layer, c),
            ];
            let upper = [
                point_index(layer + 1, a),
                point_index(layer + 1, b),
                point_index(layer + 1, c),
            ];
            let prism_tetrahedra = [
                [lower[0], lower[1], lower[2], upper[2]],
                [lower[0], lower[1], upper[1], upper[2]],
                [lower[0], upper[0], upper[1], upper[2]],
            ];

            for (local_tetrahedron, mut vertices) in prism_tetrahedra.into_iter().enumerate() {
                let signed_volume = signed_tetrahedron_volume(&points, vertices);
                let absolute_volume = signed_volume.abs();
                if !absolute_volume.is_finite()
                    || absolute_volume < policy.minimum_tetrahedron_volume
                {
                    return Err(TetrahedralBoundaryLayerError::DegenerateGeneratedTetrahedron {
                        layer,
                        triangle: triangle_index,
                        local_tetrahedron,
                        absolute_volume,
                        minimum_volume: policy.minimum_tetrahedron_volume,
                    });
                }
                if signed_volume < 0.0 {
                    vertices.swap(0, 1);
                }
                minimum_tetrahedron_volume = minimum_tetrahedron_volume.min(absolute_volume);
                maximum_tetrahedron_volume = maximum_tetrahedron_volume.max(absolute_volume);
                cells.push(Tetrahedron { vertices });
            }
        }
    }

    let outer_layer = policy.layer_count;
    let mut boundary = Vec::with_capacity(source_triangles * 2);
    for triangle in body.mesh.triangles.iter().copied() {
        boundary.push(BoundaryTriangle {
            vertices: triangle,
            marker: wall_marker,
        });
        boundary.push(BoundaryTriangle {
            vertices: [
                point_index(outer_layer, triangle[0]),
                point_index(outer_layer, triangle[1]),
                point_index(outer_layer, triangle[2]),
            ],
            marker: interface_marker,
        });
    }

    let mesh = VolumeMesh {
        points,
        cells,
        boundary,
    };
    let mesh_report = mesh
        .audit()
        .map_err(|error| TetrahedralBoundaryLayerError::VolumeAudit(error.to_string()))?;
    debug_assert_eq!(mesh_report.cells, tetrahedron_count);

    let overlap = validate_tetrahedral_interior_overlaps(
        &mesh,
        TetrahedralOverlapPolicy {
            geometric_epsilon: policy.overlap_geometric_epsilon,
            max_tetrahedron_pair_tests: policy.max_overlap_pair_tests,
        },
    )?;

    let outer_start = outer_layer * source_vertices;
    let outer_surface = SurfaceMesh {
        positions: mesh.points[outer_start..outer_start + source_vertices].to_vec(),
        triangles: body.mesh.triangles.clone(),
    };

    Ok(GeneratedTetrahedralBoundaryLayer {
        mesh,
        outer_surface,
        layer_offsets,
        wall_marker,
        interface_marker,
        report: TetrahedralBoundaryLayerReport {
            scene_object_id: body.scene_object_id,
            layer_count: policy.layer_count,
            source_vertices,
            source_triangles,
            generated_points: point_count,
            generated_tetrahedra: tetrahedron_count,
            first_layer_thickness: policy.first_layer_thickness,
            growth_ratio: policy.growth_ratio,
            total_thickness,
            maximum_adjacent_face_normal_angle_radians: observed_maximum_angle,
            minimum_vertex_face_normal_projection,
            maximum_vertex_normal_amplification,
            minimum_tetrahedron_volume,
            maximum_tetrahedron_volume,
            overlap,
        },
    })
}

fn validate_policy(
    policy: TetrahedralBoundaryLayerPolicy,
) -> Result<(), TetrahedralBoundaryLayerError> {
    for (field, value, valid) in [
        (
            "first_layer_thickness",
            policy.first_layer_thickness,
            policy.first_layer_thickness.is_finite() && policy.first_layer_thickness > 0.0,
        ),
        (
            "growth_ratio",
            policy.growth_ratio,
            policy.growth_ratio.is_finite() && policy.growth_ratio >= 1.0,
        ),
        (
            "maximum_total_thickness",
            policy.maximum_total_thickness,
            policy.maximum_total_thickness.is_finite() && policy.maximum_total_thickness > 0.0,
        ),
        (
            "maximum_adjacent_face_normal_angle_radians",
            policy.maximum_adjacent_face_normal_angle_radians,
            policy.maximum_adjacent_face_normal_angle_radians.is_finite()
                && policy.maximum_adjacent_face_normal_angle_radians >= 0.0
                && policy.maximum_adjacent_face_normal_angle_radians <= std::f64::consts::PI,
        ),
        (
            "minimum_tetrahedron_volume",
            policy.minimum_tetrahedron_volume,
            policy.minimum_tetrahedron_volume.is_finite()
                && policy.minimum_tetrahedron_volume > 0.0,
        ),
        (
            "overlap_geometric_epsilon",
            policy.overlap_geometric_epsilon,
            policy.overlap_geometric_epsilon.is_finite()
                && policy.overlap_geometric_epsilon >= 0.0,
        ),
    ] {
        if !valid {
            return Err(TetrahedralBoundaryLayerError::InvalidPolicy { field, value });
        }
    }
    if policy.layer_count == 0 {
        return Err(TetrahedralBoundaryLayerError::ZeroLayerCount);
    }
    if policy.max_generated_tetrahedra == 0 {
        return Err(TetrahedralBoundaryLayerError::ZeroBudget {
            field: "max_generated_tetrahedra",
        });
    }
    if policy.max_overlap_pair_tests == 0 {
        return Err(TetrahedralBoundaryLayerError::ZeroBudget {
            field: "max_overlap_pair_tests",
        });
    }
    Ok(())
}

fn validate_markers(
    wall_marker: BoundaryMarkerId,
    interface_marker: BoundaryMarkerId,
) -> Result<(), TetrahedralBoundaryLayerError> {
    if wall_marker.0 == 0 {
        return Err(TetrahedralBoundaryLayerError::InvalidMarker {
            marker: wall_marker.0,
        });
    }
    if interface_marker.0 == 0 {
        return Err(TetrahedralBoundaryLayerError::InvalidMarker {
            marker: interface_marker.0,
        });
    }
    if wall_marker == interface_marker {
        return Err(TetrahedralBoundaryLayerError::DuplicateMarkers {
            marker: wall_marker.0,
        });
    }
    Ok(())
}

fn build_layer_offsets(
    policy: TetrahedralBoundaryLayerPolicy,
) -> Result<Vec<f64>, TetrahedralBoundaryLayerError> {
    let mut offsets = Vec::with_capacity(policy.layer_count + 1);
    offsets.push(0.0);
    let mut thickness = policy.first_layer_thickness;
    let mut total = 0.0_f64;
    for layer in 0..policy.layer_count {
        total += thickness;
        if !total.is_finite() {
            return Err(TetrahedralBoundaryLayerError::LayerThicknessOverflow);
        }
        if total > policy.maximum_total_thickness {
            return Err(TetrahedralBoundaryLayerError::TotalThicknessExceeded {
                total,
                maximum: policy.maximum_total_thickness,
            });
        }
        offsets.push(total);
        if layer + 1 < policy.layer_count {
            thickness *= policy.growth_ratio;
            if !thickness.is_finite() || thickness <= 0.0 {
                return Err(TetrahedralBoundaryLayerError::LayerThicknessOverflow);
            }
        }
    }
    Ok(offsets)
}

fn source_face_normals_and_turns(
    mesh: &SurfaceMesh,
    maximum_angle: f64,
) -> Result<(Vec<[f64; 3]>, f64), TetrahedralBoundaryLayerError> {
    let mut face_normals = Vec::with_capacity(mesh.triangles.len());
    let mut edge_faces = BTreeMap::<[u32; 2], Vec<usize>>::new();

    for (triangle_index, triangle) in mesh.triangles.iter().copied().enumerate() {
        let a = mesh.positions[triangle[0] as usize];
        let b = mesh.positions[triangle[1] as usize];
        let c = mesh.positions[triangle[2] as usize];
        let ab = direction(a, b).ok_or(
            TetrahedralBoundaryLayerError::DegenerateSourceTriangle {
                triangle: triangle_index,
            },
        )?;
        let ac = direction(a, c).ok_or(
            TetrahedralBoundaryLayerError::DegenerateSourceTriangle {
                triangle: triangle_index,
            },
        )?;
        let normal = normalized(cross(ab, ac)).ok_or(
            TetrahedralBoundaryLayerError::DegenerateSourceTriangle {
                triangle: triangle_index,
            },
        )?;
        face_normals.push(normal);

        for [first, second] in [
            [triangle[0], triangle[1]],
            [triangle[1], triangle[2]],
            [triangle[2], triangle[0]],
        ] {
            let edge = if first < second {
                [first, second]
            } else {
                [second, first]
            };
            edge_faces.entry(edge).or_default().push(triangle_index);
        }
    }

    let mut observed_maximum = 0.0_f64;
    for (edge, faces) in edge_faces {
        if faces.len() != 2 {
            return Err(TetrahedralBoundaryLayerError::SourceNotClosedPositiveManifold);
        }
        let cosine = dot(face_normals[faces[0]], face_normals[faces[1]]).clamp(-1.0, 1.0);
        let angle = cosine.acos();
        observed_maximum = observed_maximum.max(angle);
        if angle > maximum_angle {
            return Err(TetrahedralBoundaryLayerError::AdjacentFaceNormalAngleExceeded {
                edge,
                angle_radians: angle,
                maximum_radians: maximum_angle,
            });
        }
    }
    Ok((face_normals, observed_maximum))
}

fn angle_weighted_vertex_normals(
    mesh: &SurfaceMesh,
    face_normals: &[[f64; 3]],
) -> Result<Vec<[f64; 3]>, TetrahedralBoundaryLayerError> {
    let mut sums = vec![[0.0_f64; 3]; mesh.positions.len()];

    for (triangle_index, triangle) in mesh.triangles.iter().copied().enumerate() {
        for corner in 0..3 {
            let vertex = triangle[corner];
            let first = triangle[(corner + 1) % 3];
            let second = triangle[(corner + 2) % 3];
            let origin = mesh.positions[vertex as usize];
            let first_direction = direction(origin, mesh.positions[first as usize]).ok_or(
                TetrahedralBoundaryLayerError::DegenerateSourceTriangle {
                    triangle: triangle_index,
                },
            )?;
            let second_direction = direction(origin, mesh.positions[second as usize]).ok_or(
                TetrahedralBoundaryLayerError::DegenerateSourceTriangle {
                    triangle: triangle_index,
                },
            )?;
            let angle = dot(first_direction, second_direction).clamp(-1.0, 1.0).acos();
            if !angle.is_finite() || angle <= 0.0 {
                return Err(TetrahedralBoundaryLayerError::DegenerateSourceTriangle {
                    triangle: triangle_index,
                });
            }
            for axis in 0..3 {
                sums[vertex as usize][axis] += face_normals[triangle_index][axis] * angle;
            }
        }
    }

    sums.into_iter()
        .enumerate()
        .map(|(vertex, sum)| {
            normalized(sum).ok_or(TetrahedralBoundaryLayerError::InvalidVertexNormal { vertex })
        })
        .collect()
}

fn face_spacing_preserving_vertex_extrusions(
    mesh: &SurfaceMesh,
    face_normals: &[[f64; 3]],
    vertex_normals: &[[f64; 3]],
) -> Result<(Vec<[f64; 3]>, f64, f64), TetrahedralBoundaryLayerError> {
    let mut incident_faces = vec![Vec::<usize>::new(); mesh.positions.len()];
    for (face, triangle) in mesh.triangles.iter().enumerate() {
        for &vertex in triangle {
            incident_faces[vertex as usize].push(face);
        }
    }

    let mut minimum_projection = f64::INFINITY;
    let mut maximum_amplification = 1.0_f64;
    let mut extrusions = Vec::with_capacity(vertex_normals.len());
    for (vertex, &normal) in vertex_normals.iter().enumerate() {
        let projection = incident_faces[vertex]
            .iter()
            .map(|&face| dot(normal, face_normals[face]))
            .fold(f64::INFINITY, f64::min);
        if !projection.is_finite() || projection <= 0.0 {
            return Err(TetrahedralBoundaryLayerError::InvalidVertexFaceNormalProjection {
                vertex,
                projection,
            });
        }
        let amplification = 1.0 / projection;
        if !amplification.is_finite()
            || amplification > MAX_VERTEX_NORMAL_MITER_AMPLIFICATION
        {
            return Err(
                TetrahedralBoundaryLayerError::VertexNormalMiterAmplificationExceeded {
                    vertex,
                    amplification,
                    maximum: MAX_VERTEX_NORMAL_MITER_AMPLIFICATION,
                },
            );
        }
        minimum_projection = minimum_projection.min(projection);
        maximum_amplification = maximum_amplification.max(amplification);
        extrusions.push([
            normal[0] * amplification,
            normal[1] * amplification,
            normal[2] * amplification,
        ]);
    }
    Ok((extrusions, minimum_projection, maximum_amplification))
}

fn signed_tetrahedron_volume(points: &[[f64; 3]], vertices: [u32; 4]) -> f64 {
    let a = points[vertices[0] as usize];
    let b = points[vertices[1] as usize];
    let c = points[vertices[2] as usize];
    let d = points[vertices[3] as usize];
    dot(sub(b, a), cross(sub(c, a), sub(d, a))) / 6.0
}

fn direction(start: [f64; 3], end: [f64; 3]) -> Option<[f64; 3]> {
    let scale = start
        .iter()
        .chain(end.iter())
        .fold(1.0_f64, |current, value| current.max(value.abs()));
    normalized([
        end[0] / scale - start[0] / scale,
        end[1] / scale - start[1] / scale,
        end[2] / scale - start[2] / scale,
    ])
}

fn normalized(vector: [f64; 3]) -> Option<[f64; 3]> {
    let scale = vector
        .iter()
        .fold(0.0_f64, |current, value| current.max(value.abs()));
    if scale == 0.0 || !scale.is_finite() {
        return None;
    }
    let scaled = [
        vector[0] / scale,
        vector[1] / scale,
        vector[2] / scale,
    ];
    let length = dot(scaled, scaled).sqrt();
    if length == 0.0 || !length.is_finite() {
        None
    } else {
        Some([
            scaled[0] / length,
            scaled[1] / length,
            scaled[2] / length,
        ])
    }
}

fn sub(first: [f64; 3], second: [f64; 3]) -> [f64; 3] {
    [
        first[0] - second[0],
        first[1] - second[1],
        first[2] - second[2],
    ]
}

fn cross(first: [f64; 3], second: [f64; 3]) -> [f64; 3] {
    [
        first[1] * second[2] - first[2] * second[1],
        first[2] * second[0] - first[0] * second[2],
        first[0] * second[1] - first[1] * second[0],
    ]
}

fn dot(first: [f64; 3], second: [f64; 3]) -> f64 {
    first[0] * second[0] + first[1] * second[1] + first[2] * second[2]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imported_surface::{
        audit_imported_surface_for_accurate_meshing, AccurateImportedSurfacePolicy,
    };

    fn cube_surface() -> SurfaceMesh {
        SurfaceMesh {
            positions: vec![
                [1.0, 1.0, 1.0],
                [2.0, 1.0, 1.0],
                [2.0, 2.0, 1.0],
                [1.0, 2.0, 1.0],
                [1.0, 1.0, 2.0],
                [2.0, 1.0, 2.0],
                [2.0, 2.0, 2.0],
                [1.0, 2.0, 2.0],
            ],
            triangles: vec![
                [0, 2, 1],
                [0, 3, 2],
                [4, 5, 6],
                [4, 6, 7],
                [0, 1, 5],
                [0, 5, 4],
                [3, 7, 6],
                [3, 6, 2],
                [0, 4, 7],
                [0, 7, 3],
                [1, 2, 6],
                [1, 6, 5],
            ],
        }
    }

    fn audited_cube() -> AuditedImportedSurfaceBody {
        audit_imported_surface_for_accurate_meshing(
            42,
            &cube_surface(),
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap()
    }

    fn policy() -> TetrahedralBoundaryLayerPolicy {
        TetrahedralBoundaryLayerPolicy {
            first_layer_thickness: 0.05,
            growth_ratio: 1.2,
            layer_count: 3,
            maximum_total_thickness: 0.2,
            maximum_adjacent_face_normal_angle_radians: std::f64::consts::PI,
            minimum_tetrahedron_volume: 1.0e-12,
            max_generated_tetrahedra: 1_000,
            overlap_geometric_epsilon: 1.0e-10,
            max_overlap_pair_tests: 100_000,
        }
    }

    #[test]
    fn cube_builds_conforming_three_layer_tetrahedral_shell_block() {
        let body = audited_cube();
        let generated = generate_tetrahedral_boundary_layer(
            &body,
            BoundaryMarkerId(7),
            BoundaryMarkerId(8),
            policy(),
        )
        .unwrap();

        assert_eq!(generated.layer_offsets.len(), 4);
        assert!((generated.layer_offsets[1] - 0.05).abs() < 1.0e-12);
        assert!((generated.layer_offsets[2] - 0.11).abs() < 1.0e-12);
        assert!((generated.layer_offsets[3] - 0.182).abs() < 1.0e-12);
        assert_eq!(generated.mesh.points.len(), 32);
        assert_eq!(generated.mesh.cells.len(), 108);
        assert_eq!(generated.mesh.boundary.len(), 24);
        assert_eq!(generated.outer_surface.positions.len(), 8);
        assert_eq!(generated.outer_surface.triangles, body.mesh.triangles);

        let audit = generated.mesh.audit().unwrap();
        assert_eq!(audit.cells, 108);
        assert_eq!(audit.boundary_triangles, 24);
        assert_eq!(audit.marker_triangle_counts[&BoundaryMarkerId(7)], 12);
        assert_eq!(audit.marker_triangle_counts[&BoundaryMarkerId(8)], 12);

        assert_eq!(generated.report.scene_object_id, 42);
        assert_eq!(generated.report.layer_count, 3);
        assert_eq!(generated.report.generated_tetrahedra, 108);
        assert!((generated.report.total_thickness - 0.182).abs() < 1.0e-12);
        assert!((generated.report.minimum_vertex_face_normal_projection - 1.0 / 3.0_f64.sqrt()).abs() < 1.0e-12);
        assert!((generated.report.maximum_vertex_normal_amplification - 3.0_f64.sqrt()).abs() < 1.0e-12);
        assert!(generated.report.minimum_tetrahedron_volume >= 1.0e-12);
        assert!(generated.report.maximum_tetrahedron_volume >= generated.report.minimum_tetrahedron_volume);
        assert_eq!(generated.report.overlap.cells, 108);

        for vertex in 0..body.mesh.positions.len() {
            assert_eq!(generated.mesh.points[vertex], body.mesh.positions[vertex]);
        }
        let first = generated.mesh.points[body.mesh.positions.len()];
        let source = body.mesh.positions[0];
        let displacement = sub(first, source);
        for face_normal in [[-1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]] {
            assert!((dot(displacement, face_normal) - 0.05).abs() < 1.0e-12);
        }
        assert!((dot(displacement, displacement).sqrt() - 0.05 * 3.0_f64.sqrt()).abs() < 1.0e-12);
    }

    #[test]
    fn explicit_normal_turn_limit_rejects_sharp_cube_edges() {
        let body = audited_cube();
        let mut strict = policy();
        strict.maximum_adjacent_face_normal_angle_radians = 1.0;
        let error = generate_tetrahedral_boundary_layer(
            &body,
            BoundaryMarkerId(7),
            BoundaryMarkerId(8),
            strict,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            TetrahedralBoundaryLayerError::AdjacentFaceNormalAngleExceeded { .. }
        ));
    }

    #[test]
    fn total_thickness_cap_is_fail_closed() {
        let body = audited_cube();
        let mut limited = policy();
        limited.maximum_total_thickness = 0.18;
        let error = generate_tetrahedral_boundary_layer(
            &body,
            BoundaryMarkerId(7),
            BoundaryMarkerId(8),
            limited,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            TetrahedralBoundaryLayerError::TotalThicknessExceeded { .. }
        ));
    }

    #[test]
    fn wall_and_interface_markers_must_be_distinct() {
        let body = audited_cube();
        let error = generate_tetrahedral_boundary_layer(
            &body,
            BoundaryMarkerId(7),
            BoundaryMarkerId(7),
            policy(),
        )
        .unwrap_err();

        assert_eq!(
            error,
            TetrahedralBoundaryLayerError::DuplicateMarkers { marker: 7 }
        );
    }
}
