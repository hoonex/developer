use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::{
    BoundaryMarkerId, BoundaryTriangle, Tetrahedron, VolumeMesh, VolumeMeshError,
};

use crate::boundary_layer::GeneratedTetrahedralBoundaryLayer;
use crate::tetra_overlap::{
    validate_tetrahedral_interior_overlaps, TetrahedralOverlapError, TetrahedralOverlapPolicy,
    TetrahedralOverlapReport,
};
use crate::tetgen_output::ParsedTetgenVolumeMesh;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoundaryLayerTetgenMergePolicy {
    pub interface_vertex_tolerance: f64,
    pub max_interface_vertex_comparisons: usize,
    pub max_combined_tetrahedra: usize,
    pub overlap_policy: TetrahedralOverlapPolicy,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoundaryLayerTetgenMergeReport {
    pub layer_count: usize,
    pub layer_tetrahedra: usize,
    pub tetgen_tetrahedra: usize,
    pub combined_tetrahedra: usize,
    pub welded_interface_vertices: usize,
    pub removed_layer_interface_faces: usize,
    pub removed_tetgen_interface_faces: usize,
    pub interface_vertex_comparisons: usize,
    pub overlap: TetrahedralOverlapReport,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MergedBoundaryLayerTetgenMesh {
    pub mesh: VolumeMesh,
    pub report: BoundaryLayerTetgenMergeReport,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BoundaryLayerTetgenMergeError {
    InvalidVertexTolerance { value: f64 },
    ZeroBudget { field: &'static str },
    DuplicateSceneObject { scene_object_id: u64 },
    DuplicateWallMarker { marker: BoundaryMarkerId },
    InvalidLayerMesh { scene_object_id: u64, message: String },
    UnexpectedLayerBoundaryMarker {
        scene_object_id: u64,
        marker: BoundaryMarkerId,
    },
    LayerOuterPointContractChanged { scene_object_id: u64, vertex: usize },
    MissingTetgenInterfaceFaces { marker: BoundaryMarkerId },
    InterfaceVertexBudgetOverflow,
    InterfaceVertexBudgetExceeded { requested: usize, limit: usize },
    UnmatchedTetgenInterfaceVertex {
        marker: BoundaryMarkerId,
        tetgen_vertex: u32,
    },
    AmbiguousTetgenInterfaceVertex {
        marker: BoundaryMarkerId,
        tetgen_vertex: u32,
        matches: usize,
    },
    DuplicateInterfaceVertexMatch {
        marker: BoundaryMarkerId,
        layer_vertex: u32,
    },
    UnusedLayerOuterVertex {
        marker: BoundaryMarkerId,
        layer_vertex: u32,
    },
    DuplicateInterfaceFacet { marker: BoundaryMarkerId },
    InterfaceFacetSetMismatch {
        marker: BoundaryMarkerId,
        expected: usize,
        actual: usize,
    },
    PointIndexOverflow,
    TetrahedronBudgetOverflow,
    TetrahedronBudgetExceeded { requested: usize, limit: usize },
    InvalidTetgenMesh(VolumeMeshError),
    VolumeAudit(VolumeMeshError),
    Overlap(TetrahedralOverlapError),
}

impl Display for BoundaryLayerTetgenMergeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidVertexTolerance { value } => write!(
                f,
                "boundary-layer/TetGen interface vertex tolerance must be finite and non-negative; got {value}"
            ),
            Self::ZeroBudget { field } => {
                write!(f, "boundary-layer/TetGen merge budget `{field}` must be non-zero")
            }
            Self::DuplicateSceneObject { scene_object_id } => write!(
                f,
                "boundary-layer/TetGen merge received duplicate layer SceneObject {scene_object_id}"
            ),
            Self::DuplicateWallMarker { marker } => write!(
                f,
                "boundary-layer/TetGen merge requires one unique canonical wall marker per layer; marker {:?} repeats",
                marker
            ),
            Self::InvalidLayerMesh {
                scene_object_id,
                message,
            } => write!(
                f,
                "SceneObject {scene_object_id} boundary-layer mesh failed audit before merge: {message}"
            ),
            Self::UnexpectedLayerBoundaryMarker {
                scene_object_id,
                marker,
            } => write!(
                f,
                "SceneObject {scene_object_id} boundary-layer mesh contains unexpected boundary marker {:?}",
                marker
            ),
            Self::LayerOuterPointContractChanged {
                scene_object_id,
                vertex,
            } => write!(
                f,
                "SceneObject {scene_object_id} boundary-layer outer point {vertex} no longer matches the retained outer surface"
            ),
            Self::MissingTetgenInterfaceFaces { marker } => write!(
                f,
                "TetGen output contains no preserved interface faces for canonical scene marker {:?}",
                marker
            ),
            Self::InterfaceVertexBudgetOverflow => write!(
                f,
                "boundary-layer/TetGen interface vertex comparison reservation overflowed usize"
            ),
            Self::InterfaceVertexBudgetExceeded { requested, limit } => write!(
                f,
                "boundary-layer/TetGen interface vertex matching requires {requested} comparisons, above configured limit {limit}"
            ),
            Self::UnmatchedTetgenInterfaceVertex {
                marker,
                tetgen_vertex,
            } => write!(
                f,
                "TetGen interface vertex {tetgen_vertex} on marker {:?} has no layer outer vertex within tolerance",
                marker
            ),
            Self::AmbiguousTetgenInterfaceVertex {
                marker,
                tetgen_vertex,
                matches,
            } => write!(
                f,
                "TetGen interface vertex {tetgen_vertex} on marker {:?} matches {matches} layer outer vertices within tolerance",
                marker
            ),
            Self::DuplicateInterfaceVertexMatch {
                marker,
                layer_vertex,
            } => write!(
                f,
                "more than one TetGen interface vertex on marker {:?} maps to layer outer vertex {layer_vertex}",
                marker
            ),
            Self::UnusedLayerOuterVertex {
                marker,
                layer_vertex,
            } => write!(
                f,
                "layer outer vertex {layer_vertex} for marker {:?} is absent from preserved TetGen interface faces",
                marker
            ),
            Self::DuplicateInterfaceFacet { marker } => write!(
                f,
                "TetGen preserved interface for marker {:?} contains a duplicate triangular facet",
                marker
            ),
            Self::InterfaceFacetSetMismatch {
                marker,
                expected,
                actual,
            } => write!(
                f,
                "TetGen preserved interface facet set for marker {:?} does not match the layer outer shell (expected {expected}, got {actual})",
                marker
            ),
            Self::PointIndexOverflow => write!(
                f,
                "combined boundary-layer/TetGen point count exceeds u32 VolumeMesh indexing"
            ),
            Self::TetrahedronBudgetOverflow => write!(
                f,
                "combined boundary-layer/TetGen tetrahedron count overflowed usize"
            ),
            Self::TetrahedronBudgetExceeded { requested, limit } => write!(
                f,
                "combined boundary-layer/TetGen mesh requires {requested} tetrahedra, above configured limit {limit}"
            ),
            Self::InvalidTetgenMesh(error) => write!(
                f,
                "TetGen mesh no longer satisfies the parsed VolumeMesh contract before merge: {error}"
            ),
            Self::VolumeAudit(error) => write!(
                f,
                "combined boundary-layer/TetGen VolumeMesh audit failed: {error}"
            ),
            Self::Overlap(error) => write!(
                f,
                "combined boundary-layer/TetGen overlap validation failed: {error}"
            ),
        }
    }
}

impl Error for BoundaryLayerTetgenMergeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidTetgenMesh(error) | Self::VolumeAudit(error) => Some(error),
            Self::Overlap(error) => Some(error),
            _ => None,
        }
    }
}

impl From<TetrahedralOverlapError> for BoundaryLayerTetgenMergeError {
    fn from(value: TetrahedralOverlapError) -> Self {
        Self::Overlap(value)
    }
}

/// Welds one audited tetrahedral boundary-layer block per SceneObject to the exact preserved
/// SceneObject facets in a TetGen far-field mesh.
///
/// The layer's inner physical wall keeps the canonical SceneObject wall marker. The layer's local
/// outer-interface marker and TetGen's temporary copy of the canonical SceneObject marker are both
/// removed from the final boundary; their triangles instead become ordinary two-cell interior
/// faces after vertex welding. All non-interface TetGen boundary faces (normally the six outer
/// domain boundaries) remain authoritative.
///
/// Success requires a bijective vertex match and an exact triangular facet-set match for every
/// interface marker, then a full `VolumeMesh::audit()` and bounded positive-volume overlap check on
/// the combined tetrahedral mesh. This proves a conforming tetrahedral join, not y+ adequacy,
/// sharp-feature layer semantics, solver/model suitability, or engineering CFD accuracy.
pub fn merge_tetgen_with_boundary_layers(
    tetgen: &ParsedTetgenVolumeMesh,
    layers: &[GeneratedTetrahedralBoundaryLayer],
    policy: BoundaryLayerTetgenMergePolicy,
) -> Result<MergedBoundaryLayerTetgenMesh, BoundaryLayerTetgenMergeError> {
    validate_policy(policy)?;
    tetgen
        .mesh
        .audit()
        .map_err(BoundaryLayerTetgenMergeError::InvalidTetgenMesh)?;
    if layers.is_empty() {
        return Err(BoundaryLayerTetgenMergeError::ZeroBudget { field: "layers" });
    }

    let layer_tetrahedra = layers.iter().try_fold(0_usize, |total, layer| {
        total
            .checked_add(layer.mesh.cells.len())
            .ok_or(BoundaryLayerTetgenMergeError::TetrahedronBudgetOverflow)
    })?;
    let combined_tetrahedra = layer_tetrahedra
        .checked_add(tetgen.mesh.cells.len())
        .ok_or(BoundaryLayerTetgenMergeError::TetrahedronBudgetOverflow)?;
    if combined_tetrahedra > policy.max_combined_tetrahedra {
        return Err(BoundaryLayerTetgenMergeError::TetrahedronBudgetExceeded {
            requested: combined_tetrahedra,
            limit: policy.max_combined_tetrahedra,
        });
    }

    let mut scene_ids = BTreeSet::new();
    let mut wall_markers = BTreeSet::new();
    for layer in layers {
        if !scene_ids.insert(layer.report.scene_object_id) {
            return Err(BoundaryLayerTetgenMergeError::DuplicateSceneObject {
                scene_object_id: layer.report.scene_object_id,
            });
        }
        if !wall_markers.insert(layer.wall_marker) {
            return Err(BoundaryLayerTetgenMergeError::DuplicateWallMarker {
                marker: layer.wall_marker,
            });
        }
        layer.mesh.audit().map_err(|error| {
            BoundaryLayerTetgenMergeError::InvalidLayerMesh {
                scene_object_id: layer.report.scene_object_id,
                message: error.to_string(),
            }
        })?;
    }

    let mut points = Vec::new();
    let mut cells = Vec::with_capacity(combined_tetrahedra);
    let mut boundary = Vec::new();
    let mut interfaces = BTreeMap::<BoundaryMarkerId, LayerInterface>::new();
    let mut removed_layer_interface_faces = 0_usize;

    for layer in layers {
        let point_offset = u32::try_from(points.len())
            .map_err(|_| BoundaryLayerTetgenMergeError::PointIndexOverflow)?;
        let source_vertices = layer.report.source_vertices;
        let outer_start = layer
            .mesh
            .points
            .len()
            .checked_sub(source_vertices)
            .ok_or_else(|| BoundaryLayerTetgenMergeError::InvalidLayerMesh {
                scene_object_id: layer.report.scene_object_id,
                message: "outer-shell point range underflow".into(),
            })?;
        if layer.outer_surface.positions.len() != source_vertices {
            return Err(BoundaryLayerTetgenMergeError::InvalidLayerMesh {
                scene_object_id: layer.report.scene_object_id,
                message: "outer-surface vertex count differs from retained source-vertex count".into(),
            });
        }
        for (vertex, expected) in layer.outer_surface.positions.iter().enumerate() {
            if layer.mesh.points[outer_start + vertex] != *expected {
                return Err(BoundaryLayerTetgenMergeError::LayerOuterPointContractChanged {
                    scene_object_id: layer.report.scene_object_id,
                    vertex,
                });
            }
        }

        points.extend_from_slice(&layer.mesh.points);
        for cell in &layer.mesh.cells {
            cells.push(Tetrahedron {
                vertices: add_offset4(cell.vertices, point_offset)?,
            });
        }

        let mut removed_for_layer = 0_usize;
        for face in &layer.mesh.boundary {
            if face.marker == layer.wall_marker {
                boundary.push(BoundaryTriangle {
                    vertices: add_offset3(face.vertices, point_offset)?,
                    marker: face.marker,
                });
            } else if face.marker == layer.interface_marker {
                removed_for_layer += 1;
            } else {
                return Err(BoundaryLayerTetgenMergeError::UnexpectedLayerBoundaryMarker {
                    scene_object_id: layer.report.scene_object_id,
                    marker: face.marker,
                });
            }
        }
        removed_layer_interface_faces += removed_for_layer;
        if removed_for_layer != layer.outer_surface.triangles.len() {
            return Err(BoundaryLayerTetgenMergeError::InvalidLayerMesh {
                scene_object_id: layer.report.scene_object_id,
                message: format!(
                    "outer interface has {removed_for_layer} boundary faces but retained outer surface has {} triangles",
                    layer.outer_surface.triangles.len()
                ),
            });
        }

        let mut outer_vertices = Vec::with_capacity(source_vertices);
        for vertex in 0..source_vertices {
            let local = outer_start
                .checked_add(vertex)
                .ok_or(BoundaryLayerTetgenMergeError::PointIndexOverflow)?;
            let local = u32::try_from(local)
                .map_err(|_| BoundaryLayerTetgenMergeError::PointIndexOverflow)?;
            outer_vertices.push(add_offset(local, point_offset)?);
        }
        let expected_facets = layer
            .outer_surface
            .triangles
            .iter()
            .map(|triangle| {
                Ok(sorted_triangle([
                    outer_vertices[triangle[0] as usize],
                    outer_vertices[triangle[1] as usize],
                    outer_vertices[triangle[2] as usize],
                ]))
            })
            .collect::<Result<BTreeSet<_>, BoundaryLayerTetgenMergeError>>()?;
        if expected_facets.len() != layer.outer_surface.triangles.len() {
            return Err(BoundaryLayerTetgenMergeError::InvalidLayerMesh {
                scene_object_id: layer.report.scene_object_id,
                message: "outer surface contains duplicate triangular facets".into(),
            });
        }
        interfaces.insert(
            layer.wall_marker,
            LayerInterface {
                outer_positions: layer.outer_surface.positions.clone(),
                outer_vertices,
                expected_facets,
            },
        );
    }

    let mut tetgen_interface_faces = BTreeMap::<BoundaryMarkerId, Vec<[u32; 3]>>::new();
    for face in &tetgen.mesh.boundary {
        if interfaces.contains_key(&face.marker) {
            tetgen_interface_faces
                .entry(face.marker)
                .or_default()
                .push(face.vertices);
        }
    }

    let mut comparison_reservation = 0_usize;
    for (&marker, interface) in &interfaces {
        let faces = tetgen_interface_faces
            .get(&marker)
            .ok_or(BoundaryLayerTetgenMergeError::MissingTetgenInterfaceFaces { marker })?;
        let unique_tetgen_vertices = faces
            .iter()
            .flat_map(|face| face.iter().copied())
            .collect::<BTreeSet<_>>()
            .len();
        let comparisons = unique_tetgen_vertices
            .checked_mul(interface.outer_vertices.len())
            .ok_or(BoundaryLayerTetgenMergeError::InterfaceVertexBudgetOverflow)?;
        comparison_reservation = comparison_reservation
            .checked_add(comparisons)
            .ok_or(BoundaryLayerTetgenMergeError::InterfaceVertexBudgetOverflow)?;
    }
    if comparison_reservation > policy.max_interface_vertex_comparisons {
        return Err(BoundaryLayerTetgenMergeError::InterfaceVertexBudgetExceeded {
            requested: comparison_reservation,
            limit: policy.max_interface_vertex_comparisons,
        });
    }

    let tolerance_squared = policy.interface_vertex_tolerance * policy.interface_vertex_tolerance;
    if !tolerance_squared.is_finite() {
        return Err(BoundaryLayerTetgenMergeError::InvalidVertexTolerance {
            value: policy.interface_vertex_tolerance,
        });
    }
    let mut tetgen_to_global = BTreeMap::<u32, u32>::new();
    let mut removed_tetgen_interface_faces = 0_usize;
    let mut executed_comparisons = 0_usize;

    for (&marker, interface) in &interfaces {
        let faces = &tetgen_interface_faces[&marker];
        let tetgen_vertices = faces
            .iter()
            .flat_map(|face| face.iter().copied())
            .collect::<BTreeSet<_>>();
        let mut used_layer_vertices = BTreeSet::new();
        for tetgen_vertex in tetgen_vertices {
            let point = tetgen.mesh.points[tetgen_vertex as usize];
            let mut matched = None;
            let mut matches = 0_usize;
            for (index, candidate) in interface.outer_positions.iter().enumerate() {
                executed_comparisons += 1;
                if squared_distance(point, *candidate) <= tolerance_squared {
                    matched = Some(interface.outer_vertices[index]);
                    matches += 1;
                }
            }
            let Some(layer_vertex) = matched else {
                return Err(BoundaryLayerTetgenMergeError::UnmatchedTetgenInterfaceVertex {
                    marker,
                    tetgen_vertex,
                });
            };
            if matches != 1 {
                return Err(BoundaryLayerTetgenMergeError::AmbiguousTetgenInterfaceVertex {
                    marker,
                    tetgen_vertex,
                    matches,
                });
            }
            if !used_layer_vertices.insert(layer_vertex) {
                return Err(BoundaryLayerTetgenMergeError::DuplicateInterfaceVertexMatch {
                    marker,
                    layer_vertex,
                });
            }
            tetgen_to_global.insert(tetgen_vertex, layer_vertex);
        }
        for &layer_vertex in &interface.outer_vertices {
            if !used_layer_vertices.contains(&layer_vertex) {
                return Err(BoundaryLayerTetgenMergeError::UnusedLayerOuterVertex {
                    marker,
                    layer_vertex,
                });
            }
        }

        let mut actual_facets = BTreeSet::new();
        for face in faces {
            let mapped = sorted_triangle([
                tetgen_to_global[&face[0]],
                tetgen_to_global[&face[1]],
                tetgen_to_global[&face[2]],
            ]);
            if !actual_facets.insert(mapped) {
                return Err(BoundaryLayerTetgenMergeError::DuplicateInterfaceFacet { marker });
            }
        }
        if actual_facets != interface.expected_facets {
            return Err(BoundaryLayerTetgenMergeError::InterfaceFacetSetMismatch {
                marker,
                expected: interface.expected_facets.len(),
                actual: actual_facets.len(),
            });
        }
        removed_tetgen_interface_faces += faces.len();
    }

    for (index, &point) in tetgen.mesh.points.iter().enumerate() {
        let tetgen_index = u32::try_from(index)
            .map_err(|_| BoundaryLayerTetgenMergeError::PointIndexOverflow)?;
        if tetgen_to_global.contains_key(&tetgen_index) {
            continue;
        }
        let global = u32::try_from(points.len())
            .map_err(|_| BoundaryLayerTetgenMergeError::PointIndexOverflow)?;
        points.push(point);
        tetgen_to_global.insert(tetgen_index, global);
    }

    for cell in &tetgen.mesh.cells {
        cells.push(Tetrahedron {
            vertices: remap4(cell.vertices, &tetgen_to_global),
        });
    }
    for face in &tetgen.mesh.boundary {
        if interfaces.contains_key(&face.marker) {
            continue;
        }
        boundary.push(BoundaryTriangle {
            vertices: remap3(face.vertices, &tetgen_to_global),
            marker: face.marker,
        });
    }

    let mesh = VolumeMesh {
        points,
        cells,
        boundary,
    };
    mesh.audit()
        .map_err(BoundaryLayerTetgenMergeError::VolumeAudit)?;
    let overlap = validate_tetrahedral_interior_overlaps(&mesh, policy.overlap_policy)?;

    Ok(MergedBoundaryLayerTetgenMesh {
        report: BoundaryLayerTetgenMergeReport {
            layer_count: layers.len(),
            layer_tetrahedra,
            tetgen_tetrahedra: tetgen.mesh.cells.len(),
            combined_tetrahedra,
            welded_interface_vertices: tetgen_interface_faces
                .values()
                .flat_map(|faces| faces.iter().flat_map(|face| face.iter().copied()))
                .collect::<BTreeSet<_>>()
                .len(),
            removed_layer_interface_faces,
            removed_tetgen_interface_faces,
            interface_vertex_comparisons: executed_comparisons,
            overlap,
        },
        mesh,
    })
}

#[derive(Clone, Debug)]
struct LayerInterface {
    outer_positions: Vec<[f64; 3]>,
    outer_vertices: Vec<u32>,
    expected_facets: BTreeSet<[u32; 3]>,
}

fn validate_policy(policy: BoundaryLayerTetgenMergePolicy) -> Result<(), BoundaryLayerTetgenMergeError> {
    if !policy.interface_vertex_tolerance.is_finite() || policy.interface_vertex_tolerance < 0.0 {
        return Err(BoundaryLayerTetgenMergeError::InvalidVertexTolerance {
            value: policy.interface_vertex_tolerance,
        });
    }
    if policy.max_interface_vertex_comparisons == 0 {
        return Err(BoundaryLayerTetgenMergeError::ZeroBudget {
            field: "max_interface_vertex_comparisons",
        });
    }
    if policy.max_combined_tetrahedra == 0 {
        return Err(BoundaryLayerTetgenMergeError::ZeroBudget {
            field: "max_combined_tetrahedra",
        });
    }
    Ok(())
}

fn add_offset(value: u32, offset: u32) -> Result<u32, BoundaryLayerTetgenMergeError> {
    value
        .checked_add(offset)
        .ok_or(BoundaryLayerTetgenMergeError::PointIndexOverflow)
}

fn add_offset3(
    values: [u32; 3],
    offset: u32,
) -> Result<[u32; 3], BoundaryLayerTetgenMergeError> {
    Ok([
        add_offset(values[0], offset)?,
        add_offset(values[1], offset)?,
        add_offset(values[2], offset)?,
    ])
}

fn add_offset4(
    values: [u32; 4],
    offset: u32,
) -> Result<[u32; 4], BoundaryLayerTetgenMergeError> {
    Ok([
        add_offset(values[0], offset)?,
        add_offset(values[1], offset)?,
        add_offset(values[2], offset)?,
        add_offset(values[3], offset)?,
    ])
}

fn remap3(values: [u32; 3], map: &BTreeMap<u32, u32>) -> [u32; 3] {
    [map[&values[0]], map[&values[1]], map[&values[2]]]
}

fn remap4(values: [u32; 4], map: &BTreeMap<u32, u32>) -> [u32; 4] {
    [
        map[&values[0]],
        map[&values[1]],
        map[&values[2]],
        map[&values[3]],
    ]
}

fn sorted_triangle(mut triangle: [u32; 3]) -> [u32; 3] {
    triangle.sort_unstable();
    triangle
}

fn squared_distance(first: [f64; 3], second: [f64; 3]) -> f64 {
    let dx = first[0] - second[0];
    let dy = first[1] - second[1];
    let dz = first[2] - second[2];
    dx * dx + dy * dy + dz * dz
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeroforge_geometry_core::SurfaceMesh;

    use crate::boundary_layer::{
        generate_tetrahedral_boundary_layer, TetrahedralBoundaryLayerPolicy,
    };
    use crate::imported_surface::{
        audit_imported_surface_for_accurate_meshing, AccurateImportedSurfacePolicy,
    };

    fn cube_surface() -> SurfaceMesh {
        SurfaceMesh {
            positions: vec![
                [1.0, 1.0, 1.0], [2.0, 1.0, 1.0], [2.0, 2.0, 1.0], [1.0, 2.0, 1.0],
                [1.0, 1.0, 2.0], [2.0, 1.0, 2.0], [2.0, 2.0, 2.0], [1.0, 2.0, 2.0],
            ],
            triangles: vec![
                [0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7],
                [0, 1, 5], [0, 5, 4], [3, 7, 6], [3, 6, 2],
                [0, 4, 7], [0, 7, 3], [1, 2, 6], [1, 6, 5],
            ],
        }
    }

    fn layer_policy(first: f64) -> TetrahedralBoundaryLayerPolicy {
        TetrahedralBoundaryLayerPolicy {
            first_layer_thickness: first,
            growth_ratio: 1.0,
            layer_count: 1,
            maximum_total_thickness: first * 1.000_001,
            maximum_adjacent_face_normal_angle_radians: std::f64::consts::PI,
            minimum_tetrahedron_volume: 1.0e-12,
            max_generated_tetrahedra: 10_000,
            overlap_geometric_epsilon: 1.0e-10,
            max_overlap_pair_tests: 1_000_000,
        }
    }

    fn merge_policy() -> BoundaryLayerTetgenMergePolicy {
        BoundaryLayerTetgenMergePolicy {
            interface_vertex_tolerance: 1.0e-12,
            max_interface_vertex_comparisons: 10_000,
            max_combined_tetrahedra: 10_000,
            overlap_policy: TetrahedralOverlapPolicy {
                geometric_epsilon: 1.0e-10,
                max_tetrahedron_pair_tests: 1_000_000,
            },
        }
    }

    #[test]
    fn adjacent_tetrahedral_shells_weld_into_one_audited_mesh() {
        let source = audit_imported_surface_for_accurate_meshing(
            42,
            &cube_surface(),
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap();
        let inner = generate_tetrahedral_boundary_layer(
            &source,
            BoundaryMarkerId(7),
            BoundaryMarkerId(99),
            layer_policy(0.05),
        )
        .unwrap();
        let outer_source = audit_imported_surface_for_accurate_meshing(
            42,
            &inner.outer_surface,
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap();
        let outer = generate_tetrahedral_boundary_layer(
            &outer_source,
            BoundaryMarkerId(7),
            BoundaryMarkerId(50),
            layer_policy(0.05),
        )
        .unwrap();
        let tetgen_like = ParsedTetgenVolumeMesh {
            mesh: outer.mesh.clone(),
            input_node_ids: (0..outer.mesh.points.len() as u64).collect(),
            tetrahedron_ids: (0..outer.mesh.cells.len() as u64).collect(),
            boundary_face_ids: (0..outer.mesh.boundary.len() as u64).collect(),
            reoriented_tetrahedra: 0,
        };

        let merged = merge_tetgen_with_boundary_layers(
            &tetgen_like,
            std::slice::from_ref(&inner),
            merge_policy(),
        )
        .unwrap();
        let audit = merged.mesh.audit().unwrap();

        assert_eq!(merged.report.layer_count, 1);
        assert_eq!(merged.report.layer_tetrahedra, 36);
        assert_eq!(merged.report.tetgen_tetrahedra, 36);
        assert_eq!(merged.report.combined_tetrahedra, 72);
        assert_eq!(merged.report.welded_interface_vertices, 8);
        assert_eq!(merged.report.removed_layer_interface_faces, 12);
        assert_eq!(merged.report.removed_tetgen_interface_faces, 12);
        assert_eq!(audit.cells, 72);
        assert_eq!(audit.marker_triangle_counts[&BoundaryMarkerId(7)], 12);
        assert_eq!(audit.marker_triangle_counts[&BoundaryMarkerId(50)], 12);
        assert!(!audit.marker_triangle_counts.contains_key(&BoundaryMarkerId(99)));
        assert_eq!(merged.report.overlap.cells, 72);
    }

    #[test]
    fn moved_interface_vertex_fails_before_weld() {
        let source = audit_imported_surface_for_accurate_meshing(
            42,
            &cube_surface(),
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap();
        let inner = generate_tetrahedral_boundary_layer(
            &source,
            BoundaryMarkerId(7),
            BoundaryMarkerId(99),
            layer_policy(0.05),
        )
        .unwrap();
        let outer_source = audit_imported_surface_for_accurate_meshing(
            42,
            &inner.outer_surface,
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap();
        let outer = generate_tetrahedral_boundary_layer(
            &outer_source,
            BoundaryMarkerId(7),
            BoundaryMarkerId(50),
            layer_policy(0.05),
        )
        .unwrap();
        let mut tetgen_like = ParsedTetgenVolumeMesh {
            mesh: outer.mesh.clone(),
            input_node_ids: vec![],
            tetrahedron_ids: vec![],
            boundary_face_ids: vec![],
            reoriented_tetrahedra: 0,
        };
        tetgen_like.mesh.points[0][0] += 1.0e-3;

        assert!(matches!(
            merge_tetgen_with_boundary_layers(
                &tetgen_like,
                &[inner],
                merge_policy(),
            ),
            Err(BoundaryLayerTetgenMergeError::UnmatchedTetgenInterfaceVertex { .. })
                | Err(BoundaryLayerTetgenMergeError::InterfaceFacetSetMismatch { .. })
                | Err(BoundaryLayerTetgenMergeError::InvalidTetgenMesh(_))
        ));
    }
}
