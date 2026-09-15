use std::collections::BTreeMap;
use std::error::Error;
use std::f64::consts::PI;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::{BoundaryMarkerId, VolumeMesh};

use crate::boundary_orientation::{
    orient_exterior_boundary_triangles, BoundaryOrientationError,
};
use crate::exterior_mesh::{
    validate_declared_exterior_fluid_mesh_input, DeclaredExteriorFluidMeshError,
};
use crate::imported_surface::AuditedImportedSurfaceBody;
use crate::su2_mesh::{BoundarySource, Su2MarkerMap};

/// Bounded policy for sharp-crease edge correspondence between audited source surfaces and
/// SceneObject body boundaries.
///
/// An edge is a feature when the unsigned angle between its two adjacent triangle normals is at
/// least `minimum_feature_angle_radians`. The remaining fields bound local edge association,
/// orientation-independent direction agreement, dihedral-angle agreement, and total pair work.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceBoundaryFeatureEdgePolicy {
    pub minimum_feature_angle_radians: f64,
    pub distance_tolerance: f64,
    pub minimum_direction_alignment_cosine: f64,
    pub maximum_dihedral_angle_difference_radians: f64,
    pub max_edge_pair_tests: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceBoundaryFeatureEdgeBodyReport {
    pub scene_object_id: u64,
    pub source_feature_edge_count: usize,
    pub boundary_feature_edge_count: usize,
    pub max_source_to_boundary_midpoint_distance: f64,
    pub max_boundary_to_source_midpoint_distance: f64,
    pub min_source_to_boundary_direction_alignment_cosine: f64,
    pub min_boundary_to_source_direction_alignment_cosine: f64,
    pub max_source_to_boundary_dihedral_angle_difference_radians: f64,
    pub max_boundary_to_source_dihedral_angle_difference_radians: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceBoundaryFeatureEdgeReport {
    pub bodies: Vec<SourceBoundaryFeatureEdgeBodyReport>,
    pub edge_pair_tests: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeatureEdgeComparisonDirection {
    SourceToBoundary,
    BoundaryToSource,
}

impl Display for FeatureEdgeComparisonDirection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceToBoundary => write!(f, "source->boundary"),
            Self::BoundaryToSource => write!(f, "boundary->source"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeatureEdgeSurface {
    Source,
    Boundary,
}

impl Display for FeatureEdgeSurface {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source => write!(f, "source"),
            Self::Boundary => write!(f, "body-boundary"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SourceBoundaryFeatureEdgeError {
    Exterior(DeclaredExteriorFluidMeshError),
    BoundaryOrientation(BoundaryOrientationError),
    InvalidPolicy { field: &'static str, value: f64 },
    ZeroComparisonBudget,
    DuplicateSourceSceneObject { scene_object_id: u64 },
    MissingSourceForBoundary { scene_object_id: u64 },
    MissingBoundaryForSource { scene_object_id: u64 },
    SourceSurfaceAuditInvalid { scene_object_id: u64, message: String },
    DegenerateTriangle {
        scene_object_id: u64,
        surface: FeatureEdgeSurface,
        triangle: usize,
    },
    InvalidEdgeUse {
        scene_object_id: u64,
        surface: FeatureEdgeSurface,
        edge: [u32; 2],
        uses: usize,
    },
    DegenerateEdge {
        scene_object_id: u64,
        surface: FeatureEdgeSurface,
        edge: [u32; 2],
    },
    FeaturePresenceMismatch {
        scene_object_id: u64,
        source_feature_edges: usize,
        boundary_feature_edges: usize,
    },
    ComparisonBudgetOverflow,
    ComparisonBudgetExceeded { requested: usize, limit: usize },
    DistanceToleranceExceeded {
        scene_object_id: u64,
        direction: FeatureEdgeComparisonDirection,
        edge: usize,
        distance: f64,
        tolerance: f64,
    },
    DirectionAlignmentBelowThreshold {
        scene_object_id: u64,
        direction: FeatureEdgeComparisonDirection,
        edge: usize,
        alignment_cosine: f64,
        minimum: f64,
    },
    DihedralAngleDifferenceExceeded {
        scene_object_id: u64,
        direction: FeatureEdgeComparisonDirection,
        edge: usize,
        difference_radians: f64,
        maximum_radians: f64,
    },
}

impl Display for SourceBoundaryFeatureEdgeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exterior(error) => write!(f, "exterior-fluid input contract failed: {error}"),
            Self::BoundaryOrientation(error) => {
                write!(f, "canonical exterior boundary orientation failed: {error}")
            }
            Self::InvalidPolicy { field, value } => {
                write!(f, "source/boundary feature-edge policy field {field} is invalid: {value}")
            }
            Self::ZeroComparisonBudget => write!(
                f,
                "source/boundary feature-edge validation requires a non-zero edge-pair budget"
            ),
            Self::DuplicateSourceSceneObject { scene_object_id } => write!(
                f,
                "source/boundary feature-edge validation received duplicate source SceneObject {scene_object_id}"
            ),
            Self::MissingSourceForBoundary { scene_object_id } => write!(
                f,
                "body boundary for SceneObject {scene_object_id} has no audited source surface"
            ),
            Self::MissingBoundaryForSource { scene_object_id } => write!(
                f,
                "audited source SceneObject {scene_object_id} has no body boundary"
            ),
            Self::SourceSurfaceAuditInvalid { scene_object_id, message } => write!(
                f,
                "source SceneObject {scene_object_id} no longer satisfies the accurate surface audit: {message}"
            ),
            Self::DegenerateTriangle { scene_object_id, surface, triangle } => write!(
                f,
                "{surface} triangle {triangle} for SceneObject {scene_object_id} has no finite unit normal"
            ),
            Self::InvalidEdgeUse { scene_object_id, surface, edge, uses } => write!(
                f,
                "{surface} edge {edge:?} for SceneObject {scene_object_id} has {uses} adjacent triangles; expected two"
            ),
            Self::DegenerateEdge { scene_object_id, surface, edge } => write!(
                f,
                "{surface} edge {edge:?} for SceneObject {scene_object_id} has zero or non-finite length"
            ),
            Self::FeaturePresenceMismatch {
                scene_object_id,
                source_feature_edges,
                boundary_feature_edges,
            } => write!(
                f,
                "SceneObject {scene_object_id} feature-edge presence differs: source={source_feature_edges}, boundary={boundary_feature_edges}"
            ),
            Self::ComparisonBudgetOverflow => write!(
                f,
                "source/boundary feature-edge comparison count overflowed usize"
            ),
            Self::ComparisonBudgetExceeded { requested, limit } => write!(
                f,
                "source/boundary feature-edge validation requires {requested} edge-pair tests, above explicit limit {limit}"
            ),
            Self::DistanceToleranceExceeded {
                scene_object_id,
                direction,
                edge,
                distance,
                tolerance,
            } => write!(
                f,
                "SceneObject {scene_object_id} {direction} feature edge {edge} nearest opposite-edge distance {distance} exceeds tolerance {tolerance}"
            ),
            Self::DirectionAlignmentBelowThreshold {
                scene_object_id,
                direction,
                edge,
                alignment_cosine,
                minimum,
            } => write!(
                f,
                "SceneObject {scene_object_id} {direction} feature edge {edge} direction alignment cosine {alignment_cosine} is below {minimum}"
            ),
            Self::DihedralAngleDifferenceExceeded {
                scene_object_id,
                direction,
                edge,
                difference_radians,
                maximum_radians,
            } => write!(
                f,
                "SceneObject {scene_object_id} {direction} feature edge {edge} dihedral difference {difference_radians} rad exceeds {maximum_radians} rad"
            ),
        }
    }
}

impl Error for SourceBoundaryFeatureEdgeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Exterior(error) => Some(error),
            Self::BoundaryOrientation(error) => Some(error),
            _ => None,
        }
    }
}

impl From<DeclaredExteriorFluidMeshError> for SourceBoundaryFeatureEdgeError {
    fn from(value: DeclaredExteriorFluidMeshError) -> Self {
        Self::Exterior(value)
    }
}

impl From<BoundaryOrientationError> for SourceBoundaryFeatureEdgeError {
    fn from(value: BoundaryOrientationError) -> Self {
        Self::BoundaryOrientation(value)
    }
}

#[derive(Clone, Copy)]
struct FeatureEdge {
    points: [[f64; 3]; 2],
    midpoint: [f64; 3],
    unit_direction: [f64; 3],
    dihedral_angle_radians: f64,
}

struct PreparedBodyFeatureComparison {
    scene_object_id: u64,
    source: Vec<FeatureEdge>,
    boundary: Vec<FeatureEdge>,
}

/// Validates bounded bidirectional sharp-crease edge correspondence for every SceneObject body.
///
/// Source and canonical volume-side body boundaries are independently converted to manifold edge
/// maps. Every selected feature-edge midpoint scans every selected opposite edge. The nearest
/// segment must be local, directionally aligned independent of edge orientation, and have a
/// sufficiently similar unsigned dihedral angle. Complete bidirectional pair work is reserved
/// before geometric comparison.
///
/// Passing establishes only this sharp-crease edge correspondence contract. It does not establish
/// exact source/output edge identity, smooth-curvature preservation, CAD-feature preservation,
/// body-fitted fidelity, boundary-layer quality, or CFD accuracy.
pub fn validate_source_boundary_feature_edges(
    mesh: &VolumeMesh,
    marker_map: &Su2MarkerMap,
    audited_sources: &[AuditedImportedSurfaceBody],
    policy: SourceBoundaryFeatureEdgePolicy,
) -> Result<SourceBoundaryFeatureEdgeReport, SourceBoundaryFeatureEdgeError> {
    validate_policy(policy)?;
    let exterior = validate_declared_exterior_fluid_mesh_input(mesh, marker_map)?;
    let oriented_boundary = orient_exterior_boundary_triangles(mesh)?;

    let mut sources = BTreeMap::<u64, &AuditedImportedSurfaceBody>::new();
    for source in audited_sources {
        if sources.insert(source.scene_object_id, source).is_some() {
            return Err(SourceBoundaryFeatureEdgeError::DuplicateSourceSceneObject {
                scene_object_id: source.scene_object_id,
            });
        }
        validate_source_still_audited(source)?;
    }

    let marker_scene_ids = marker_map
        .bindings
        .iter()
        .filter_map(|binding| match &binding.source {
            BoundarySource::SceneObject { scene_object_id } => {
                Some((binding.marker, *scene_object_id))
            }
            _ => None,
        })
        .collect::<BTreeMap<BoundaryMarkerId, u64>>();
    let mut body_boundary = BTreeMap::<u64, Vec<[u32; 3]>>::new();
    for boundary in oriented_boundary {
        if let Some(&scene_object_id) = marker_scene_ids.get(&boundary.marker) {
            body_boundary.entry(scene_object_id).or_default().push(boundary.vertices);
        }
    }

    for &scene_object_id in &exterior.scene_object_ids {
        if !sources.contains_key(&scene_object_id) {
            return Err(SourceBoundaryFeatureEdgeError::MissingSourceForBoundary { scene_object_id });
        }
    }
    for &scene_object_id in sources.keys() {
        if !body_boundary.contains_key(&scene_object_id) {
            return Err(SourceBoundaryFeatureEdgeError::MissingBoundaryForSource { scene_object_id });
        }
    }

    let mut prepared = Vec::with_capacity(exterior.scene_object_ids.len());
    let mut requested_tests = 0_usize;
    for &scene_object_id in &exterior.scene_object_ids {
        let source = feature_edges(
            &sources[&scene_object_id].mesh.positions,
            &sources[&scene_object_id].mesh.triangles,
            scene_object_id,
            FeatureEdgeSurface::Source,
            policy.minimum_feature_angle_radians,
        )?;
        let boundary = feature_edges(
            &mesh.points,
            &body_boundary[&scene_object_id],
            scene_object_id,
            FeatureEdgeSurface::Boundary,
            policy.minimum_feature_angle_radians,
        )?;
        if source.is_empty() != boundary.is_empty() {
            return Err(SourceBoundaryFeatureEdgeError::FeaturePresenceMismatch {
                scene_object_id,
                source_feature_edges: source.len(),
                boundary_feature_edges: boundary.len(),
            });
        }
        let one_direction = source
            .len()
            .checked_mul(boundary.len())
            .ok_or(SourceBoundaryFeatureEdgeError::ComparisonBudgetOverflow)?;
        requested_tests = requested_tests
            .checked_add(
                one_direction
                    .checked_mul(2)
                    .ok_or(SourceBoundaryFeatureEdgeError::ComparisonBudgetOverflow)?,
            )
            .ok_or(SourceBoundaryFeatureEdgeError::ComparisonBudgetOverflow)?;
        prepared.push(PreparedBodyFeatureComparison { scene_object_id, source, boundary });
    }
    if requested_tests > policy.max_edge_pair_tests {
        return Err(SourceBoundaryFeatureEdgeError::ComparisonBudgetExceeded {
            requested: requested_tests,
            limit: policy.max_edge_pair_tests,
        });
    }

    let mut bodies = Vec::with_capacity(prepared.len());
    for body in prepared {
        let forward = compare_direction(
            body.scene_object_id,
            FeatureEdgeComparisonDirection::SourceToBoundary,
            &body.source,
            &body.boundary,
            policy,
        )?;
        let reverse = compare_direction(
            body.scene_object_id,
            FeatureEdgeComparisonDirection::BoundaryToSource,
            &body.boundary,
            &body.source,
            policy,
        )?;
        bodies.push(SourceBoundaryFeatureEdgeBodyReport {
            scene_object_id: body.scene_object_id,
            source_feature_edge_count: body.source.len(),
            boundary_feature_edge_count: body.boundary.len(),
            max_source_to_boundary_midpoint_distance: forward.0,
            max_boundary_to_source_midpoint_distance: reverse.0,
            min_source_to_boundary_direction_alignment_cosine: forward.1,
            min_boundary_to_source_direction_alignment_cosine: reverse.1,
            max_source_to_boundary_dihedral_angle_difference_radians: forward.2,
            max_boundary_to_source_dihedral_angle_difference_radians: reverse.2,
        });
    }
    Ok(SourceBoundaryFeatureEdgeReport { bodies, edge_pair_tests: requested_tests })
}

fn validate_policy(policy: SourceBoundaryFeatureEdgePolicy) -> Result<(), SourceBoundaryFeatureEdgeError> {
    for (field, value, valid) in [
        (
            "minimum_feature_angle_radians",
            policy.minimum_feature_angle_radians,
            policy.minimum_feature_angle_radians.is_finite()
                && policy.minimum_feature_angle_radians > 0.0
                && policy.minimum_feature_angle_radians <= PI,
        ),
        (
            "distance_tolerance",
            policy.distance_tolerance,
            policy.distance_tolerance.is_finite() && policy.distance_tolerance >= 0.0,
        ),
        (
            "minimum_direction_alignment_cosine",
            policy.minimum_direction_alignment_cosine,
            policy.minimum_direction_alignment_cosine.is_finite()
                && (0.0..=1.0).contains(&policy.minimum_direction_alignment_cosine),
        ),
        (
            "maximum_dihedral_angle_difference_radians",
            policy.maximum_dihedral_angle_difference_radians,
            policy.maximum_dihedral_angle_difference_radians.is_finite()
                && (0.0..=PI).contains(&policy.maximum_dihedral_angle_difference_radians),
        ),
    ] {
        if !valid {
            return Err(SourceBoundaryFeatureEdgeError::InvalidPolicy { field, value });
        }
    }
    if policy.max_edge_pair_tests == 0 {
        return Err(SourceBoundaryFeatureEdgeError::ZeroComparisonBudget);
    }
    Ok(())
}

fn validate_source_still_audited(
    source: &AuditedImportedSurfaceBody,
) -> Result<(), SourceBoundaryFeatureEdgeError> {
    let topology = source.mesh.topology_report().map_err(|error| {
        SourceBoundaryFeatureEdgeError::SourceSurfaceAuditInvalid {
            scene_object_id: source.scene_object_id,
            message: error.to_string(),
        }
    })?;
    let positive_volume = topology
        .signed_volume
        .is_some_and(|value| value.is_finite() && value > 0.0);
    if !topology.watertight_two_manifold
        || !topology.consistently_oriented
        || topology.connected_components != 1
        || !positive_volume
    {
        return Err(SourceBoundaryFeatureEdgeError::SourceSurfaceAuditInvalid {
            scene_object_id: source.scene_object_id,
            message: format!(
                "watertight={}, oriented={}, components={}, signed_volume={:?}",
                topology.watertight_two_manifold,
                topology.consistently_oriented,
                topology.connected_components,
                topology.signed_volume
            ),
        });
    }
    Ok(())
}

fn feature_edges(
    points: &[[f64; 3]],
    triangles: &[[u32; 3]],
    scene_object_id: u64,
    surface: FeatureEdgeSurface,
    minimum_feature_angle_radians: f64,
) -> Result<Vec<FeatureEdge>, SourceBoundaryFeatureEdgeError> {
    let normals = triangles
        .iter()
        .enumerate()
        .map(|(triangle, &vertices)| {
            unit_normal([
                points[vertices[0] as usize],
                points[vertices[1] as usize],
                points[vertices[2] as usize],
            ])
            .ok_or(SourceBoundaryFeatureEdgeError::DegenerateTriangle {
                scene_object_id,
                surface,
                triangle,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut edge_uses = BTreeMap::<(u32, u32), Vec<usize>>::new();
    for (triangle_index, &triangle) in triangles.iter().enumerate() {
        for [a, b] in [
            [triangle[0], triangle[1]],
            [triangle[1], triangle[2]],
            [triangle[2], triangle[0]],
        ] {
            let key = if a < b { (a, b) } else { (b, a) };
            edge_uses.entry(key).or_default().push(triangle_index);
        }
    }

    let mut output = Vec::new();
    for ((a, b), uses) in edge_uses {
        if uses.len() != 2 {
            return Err(SourceBoundaryFeatureEdgeError::InvalidEdgeUse {
                scene_object_id,
                surface,
                edge: [a, b],
                uses: uses.len(),
            });
        }
        let dihedral_angle_radians = dot(normals[uses[0]], normals[uses[1]])
            .clamp(-1.0, 1.0)
            .acos();
        if dihedral_angle_radians < minimum_feature_angle_radians {
            continue;
        }
        let edge_points = [points[a as usize], points[b as usize]];
        let edge_vector = sub(edge_points[1], edge_points[0]);
        let length_squared = dot(edge_vector, edge_vector);
        if !length_squared.is_finite() || length_squared <= 0.0 {
            return Err(SourceBoundaryFeatureEdgeError::DegenerateEdge {
                scene_object_id,
                surface,
                edge: [a, b],
            });
        }
        let unit_direction = scale(edge_vector, length_squared.sqrt().recip());
        output.push(FeatureEdge {
            points: edge_points,
            midpoint: scale(add(edge_points[0], edge_points[1]), 0.5),
            unit_direction,
            dihedral_angle_radians,
        });
    }
    Ok(output)
}

fn compare_direction(
    scene_object_id: u64,
    direction: FeatureEdgeComparisonDirection,
    source: &[FeatureEdge],
    target: &[FeatureEdge],
    policy: SourceBoundaryFeatureEdgePolicy,
) -> Result<(f64, f64, f64), SourceBoundaryFeatureEdgeError> {
    if source.is_empty() {
        return Ok((0.0, 1.0, 0.0));
    }
    let mut max_distance = 0.0_f64;
    let mut min_alignment = 1.0_f64;
    let mut max_dihedral_difference = 0.0_f64;
    for (edge_index, edge) in source.iter().enumerate() {
        let (nearest_index, distance_squared) = target.iter().enumerate().fold(
            (0_usize, f64::INFINITY),
            |best, (index, candidate)| {
                let distance = point_segment_distance_squared(edge.midpoint, candidate.points);
                if distance < best.1 { (index, distance) } else { best }
            },
        );
        let distance = distance_squared.sqrt();
        if !distance.is_finite() || distance > policy.distance_tolerance {
            return Err(SourceBoundaryFeatureEdgeError::DistanceToleranceExceeded {
                scene_object_id,
                direction,
                edge: edge_index,
                distance,
                tolerance: policy.distance_tolerance,
            });
        }
        max_distance = max_distance.max(distance);
        let target_edge = target[nearest_index];
        let alignment = dot(edge.unit_direction, target_edge.unit_direction)
            .abs()
            .clamp(0.0, 1.0);
        if alignment < policy.minimum_direction_alignment_cosine {
            return Err(SourceBoundaryFeatureEdgeError::DirectionAlignmentBelowThreshold {
                scene_object_id,
                direction,
                edge: edge_index,
                alignment_cosine: alignment,
                minimum: policy.minimum_direction_alignment_cosine,
            });
        }
        min_alignment = min_alignment.min(alignment);
        let difference = (edge.dihedral_angle_radians - target_edge.dihedral_angle_radians).abs();
        if !difference.is_finite()
            || difference > policy.maximum_dihedral_angle_difference_radians
        {
            return Err(SourceBoundaryFeatureEdgeError::DihedralAngleDifferenceExceeded {
                scene_object_id,
                direction,
                edge: edge_index,
                difference_radians: difference,
                maximum_radians: policy.maximum_dihedral_angle_difference_radians,
            });
        }
        max_dihedral_difference = max_dihedral_difference.max(difference);
    }
    Ok((max_distance, min_alignment, max_dihedral_difference))
}

fn unit_normal(triangle: [[f64; 3]; 3]) -> Option<[f64; 3]> {
    let normal = cross(sub(triangle[1], triangle[0]), sub(triangle[2], triangle[0]));
    let length_squared = dot(normal, normal);
    if !length_squared.is_finite() || length_squared <= 0.0 {
        return None;
    }
    let unit = scale(normal, length_squared.sqrt().recip());
    unit.iter().all(|value| value.is_finite()).then_some(unit)
}

fn point_segment_distance_squared(point: [f64; 3], segment: [[f64; 3]; 2]) -> f64 {
    let edge = sub(segment[1], segment[0]);
    let length_squared = dot(edge, edge);
    if !length_squared.is_finite() || length_squared <= 0.0 {
        return f64::INFINITY;
    }
    let t = (dot(sub(point, segment[0]), edge) / length_squared).clamp(0.0, 1.0);
    let delta = sub(point, add(segment[0], scale(edge, t)));
    dot(delta, delta)
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: [f64; 3], factor: f64) -> [f64; 3] {
    [a[0] * factor, a[1] * factor, a[2] * factor]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeroforge_geometry_core::SurfaceMesh;
    use aeroforge_volume_core::{BlockBoundaryMarkers, BoundaryMarkerId};

    use crate::imported_surface::{
        audit_imported_surface_for_accurate_meshing, AccurateImportedSurfacePolicy,
    };
    use crate::scene_provenance::build_scene_owner_marker_provenance;
    use crate::su2_mesh::{BoundaryRole, DomainAxis, DomainSide, Su2MarkerBinding};
    use crate::voxel_mesh::{tetrahedralize_voxel_fluid_domain, VoxelFluidDomainSpec};

    fn domain() -> VoxelFluidDomainSpec {
        VoxelFluidDomainSpec {
            min: [0.0, 0.0, 0.0],
            max: [3.0, 3.0, 3.0],
            cells: [3, 3, 3],
            outer_markers: BlockBoundaryMarkers {
                x_min: BoundaryMarkerId(1), x_max: BoundaryMarkerId(2),
                y_min: BoundaryMarkerId(3), y_max: BoundaryMarkerId(4),
                z_min: BoundaryMarkerId(5), z_max: BoundaryMarkerId(6),
            },
        }
    }

    fn domain_bindings() -> Vec<Su2MarkerBinding> {
        let binding = |marker, tag: &str, role, axis, side| Su2MarkerBinding {
            marker: BoundaryMarkerId(marker), tag: tag.into(), role,
            source: BoundarySource::DomainFace { axis, side },
        };
        vec![
            binding(1, "inlet", BoundaryRole::Inlet, DomainAxis::X, DomainSide::Min),
            binding(2, "outlet", BoundaryRole::Outlet, DomainAxis::X, DomainSide::Max),
            binding(3, "y_min", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Min),
            binding(4, "y_max", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Max),
            binding(5, "z_min", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Min),
            binding(6, "z_max", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Max),
        ]
    }

    fn cube_surface() -> SurfaceMesh {
        let (x0, y0, z0) = (1.0, 1.0, 1.0);
        let (x1, y1, z1) = (2.0, 2.0, 2.0);
        SurfaceMesh {
            positions: vec![
                [x0,y0,z0],[x1,y0,z0],[x1,y1,z0],[x0,y1,z0],
                [x0,y0,z1],[x1,y0,z1],[x1,y1,z1],[x0,y1,z1],
            ],
            triangles: vec![
                [0,2,1],[0,3,2],[4,5,6],[4,6,7],
                [0,1,5],[0,5,4],[3,7,6],[3,6,2],
                [0,4,7],[0,7,3],[1,2,6],[1,6,5],
            ],
        }
    }

    fn fixture(source_surface: SurfaceMesh) -> (VolumeMesh, Su2MarkerMap, AuditedImportedSurfaceBody) {
        let provenance = build_scene_owner_marker_provenance(&[42], domain_bindings()).unwrap();
        let mut solid_owner = vec![0_u32; 27];
        solid_owner[(1 * 3 + 1) * 3 + 1] = 1;
        let mesh = tetrahedralize_voxel_fluid_domain(domain(), &solid_owner, &provenance.owner_markers).unwrap();
        let source = audit_imported_surface_for_accurate_meshing(
            42, &source_surface, AccurateImportedSurfacePolicy::default(),
        ).unwrap();
        (mesh, provenance.marker_map, source)
    }

    fn policy() -> SourceBoundaryFeatureEdgePolicy {
        SourceBoundaryFeatureEdgePolicy {
            minimum_feature_angle_radians: 0.5,
            distance_tolerance: 1.0e-10,
            minimum_direction_alignment_cosine: 0.999_999,
            maximum_dihedral_angle_difference_radians: 1.0e-10,
            max_edge_pair_tests: 10_000,
        }
    }

    #[test]
    fn exact_cube_matches_twelve_true_crease_edges_bidirectionally() {
        let (mut mesh, marker_map, source) = fixture(cube_surface());
        for boundary in &mut mesh.boundary { boundary.vertices.swap(1, 2); }
        mesh.audit().unwrap();
        let report = validate_source_boundary_feature_edges(&mesh, &marker_map, &[source], policy()).unwrap();
        let body = &report.bodies[0];
        assert_eq!(body.scene_object_id, 42);
        assert_eq!(body.source_feature_edge_count, 12);
        assert_eq!(body.boundary_feature_edge_count, 12);
        assert_eq!(report.edge_pair_tests, 288);
        assert!(body.max_source_to_boundary_midpoint_distance <= 1.0e-12);
        assert!(body.max_boundary_to_source_midpoint_distance <= 1.0e-12);
        assert!(body.min_source_to_boundary_direction_alignment_cosine > 0.999_999_999);
        assert!(body.min_boundary_to_source_direction_alignment_cosine > 0.999_999_999);
        assert!(body.max_source_to_boundary_dihedral_angle_difference_radians <= 1.0e-12);
        assert!(body.max_boundary_to_source_dihedral_angle_difference_radians <= 1.0e-12);
    }

    #[test]
    fn coplanar_face_diagonals_are_not_features() {
        let (mesh, marker_map, source) = fixture(cube_surface());
        let report = validate_source_boundary_feature_edges(&mesh, &marker_map, &[source], policy()).unwrap();
        assert_eq!(report.bodies[0].source_feature_edge_count, 12);
        assert_eq!(report.bodies[0].boundary_feature_edge_count, 12);
    }

    #[test]
    fn displaced_source_crease_fails_local_distance_contract() {
        let mut displaced = cube_surface();
        for position in &mut displaced.positions { position[0] += 0.01; }
        let (mesh, marker_map, source) = fixture(displaced);
        let error = validate_source_boundary_feature_edges(
            &mesh, &marker_map, &[source],
            SourceBoundaryFeatureEdgePolicy { distance_tolerance: 1.0e-4, ..policy() },
        ).unwrap_err();
        assert!(matches!(
            error,
            SourceBoundaryFeatureEdgeError::DistanceToleranceExceeded { scene_object_id: 42, .. }
        ));
    }

    #[test]
    fn edge_pair_budget_is_preflighted_before_feature_comparison() {
        let (mesh, marker_map, source) = fixture(cube_surface());
        let error = validate_source_boundary_feature_edges(
            &mesh, &marker_map, &[source],
            SourceBoundaryFeatureEdgePolicy { max_edge_pair_tests: 287, ..policy() },
        ).unwrap_err();
        assert_eq!(
            error,
            SourceBoundaryFeatureEdgeError::ComparisonBudgetExceeded { requested: 288, limit: 287 }
        );
    }
}
