use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_geometry_core::SurfaceMesh;
use aeroforge_volume_core::{BoundaryMarkerId, VolumeMesh};

use crate::exterior_mesh::{
    validate_declared_exterior_fluid_mesh_input, DeclaredExteriorFluidMeshError,
};
use crate::imported_surface::AuditedImportedSurfaceBody;
use crate::su2_mesh::{BoundarySource, Su2MarkerMap};

/// Bounded geometric comparison policy for a future source-surface -> exterior-volume handoff.
///
/// `distance_tolerance` is expressed in the same coordinate units as the source and volume mesh.
/// AeroForge deliberately does not infer units or silently downsample when the comparison budget
/// would be exceeded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceSurfaceCorrespondencePolicy {
    pub distance_tolerance: f64,
    pub max_point_triangle_tests: usize,
}

impl Default for SourceSurfaceCorrespondencePolicy {
    fn default() -> Self {
        Self {
            distance_tolerance: 1.0e-6,
            max_point_triangle_tests: 2_000_000,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceSurfaceBodyCorrespondence {
    pub scene_object_id: u64,
    pub source_triangle_count: usize,
    pub boundary_triangle_count: usize,
    pub source_sample_count: usize,
    pub boundary_sample_count: usize,
    pub max_source_to_boundary_distance: f64,
    pub max_boundary_to_source_distance: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceSurfaceCorrespondenceReport {
    pub bodies: Vec<SourceSurfaceBodyCorrespondence>,
    pub point_triangle_tests: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SourceSurfaceCorrespondenceError {
    Exterior(DeclaredExteriorFluidMeshError),
    InvalidDistanceTolerance { value: f64 },
    ZeroComparisonBudget,
    DuplicateSourceSceneObject { scene_object_id: u64 },
    MissingSourceForBoundary { scene_object_id: u64 },
    MissingBoundaryForSource { scene_object_id: u64 },
    SourceSurfaceAuditInvalid {
        scene_object_id: u64,
        message: String,
    },
    ComparisonBudgetOverflow,
    ComparisonBudgetExceeded {
        requested: usize,
        limit: usize,
    },
    DistanceToleranceExceeded {
        scene_object_id: u64,
        max_source_to_boundary_distance: f64,
        max_boundary_to_source_distance: f64,
        tolerance: f64,
    },
}

impl Display for SourceSurfaceCorrespondenceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exterior(error) => write!(f, "exterior-fluid input contract failed: {error}"),
            Self::InvalidDistanceTolerance { value } => write!(
                f,
                "source-surface correspondence distance tolerance must be finite and non-negative; got {value}"
            ),
            Self::ZeroComparisonBudget => write!(
                f,
                "source-surface correspondence requires a non-zero point/triangle comparison budget"
            ),
            Self::DuplicateSourceSceneObject { scene_object_id } => write!(
                f,
                "source-surface correspondence received duplicate audited sources for SceneObject {scene_object_id}"
            ),
            Self::MissingSourceForBoundary { scene_object_id } => write!(
                f,
                "exterior-fluid boundary for SceneObject {scene_object_id} has no audited source surface"
            ),
            Self::MissingBoundaryForSource { scene_object_id } => write!(
                f,
                "audited source surface for SceneObject {scene_object_id} has no exterior-fluid body boundary"
            ),
            Self::SourceSurfaceAuditInvalid {
                scene_object_id,
                message,
            } => write!(
                f,
                "source surface for SceneObject {scene_object_id} no longer satisfies the accurate imported-surface audit: {message}"
            ),
            Self::ComparisonBudgetOverflow => write!(
                f,
                "source-surface correspondence comparison count overflowed usize"
            ),
            Self::ComparisonBudgetExceeded { requested, limit } => write!(
                f,
                "source-surface correspondence requires {requested} point/triangle tests, above the explicit limit {limit}"
            ),
            Self::DistanceToleranceExceeded {
                scene_object_id,
                max_source_to_boundary_distance,
                max_boundary_to_source_distance,
                tolerance,
            } => write!(
                f,
                "SceneObject {scene_object_id} source/boundary correspondence exceeds tolerance {tolerance}: source->boundary max {max_source_to_boundary_distance}, boundary->source max {max_boundary_to_source_distance}"
            ),
        }
    }
}

impl Error for SourceSurfaceCorrespondenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Exterior(error) => Some(error),
            _ => None,
        }
    }
}

impl From<DeclaredExteriorFluidMeshError> for SourceSurfaceCorrespondenceError {
    fn from(value: DeclaredExteriorFluidMeshError) -> Self {
        Self::Exterior(value)
    }
}

/// Validates bounded, bidirectional geometric proximity between audited source surfaces and
/// SceneObject body boundaries in an already-declared exterior-fluid `VolumeMesh`.
///
/// Every source/body triangle vertex and triangle centroid is checked against the opposite triangle
/// surface. No random/downsampled subset is used. If the explicit pair-test budget would be
/// exceeded, validation fails before the expensive comparison starts.
///
/// Passing this contract proves only bounded bidirectional sample-to-surface proximity under the
/// supplied tolerance. It does **not** prove exact triangle-to-triangle coincidence, normal/feature
/// preservation, non-overlapping tetrahedra, body-fitted meshing, or engineering-quality CFD.
pub fn validate_source_surface_correspondence(
    mesh: &VolumeMesh,
    marker_map: &Su2MarkerMap,
    audited_sources: &[AuditedImportedSurfaceBody],
    policy: SourceSurfaceCorrespondencePolicy,
) -> Result<SourceSurfaceCorrespondenceReport, SourceSurfaceCorrespondenceError> {
    if !policy.distance_tolerance.is_finite() || policy.distance_tolerance < 0.0 {
        return Err(SourceSurfaceCorrespondenceError::InvalidDistanceTolerance {
            value: policy.distance_tolerance,
        });
    }
    if policy.max_point_triangle_tests == 0 {
        return Err(SourceSurfaceCorrespondenceError::ZeroComparisonBudget);
    }

    let exterior = validate_declared_exterior_fluid_mesh_input(mesh, marker_map)?;

    let mut sources = BTreeMap::<u64, &AuditedImportedSurfaceBody>::new();
    for source in audited_sources {
        if sources.insert(source.scene_object_id, source).is_some() {
            return Err(SourceSurfaceCorrespondenceError::DuplicateSourceSceneObject {
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
    for boundary in &mesh.boundary {
        if let Some(&scene_object_id) = marker_scene_ids.get(&boundary.marker) {
            body_boundary
                .entry(scene_object_id)
                .or_default()
                .push(boundary.vertices);
        }
    }

    for &scene_object_id in &exterior.scene_object_ids {
        if !sources.contains_key(&scene_object_id) {
            return Err(SourceSurfaceCorrespondenceError::MissingSourceForBoundary {
                scene_object_id,
            });
        }
    }
    for &scene_object_id in sources.keys() {
        if !body_boundary.contains_key(&scene_object_id) {
            return Err(SourceSurfaceCorrespondenceError::MissingBoundaryForSource {
                scene_object_id,
            });
        }
    }

    let mut prepared = Vec::<PreparedBodyComparison>::new();
    let mut requested_tests = 0_usize;
    for &scene_object_id in &exterior.scene_object_ids {
        let source = sources[&scene_object_id];
        let boundary_indices = &body_boundary[&scene_object_id];
        let source_triangles = surface_triangles(&source.mesh);
        let boundary_triangles = volume_boundary_triangles(mesh, boundary_indices);
        let source_samples = surface_samples(&source.mesh);
        let boundary_samples = volume_boundary_samples(mesh, boundary_indices);

        let forward = source_samples
            .len()
            .checked_mul(boundary_triangles.len())
            .ok_or(SourceSurfaceCorrespondenceError::ComparisonBudgetOverflow)?;
        let reverse = boundary_samples
            .len()
            .checked_mul(source_triangles.len())
            .ok_or(SourceSurfaceCorrespondenceError::ComparisonBudgetOverflow)?;
        requested_tests = requested_tests
            .checked_add(
                forward
                    .checked_add(reverse)
                    .ok_or(SourceSurfaceCorrespondenceError::ComparisonBudgetOverflow)?,
            )
            .ok_or(SourceSurfaceCorrespondenceError::ComparisonBudgetOverflow)?;

        prepared.push(PreparedBodyComparison {
            scene_object_id,
            source_triangles,
            boundary_triangles,
            source_samples,
            boundary_samples,
        });
    }

    if requested_tests > policy.max_point_triangle_tests {
        return Err(SourceSurfaceCorrespondenceError::ComparisonBudgetExceeded {
            requested: requested_tests,
            limit: policy.max_point_triangle_tests,
        });
    }

    let mut bodies = Vec::with_capacity(prepared.len());
    for body in prepared {
        let source_to_boundary =
            max_closest_distance(&body.source_samples, &body.boundary_triangles);
        let boundary_to_source =
            max_closest_distance(&body.boundary_samples, &body.source_triangles);
        if source_to_boundary > policy.distance_tolerance
            || boundary_to_source > policy.distance_tolerance
        {
            return Err(SourceSurfaceCorrespondenceError::DistanceToleranceExceeded {
                scene_object_id: body.scene_object_id,
                max_source_to_boundary_distance: source_to_boundary,
                max_boundary_to_source_distance: boundary_to_source,
                tolerance: policy.distance_tolerance,
            });
        }
        bodies.push(SourceSurfaceBodyCorrespondence {
            scene_object_id: body.scene_object_id,
            source_triangle_count: body.source_triangles.len(),
            boundary_triangle_count: body.boundary_triangles.len(),
            source_sample_count: body.source_samples.len(),
            boundary_sample_count: body.boundary_samples.len(),
            max_source_to_boundary_distance: source_to_boundary,
            max_boundary_to_source_distance: boundary_to_source,
        });
    }

    Ok(SourceSurfaceCorrespondenceReport {
        bodies,
        point_triangle_tests: requested_tests,
    })
}

struct PreparedBodyComparison {
    scene_object_id: u64,
    source_triangles: Vec<[[f64; 3]; 3]>,
    boundary_triangles: Vec<[[f64; 3]; 3]>,
    source_samples: Vec<[f64; 3]>,
    boundary_samples: Vec<[f64; 3]>,
}

fn validate_source_still_audited(
    source: &AuditedImportedSurfaceBody,
) -> Result<(), SourceSurfaceCorrespondenceError> {
    let topology = source.mesh.topology_report().map_err(|error| {
        SourceSurfaceCorrespondenceError::SourceSurfaceAuditInvalid {
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
        return Err(SourceSurfaceCorrespondenceError::SourceSurfaceAuditInvalid {
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

fn surface_triangles(mesh: &SurfaceMesh) -> Vec<[[f64; 3]; 3]> {
    mesh.triangles
        .iter()
        .map(|triangle| {
            [
                mesh.positions[triangle[0] as usize],
                mesh.positions[triangle[1] as usize],
                mesh.positions[triangle[2] as usize],
            ]
        })
        .collect()
}

fn volume_boundary_triangles(
    mesh: &VolumeMesh,
    triangles: &[[u32; 3]],
) -> Vec<[[f64; 3]; 3]> {
    triangles
        .iter()
        .map(|triangle| {
            [
                mesh.points[triangle[0] as usize],
                mesh.points[triangle[1] as usize],
                mesh.points[triangle[2] as usize],
            ]
        })
        .collect()
}

fn surface_samples(mesh: &SurfaceMesh) -> Vec<[f64; 3]> {
    let mut used_vertices = BTreeSet::<u32>::new();
    for triangle in &mesh.triangles {
        used_vertices.extend(triangle.iter().copied());
    }
    let mut samples = used_vertices
        .into_iter()
        .map(|index| mesh.positions[index as usize])
        .collect::<Vec<_>>();
    samples.extend(surface_triangles(mesh).iter().map(triangle_centroid));
    samples
}

fn volume_boundary_samples(mesh: &VolumeMesh, triangles: &[[u32; 3]]) -> Vec<[f64; 3]> {
    let mut used_vertices = BTreeSet::<u32>::new();
    for triangle in triangles {
        used_vertices.extend(triangle.iter().copied());
    }
    let mut samples = used_vertices
        .into_iter()
        .map(|index| mesh.points[index as usize])
        .collect::<Vec<_>>();
    samples.extend(
        volume_boundary_triangles(mesh, triangles)
            .iter()
            .map(triangle_centroid),
    );
    samples
}

fn triangle_centroid(triangle: &[[f64; 3]; 3]) -> [f64; 3] {
    [
        (triangle[0][0] + triangle[1][0] + triangle[2][0]) / 3.0,
        (triangle[0][1] + triangle[1][1] + triangle[2][1]) / 3.0,
        (triangle[0][2] + triangle[1][2] + triangle[2][2]) / 3.0,
    ]
}

fn max_closest_distance(samples: &[[f64; 3]], triangles: &[[[f64; 3]; 3]]) -> f64 {
    samples.iter().fold(0.0_f64, |max_distance, &sample| {
        let closest_squared = triangles.iter().fold(f64::INFINITY, |closest, triangle| {
            closest.min(point_triangle_distance_squared(sample, *triangle))
        });
        max_distance.max(closest_squared.sqrt())
    })
}

fn point_triangle_distance_squared(point: [f64; 3], triangle: [[f64; 3]; 3]) -> f64 {
    let [a, b, c] = triangle;
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(point, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return dot(ap, ap);
    }

    let bp = sub(point, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return dot(bp, bp);
    }

    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return squared_distance(point, add(a, scale(ab, v)));
    }

    let cp = sub(point, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return dot(cp, cp);
    }

    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return squared_distance(point, add(a, scale(ac, w)));
    }

    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return squared_distance(point, add(b, scale(sub(c, b), w)));
    }

    let denominator = 1.0 / (va + vb + vc);
    let v = vb * denominator;
    let w = vc * denominator;
    squared_distance(point, add(add(a, scale(ab, v)), scale(ac, w)))
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(v: [f64; 3], scalar: f64) -> [f64; 3] {
    [v[0] * scalar, v[1] * scalar, v[2] * scalar]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn squared_distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    let delta = sub(a, b);
    dot(delta, delta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeroforge_geometry_core::SurfaceMesh;
    use aeroforge_volume_core::BlockBoundaryMarkers;

    use crate::imported_surface::{
        audit_imported_surface_for_accurate_meshing, AccurateImportedSurfacePolicy,
    };
    use crate::scene_provenance::build_scene_owner_marker_provenance;
    use crate::su2_mesh::{
        BoundaryRole, BoundarySource, DomainAxis, DomainSide, Su2MarkerBinding,
    };
    use crate::voxel_mesh::{tetrahedralize_voxel_fluid_domain, VoxelFluidDomainSpec};

    fn domain() -> VoxelFluidDomainSpec {
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
            binding(1, "inlet", BoundaryRole::Inlet, DomainAxis::X, DomainSide::Min),
            binding(2, "outlet", BoundaryRole::Outlet, DomainAxis::X, DomainSide::Max),
            binding(3, "y_min", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Min),
            binding(4, "y_max", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Max),
            binding(5, "z_min", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Min),
            binding(6, "z_max", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Max),
        ]
    }

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

    fn fixture() -> (VolumeMesh, Su2MarkerMap, AuditedImportedSurfaceBody) {
        let provenance = build_scene_owner_marker_provenance(&[42], domain_bindings()).unwrap();
        let mut solid_owner = vec![0_u32; 27];
        solid_owner[(1 * 3 + 1) * 3 + 1] = 1;
        let mesh = tetrahedralize_voxel_fluid_domain(
            domain(),
            &solid_owner,
            &provenance.owner_markers,
        )
        .unwrap();
        let source = audit_imported_surface_for_accurate_meshing(
            42,
            &cube_surface([1.0, 1.0, 1.0], [2.0, 2.0, 2.0]),
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap();
        (mesh, provenance.marker_map, source)
    }

    fn test_policy() -> SourceSurfaceCorrespondencePolicy {
        SourceSurfaceCorrespondencePolicy {
            distance_tolerance: 1.0e-10,
            max_point_triangle_tests: 100_000,
        }
    }

    #[test]
    fn exact_cube_body_boundary_satisfies_bidirectional_correspondence() {
        let (mesh, marker_map, source) = fixture();
        let report = validate_source_surface_correspondence(
            &mesh,
            &marker_map,
            &[source],
            test_policy(),
        )
        .unwrap();
        assert_eq!(report.bodies.len(), 1);
        assert_eq!(report.bodies[0].scene_object_id, 42);
        assert_eq!(report.bodies[0].source_triangle_count, 12);
        assert_eq!(report.bodies[0].boundary_triangle_count, 12);
        assert!(report.bodies[0].max_source_to_boundary_distance <= 1.0e-12);
        assert!(report.bodies[0].max_boundary_to_source_distance <= 1.0e-12);
        assert_eq!(report.point_triangle_tests, 480);
    }

    #[test]
    fn shifted_source_surface_fails_distance_contract() {
        let (mesh, marker_map, mut source) = fixture();
        for point in &mut source.mesh.positions {
            point[0] += 0.1;
        }
        let error = validate_source_surface_correspondence(
            &mesh,
            &marker_map,
            &[source],
            SourceSurfaceCorrespondencePolicy {
                distance_tolerance: 1.0e-3,
                ..test_policy()
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            SourceSurfaceCorrespondenceError::DistanceToleranceExceeded {
                scene_object_id: 42,
                ..
            }
        ));
    }

    #[test]
    fn comparison_budget_is_fail_closed_not_silently_sampled() {
        let (mesh, marker_map, source) = fixture();
        let error = validate_source_surface_correspondence(
            &mesh,
            &marker_map,
            &[source],
            SourceSurfaceCorrespondencePolicy {
                max_point_triangle_tests: 100,
                ..test_policy()
            },
        )
        .unwrap_err();
        assert_eq!(
            error,
            SourceSurfaceCorrespondenceError::ComparisonBudgetExceeded {
                requested: 480,
                limit: 100,
            }
        );
    }

    #[test]
    fn exterior_body_requires_matching_audited_source() {
        let (mesh, marker_map, _) = fixture();
        assert_eq!(
            validate_source_surface_correspondence(&mesh, &marker_map, &[], test_policy())
                .unwrap_err(),
            SourceSurfaceCorrespondenceError::MissingSourceForBoundary {
                scene_object_id: 42,
            }
        );
    }

    #[test]
    fn duplicate_source_identity_fails_closed() {
        let (mesh, marker_map, source) = fixture();
        assert_eq!(
            validate_source_surface_correspondence(
                &mesh,
                &marker_map,
                &[source.clone(), source],
                test_policy(),
            )
            .unwrap_err(),
            SourceSurfaceCorrespondenceError::DuplicateSourceSceneObject {
                scene_object_id: 42,
            }
        );
    }
}
