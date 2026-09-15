use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::{BoundaryMarkerId, VolumeMesh};

use crate::exterior_mesh::{
    validate_declared_exterior_fluid_mesh_input, DeclaredExteriorFluidMeshError,
};
use crate::imported_surface::AuditedImportedSurfaceBody;
use crate::su2_mesh::{BoundarySource, Su2MarkerMap};

/// Bounded one-to-one triangle-facet correspondence policy for audited source surfaces and
/// SceneObject body boundaries.
///
/// Two triangles correspond when there is a permutation of their three vertices whose maximum
/// Euclidean vertex distance is no greater than `vertex_distance_tolerance`. Winding and starting
/// vertex are deliberately ignored. Every source/body-boundary triangle pair is tested and every
/// triangle on both sides must participate in exactly one match.
///
/// This is a triangulated constrained-facet correspondence contract. It does not establish
/// analytic/CAD semantics, continuous-curvature preservation, boundary-layer quality, engineering
/// mesh quality, or aerodynamic accuracy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceBoundaryFacetCorrespondencePolicy {
    pub vertex_distance_tolerance: f64,
    pub max_triangle_pair_tests: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceBoundaryFacetCorrespondenceBodyReport {
    pub scene_object_id: u64,
    pub source_triangle_count: usize,
    pub boundary_triangle_count: usize,
    pub matched_triangle_count: usize,
    pub maximum_matched_vertex_distance: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceBoundaryFacetCorrespondenceReport {
    pub bodies: Vec<SourceBoundaryFacetCorrespondenceBodyReport>,
    pub triangle_pair_tests: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FacetCorrespondenceSurface {
    Source,
    Boundary,
}

impl Display for FacetCorrespondenceSurface {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source => write!(f, "source"),
            Self::Boundary => write!(f, "body-boundary"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SourceBoundaryFacetCorrespondenceError {
    Exterior(DeclaredExteriorFluidMeshError),
    InvalidVertexDistanceTolerance { value: f64 },
    ZeroComparisonBudget,
    DuplicateSourceSceneObject { scene_object_id: u64 },
    MissingSourceForBoundary { scene_object_id: u64 },
    MissingBoundaryForSource { scene_object_id: u64 },
    SourceSurfaceAuditInvalid { scene_object_id: u64, message: String },
    TriangleCountMismatch {
        scene_object_id: u64,
        source_triangles: usize,
        boundary_triangles: usize,
    },
    ComparisonBudgetOverflow,
    ComparisonBudgetExceeded { requested: usize, limit: usize },
    NonUniqueTriangleMatch {
        scene_object_id: u64,
        surface: FacetCorrespondenceSurface,
        triangle: usize,
        matches: usize,
    },
}

impl Display for SourceBoundaryFacetCorrespondenceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exterior(error) => write!(f, "exterior-fluid input contract failed: {error}"),
            Self::InvalidVertexDistanceTolerance { value } => write!(
                f,
                "source/body facet vertex-distance tolerance must be finite and non-negative; got {value}"
            ),
            Self::ZeroComparisonBudget => write!(
                f,
                "source/body facet correspondence requires a non-zero triangle-pair budget"
            ),
            Self::DuplicateSourceSceneObject { scene_object_id } => write!(
                f,
                "source/body facet correspondence received duplicate source SceneObject {scene_object_id}"
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
            Self::TriangleCountMismatch {
                scene_object_id,
                source_triangles,
                boundary_triangles,
            } => write!(
                f,
                "SceneObject {scene_object_id} constrained-facet triangle counts differ: source={source_triangles}, boundary={boundary_triangles}"
            ),
            Self::ComparisonBudgetOverflow => write!(
                f,
                "source/body constrained-facet triangle-pair work overflowed usize"
            ),
            Self::ComparisonBudgetExceeded { requested, limit } => write!(
                f,
                "source/body constrained-facet correspondence requires {requested} triangle-pair tests, above explicit limit {limit}"
            ),
            Self::NonUniqueTriangleMatch {
                scene_object_id,
                surface,
                triangle,
                matches,
            } => write!(
                f,
                "SceneObject {scene_object_id} {surface} triangle {triangle} has {matches} opposite constrained-facet matches; expected exactly one"
            ),
        }
    }
}

impl Error for SourceBoundaryFacetCorrespondenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Exterior(error) => Some(error),
            _ => None,
        }
    }
}

impl From<DeclaredExteriorFluidMeshError> for SourceBoundaryFacetCorrespondenceError {
    fn from(value: DeclaredExteriorFluidMeshError) -> Self {
        Self::Exterior(value)
    }
}

struct PreparedBodyFacetComparison<'a> {
    scene_object_id: u64,
    source_points: &'a [[f64; 3]],
    source_triangles: &'a [[u32; 3]],
    boundary_triangles: Vec<[u32; 3]>,
}

/// Validates one-to-one constrained triangle-facet correspondence for every SceneObject body.
///
/// Marker ownership comes only from `BoundarySource::SceneObject`. Source topology is revalidated
/// before comparison. Source and body-boundary triangle counts must be identical. The complete
/// source×boundary triangle-pair work is reserved before any matching. Each pair is then compared
/// under all six vertex permutations; both source and boundary triangles must have exactly one
/// opposite match within the caller-selected vertex-distance tolerance.
///
/// Passing establishes one-to-one triangulated facet coincidence within that explicit tolerance.
/// It does not prove bitwise coordinate identity when the tolerance is non-zero, analytic/CAD
/// feature semantics, continuous curvature, boundary-layer suitability, engineering mesh quality,
/// or CFD accuracy.
pub fn validate_source_boundary_facet_correspondence(
    mesh: &VolumeMesh,
    marker_map: &Su2MarkerMap,
    audited_sources: &[AuditedImportedSurfaceBody],
    policy: SourceBoundaryFacetCorrespondencePolicy,
) -> Result<SourceBoundaryFacetCorrespondenceReport, SourceBoundaryFacetCorrespondenceError> {
    if !policy.vertex_distance_tolerance.is_finite() || policy.vertex_distance_tolerance < 0.0 {
        return Err(
            SourceBoundaryFacetCorrespondenceError::InvalidVertexDistanceTolerance {
                value: policy.vertex_distance_tolerance,
            },
        );
    }
    if policy.max_triangle_pair_tests == 0 {
        return Err(SourceBoundaryFacetCorrespondenceError::ZeroComparisonBudget);
    }

    let exterior = validate_declared_exterior_fluid_mesh_input(mesh, marker_map)?;

    let mut sources = BTreeMap::<u64, &AuditedImportedSurfaceBody>::new();
    for source in audited_sources {
        if sources.insert(source.scene_object_id, source).is_some() {
            return Err(
                SourceBoundaryFacetCorrespondenceError::DuplicateSourceSceneObject {
                    scene_object_id: source.scene_object_id,
                },
            );
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
    for face in &mesh.boundary {
        if let Some(&scene_object_id) = marker_scene_ids.get(&face.marker) {
            body_boundary
                .entry(scene_object_id)
                .or_default()
                .push(face.vertices);
        }
    }

    for &scene_object_id in &exterior.scene_object_ids {
        if !sources.contains_key(&scene_object_id) {
            return Err(
                SourceBoundaryFacetCorrespondenceError::MissingSourceForBoundary {
                    scene_object_id,
                },
            );
        }
    }
    for &scene_object_id in sources.keys() {
        if !body_boundary.contains_key(&scene_object_id) {
            return Err(
                SourceBoundaryFacetCorrespondenceError::MissingBoundaryForSource {
                    scene_object_id,
                },
            );
        }
    }

    let mut prepared = Vec::with_capacity(exterior.scene_object_ids.len());
    let mut requested_tests = 0_usize;
    for &scene_object_id in &exterior.scene_object_ids {
        let source = sources[&scene_object_id];
        let boundary_triangles = body_boundary
            .remove(&scene_object_id)
            .expect("body boundary presence checked above");
        if source.mesh.triangles.len() != boundary_triangles.len() {
            return Err(SourceBoundaryFacetCorrespondenceError::TriangleCountMismatch {
                scene_object_id,
                source_triangles: source.mesh.triangles.len(),
                boundary_triangles: boundary_triangles.len(),
            });
        }
        let body_tests = source
            .mesh
            .triangles
            .len()
            .checked_mul(boundary_triangles.len())
            .ok_or(SourceBoundaryFacetCorrespondenceError::ComparisonBudgetOverflow)?;
        requested_tests = requested_tests
            .checked_add(body_tests)
            .ok_or(SourceBoundaryFacetCorrespondenceError::ComparisonBudgetOverflow)?;
        prepared.push(PreparedBodyFacetComparison {
            scene_object_id,
            source_points: &source.mesh.positions,
            source_triangles: &source.mesh.triangles,
            boundary_triangles,
        });
    }
    if requested_tests > policy.max_triangle_pair_tests {
        return Err(SourceBoundaryFacetCorrespondenceError::ComparisonBudgetExceeded {
            requested: requested_tests,
            limit: policy.max_triangle_pair_tests,
        });
    }

    let mut bodies = Vec::with_capacity(prepared.len());
    for body in prepared {
        let mut source_matches = vec![0_usize; body.source_triangles.len()];
        let mut boundary_matches = vec![0_usize; body.boundary_triangles.len()];
        let mut maximum_matched_vertex_distance = 0.0_f64;

        for (source_index, &source_triangle) in body.source_triangles.iter().enumerate() {
            let source_points = triangle_points(body.source_points, source_triangle);
            for (boundary_index, &boundary_triangle) in
                body.boundary_triangles.iter().enumerate()
            {
                let boundary_points = triangle_points(&mesh.points, boundary_triangle);
                let distance = best_vertex_bijection_max_distance(source_points, boundary_points);
                if distance <= policy.vertex_distance_tolerance {
                    source_matches[source_index] += 1;
                    boundary_matches[boundary_index] += 1;
                    maximum_matched_vertex_distance =
                        maximum_matched_vertex_distance.max(distance);
                }
            }
        }

        for (triangle, &matches) in source_matches.iter().enumerate() {
            if matches != 1 {
                return Err(SourceBoundaryFacetCorrespondenceError::NonUniqueTriangleMatch {
                    scene_object_id: body.scene_object_id,
                    surface: FacetCorrespondenceSurface::Source,
                    triangle,
                    matches,
                });
            }
        }
        for (triangle, &matches) in boundary_matches.iter().enumerate() {
            if matches != 1 {
                return Err(SourceBoundaryFacetCorrespondenceError::NonUniqueTriangleMatch {
                    scene_object_id: body.scene_object_id,
                    surface: FacetCorrespondenceSurface::Boundary,
                    triangle,
                    matches,
                });
            }
        }

        bodies.push(SourceBoundaryFacetCorrespondenceBodyReport {
            scene_object_id: body.scene_object_id,
            source_triangle_count: body.source_triangles.len(),
            boundary_triangle_count: body.boundary_triangles.len(),
            matched_triangle_count: body.source_triangles.len(),
            maximum_matched_vertex_distance,
        });
    }

    Ok(SourceBoundaryFacetCorrespondenceReport {
        bodies,
        triangle_pair_tests: requested_tests,
    })
}

fn validate_source_still_audited(
    source: &AuditedImportedSurfaceBody,
) -> Result<(), SourceBoundaryFacetCorrespondenceError> {
    let topology = source.mesh.topology_report().map_err(|error| {
        SourceBoundaryFacetCorrespondenceError::SourceSurfaceAuditInvalid {
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
        return Err(SourceBoundaryFacetCorrespondenceError::SourceSurfaceAuditInvalid {
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

fn triangle_points(points: &[[f64; 3]], triangle: [u32; 3]) -> [[f64; 3]; 3] {
    [
        points[triangle[0] as usize],
        points[triangle[1] as usize],
        points[triangle[2] as usize],
    ]
}

fn best_vertex_bijection_max_distance(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> f64 {
    const PERMUTATIONS: [[usize; 3]; 6] = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    PERMUTATIONS
        .iter()
        .map(|permutation| {
            (0..3)
                .map(|index| distance(a[index], b[permutation[index]]))
                .fold(0.0_f64, f64::max)
        })
        .fold(f64::INFINITY, f64::min)
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
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

    fn cube_surface(min: [f64; 3], max: [f64; 3]) -> SurfaceMesh {
        let [x0, y0, z0] = min;
        let [x1, y1, z1] = max;
        SurfaceMesh {
            positions: vec![
                [x0, y0, z0],
                [x1, y0, z0],
                [x1, y1, z0],
                [x0, y1, z0],
                [x0, y0, z1],
                [x1, y0, z1],
                [x1, y1, z1],
                [x0, y1, z1],
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

    fn domain_bindings() -> Vec<Su2MarkerBinding> {
        let binding = |marker, tag: &str, role, axis, side| Su2MarkerBinding {
            marker: BoundaryMarkerId(marker),
            tag: tag.into(),
            role,
            source: BoundarySource::DomainFace { axis, side },
        };
        vec![
            binding(1, "x_min", BoundaryRole::Inlet, DomainAxis::X, DomainSide::Min),
            binding(2, "x_max", BoundaryRole::Outlet, DomainAxis::X, DomainSide::Max),
            binding(3, "y_min", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Min),
            binding(4, "y_max", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Max),
            binding(5, "z_min", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Min),
            binding(6, "z_max", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Max),
        ]
    }

    fn fixture() -> (VolumeMesh, Su2MarkerMap, AuditedImportedSurfaceBody) {
        let audited = audit_imported_surface_for_accurate_meshing(
            42,
            &cube_surface([1.0, 1.0, 1.0], [2.0, 2.0, 2.0]),
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap();
        let provenance = build_scene_owner_marker_provenance(&[42], domain_bindings()).unwrap();
        let mut solid_owner = vec![0_u32; 27];
        solid_owner[(1 * 3 + 1) * 3 + 1] = 1;
        let mesh = tetrahedralize_voxel_fluid_domain(
            VoxelFluidDomainSpec {
                min: [0.0, 0.0, 0.0],
                max: [3.0, 3.0, 3.0],
                cells: [3, 3, 3],
                outer_markers: BlockBoundaryMarkers {
                    x_min: BoundaryMarkerId(1),
                    x_max: BoundaryMarkerId(2),
                    y_min: BoundaryMarkerId(3),
                    y_max: BoundaryMarkerId(4),
                    z_min: BoundaryMarkerId(5),
                    z_max: BoundaryMarkerId(6),
                },
            },
            &solid_owner,
            &provenance.owner_markers,
        )
        .unwrap();
        (mesh, provenance.marker_map, audited)
    }

    fn policy() -> SourceBoundaryFacetCorrespondencePolicy {
        SourceBoundaryFacetCorrespondencePolicy {
            vertex_distance_tolerance: 0.0,
            max_triangle_pair_tests: 144,
        }
    }

    #[test]
    fn voxel_aligned_cube_has_one_to_one_zero_distance_facets() {
        let (mesh, marker_map, audited) = fixture();
        let report = validate_source_boundary_facet_correspondence(
            &mesh,
            &marker_map,
            &[audited],
            policy(),
        )
        .unwrap();

        assert_eq!(report.triangle_pair_tests, 144);
        assert_eq!(report.bodies.len(), 1);
        assert_eq!(report.bodies[0].scene_object_id, 42);
        assert_eq!(report.bodies[0].source_triangle_count, 12);
        assert_eq!(report.bodies[0].boundary_triangle_count, 12);
        assert_eq!(report.bodies[0].matched_triangle_count, 12);
        assert_eq!(report.bodies[0].maximum_matched_vertex_distance, 0.0);
    }

    #[test]
    fn shifted_source_fails_unique_match() {
        let (mesh, marker_map, _) = fixture();
        let shifted = audit_imported_surface_for_accurate_meshing(
            42,
            &cube_surface([1.01, 1.0, 1.0], [2.01, 2.0, 2.0]),
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap();
        let error = validate_source_boundary_facet_correspondence(
            &mesh,
            &marker_map,
            &[shifted],
            policy(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            SourceBoundaryFacetCorrespondenceError::NonUniqueTriangleMatch {
                surface: FacetCorrespondenceSurface::Source,
                matches: 0,
                ..
            }
        ));
    }

    #[test]
    fn complete_pair_budget_fails_closed_before_matching() {
        let (mesh, marker_map, audited) = fixture();
        let error = validate_source_boundary_facet_correspondence(
            &mesh,
            &marker_map,
            &[audited],
            SourceBoundaryFacetCorrespondencePolicy {
                max_triangle_pair_tests: 143,
                ..policy()
            },
        )
        .unwrap_err();
        assert_eq!(
            error,
            SourceBoundaryFacetCorrespondenceError::ComparisonBudgetExceeded {
                requested: 144,
                limit: 143,
            }
        );
    }

    #[test]
    fn winding_and_starting_vertex_do_not_change_correspondence() {
        let a = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let b = [a[2], a[0], a[1]];
        assert_eq!(best_vertex_bijection_max_distance(a, b), 0.0);
    }
}
