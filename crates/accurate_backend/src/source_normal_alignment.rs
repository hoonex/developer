use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_geometry_core::SurfaceMesh;
use aeroforge_volume_core::{BoundaryMarkerId, VolumeMesh};

use crate::boundary_orientation::{
    orient_exterior_boundary_triangles, BoundaryOrientationError,
};
use crate::exterior_mesh::{
    validate_declared_exterior_fluid_mesh_input, DeclaredExteriorFluidMeshError,
};
use crate::imported_surface::AuditedImportedSurfaceBody;
use crate::su2_mesh::{BoundarySource, Su2MarkerMap};

/// Explicit bounded policy for triangle-normal evidence between audited source surfaces and
/// SceneObject body boundaries in an exterior-fluid volume mesh.
///
/// `minimum_opposition_cosine` is in `[0, 1]`. A value of `1` requires exact anti-parallel unit
/// normals. Anti-parallel is the expected sign because audited source normals point outward from
/// the solid while canonical volume-side body-wall normals point outward from the fluid, into the
/// solid. `distance_tolerance` is used to keep every centroid-to-nearest-triangle association local.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceBoundaryNormalPolicy {
    pub distance_tolerance: f64,
    pub minimum_opposition_cosine: f64,
    pub max_triangle_pair_tests: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceBoundaryNormalBodyReport {
    pub scene_object_id: u64,
    pub source_triangle_count: usize,
    pub boundary_triangle_count: usize,
    pub max_source_to_boundary_centroid_distance: f64,
    pub max_boundary_to_source_centroid_distance: f64,
    pub min_source_to_boundary_opposition_cosine: f64,
    pub min_boundary_to_source_opposition_cosine: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceBoundaryNormalReport {
    pub bodies: Vec<SourceBoundaryNormalBodyReport>,
    pub triangle_pair_tests: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NormalComparisonDirection {
    SourceToBoundary,
    BoundaryToSource,
}

impl Display for NormalComparisonDirection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceToBoundary => write!(f, "source->boundary"),
            Self::BoundaryToSource => write!(f, "boundary->source"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SourceBoundaryNormalError {
    Exterior(DeclaredExteriorFluidMeshError),
    BoundaryOrientation(BoundaryOrientationError),
    InvalidDistanceTolerance { value: f64 },
    InvalidMinimumOppositionCosine { value: f64 },
    ZeroComparisonBudget,
    DuplicateSourceSceneObject { scene_object_id: u64 },
    MissingSourceForBoundary { scene_object_id: u64 },
    MissingBoundaryForSource { scene_object_id: u64 },
    SourceSurfaceAuditInvalid {
        scene_object_id: u64,
        message: String,
    },
    DegenerateSourceTriangle {
        scene_object_id: u64,
        triangle: usize,
    },
    DegenerateBoundaryTriangle {
        scene_object_id: u64,
        triangle: usize,
    },
    ComparisonBudgetOverflow,
    ComparisonBudgetExceeded {
        requested: usize,
        limit: usize,
    },
    DistanceToleranceExceeded {
        scene_object_id: u64,
        direction: NormalComparisonDirection,
        triangle: usize,
        distance: f64,
        tolerance: f64,
    },
    OppositionBelowThreshold {
        scene_object_id: u64,
        direction: NormalComparisonDirection,
        triangle: usize,
        opposition_cosine: f64,
        minimum: f64,
    },
}

impl Display for SourceBoundaryNormalError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exterior(error) => write!(f, "exterior-fluid input contract failed: {error}"),
            Self::BoundaryOrientation(error) => {
                write!(f, "canonical exterior boundary orientation failed: {error}")
            }
            Self::InvalidDistanceTolerance { value } => write!(
                f,
                "source/boundary normal distance tolerance must be finite and non-negative; got {value}"
            ),
            Self::InvalidMinimumOppositionCosine { value } => write!(
                f,
                "source/boundary minimum opposition cosine must be finite and in [0, 1]; got {value}"
            ),
            Self::ZeroComparisonBudget => write!(
                f,
                "source/boundary normal validation requires a non-zero triangle-pair budget"
            ),
            Self::DuplicateSourceSceneObject { scene_object_id } => write!(
                f,
                "source/boundary normal validation received duplicate audited sources for SceneObject {scene_object_id}"
            ),
            Self::MissingSourceForBoundary { scene_object_id } => write!(
                f,
                "body boundary for SceneObject {scene_object_id} has no audited source surface"
            ),
            Self::MissingBoundaryForSource { scene_object_id } => write!(
                f,
                "audited source surface for SceneObject {scene_object_id} has no body boundary"
            ),
            Self::SourceSurfaceAuditInvalid {
                scene_object_id,
                message,
            } => write!(
                f,
                "source surface for SceneObject {scene_object_id} no longer satisfies the accurate imported-surface audit: {message}"
            ),
            Self::DegenerateSourceTriangle {
                scene_object_id,
                triangle,
            } => write!(
                f,
                "source triangle {triangle} for SceneObject {scene_object_id} has no finite unit normal"
            ),
            Self::DegenerateBoundaryTriangle {
                scene_object_id,
                triangle,
            } => write!(
                f,
                "body-boundary triangle {triangle} for SceneObject {scene_object_id} has no finite unit normal"
            ),
            Self::ComparisonBudgetOverflow => write!(
                f,
                "source/boundary normal triangle-pair comparison count overflowed usize"
            ),
            Self::ComparisonBudgetExceeded { requested, limit } => write!(
                f,
                "source/boundary normal validation requires {requested} triangle-pair tests, above the explicit limit {limit}"
            ),
            Self::DistanceToleranceExceeded {
                scene_object_id,
                direction,
                triangle,
                distance,
                tolerance,
            } => write!(
                f,
                "SceneObject {scene_object_id} {direction} triangle {triangle} nearest opposite-surface distance {distance} exceeds tolerance {tolerance}"
            ),
            Self::OppositionBelowThreshold {
                scene_object_id,
                direction,
                triangle,
                opposition_cosine,
                minimum,
            } => write!(
                f,
                "SceneObject {scene_object_id} {direction} triangle {triangle} normal opposition cosine {opposition_cosine} is below required minimum {minimum}"
            ),
        }
    }
}

impl Error for SourceBoundaryNormalError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Exterior(error) => Some(error),
            Self::BoundaryOrientation(error) => Some(error),
            _ => None,
        }
    }
}

impl From<DeclaredExteriorFluidMeshError> for SourceBoundaryNormalError {
    fn from(value: DeclaredExteriorFluidMeshError) -> Self {
        Self::Exterior(value)
    }
}

impl From<BoundaryOrientationError> for SourceBoundaryNormalError {
    fn from(value: BoundaryOrientationError) -> Self {
        Self::BoundaryOrientation(value)
    }
}

#[derive(Clone, Copy)]
struct NormalTriangle {
    points: [[f64; 3]; 3],
    centroid: [f64; 3],
    unit_normal: [f64; 3],
}

struct PreparedBodyNormalComparison {
    scene_object_id: u64,
    source: Vec<NormalTriangle>,
    boundary: Vec<NormalTriangle>,
}

/// Validates bounded bidirectional nearest-triangle normal opposition for every SceneObject body.
///
/// The volume-side boundary winding is reconstructed from each face's unique owning positive
/// tetrahedron rather than trusting raw external-mesher `.face` ordering. Every source-triangle
/// centroid scans every corresponding body-boundary triangle, and every boundary-triangle centroid
/// scans every source triangle. Associations use deterministic first-record tie breaking at equal
/// squared distance. The complete `2 * source_triangles * boundary_triangles` work is reserved
/// before geometric comparison; budget exhaustion fails closed.
///
/// Passing this gate proves only the retained centroid-local nearest-triangle normal-opposition
/// contract under the explicit distance/cosine/work policy. It does **not** establish exact
/// triangle identity, sharp-feature or curvature preservation, body-fitted fidelity, boundary-layer
/// quality, solver accuracy, or engineering-quality CFD.
pub fn validate_source_boundary_normal_alignment(
    mesh: &VolumeMesh,
    marker_map: &Su2MarkerMap,
    audited_sources: &[AuditedImportedSurfaceBody],
    policy: SourceBoundaryNormalPolicy,
) -> Result<SourceBoundaryNormalReport, SourceBoundaryNormalError> {
    if !policy.distance_tolerance.is_finite() || policy.distance_tolerance < 0.0 {
        return Err(SourceBoundaryNormalError::InvalidDistanceTolerance {
            value: policy.distance_tolerance,
        });
    }
    if !policy.minimum_opposition_cosine.is_finite()
        || !(0.0..=1.0).contains(&policy.minimum_opposition_cosine)
    {
        return Err(SourceBoundaryNormalError::InvalidMinimumOppositionCosine {
            value: policy.minimum_opposition_cosine,
        });
    }
    if policy.max_triangle_pair_tests == 0 {
        return Err(SourceBoundaryNormalError::ZeroComparisonBudget);
    }

    let exterior = validate_declared_exterior_fluid_mesh_input(mesh, marker_map)?;
    let oriented_boundary = orient_exterior_boundary_triangles(mesh)?;

    let mut sources = BTreeMap::<u64, &AuditedImportedSurfaceBody>::new();
    for source in audited_sources {
        if sources.insert(source.scene_object_id, source).is_some() {
            return Err(SourceBoundaryNormalError::DuplicateSourceSceneObject {
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
            body_boundary
                .entry(scene_object_id)
                .or_default()
                .push(boundary.vertices);
        }
    }

    for &scene_object_id in &exterior.scene_object_ids {
        if !sources.contains_key(&scene_object_id) {
            return Err(SourceBoundaryNormalError::MissingSourceForBoundary {
                scene_object_id,
            });
        }
    }
    for &scene_object_id in sources.keys() {
        if !body_boundary.contains_key(&scene_object_id) {
            return Err(SourceBoundaryNormalError::MissingBoundaryForSource {
                scene_object_id,
            });
        }
    }

    let mut prepared = Vec::with_capacity(exterior.scene_object_ids.len());
    let mut requested_tests = 0_usize;
    for &scene_object_id in &exterior.scene_object_ids {
        let source = source_normal_triangles(sources[&scene_object_id], scene_object_id)?;
        let boundary = boundary_normal_triangles(
            mesh,
            &body_boundary[&scene_object_id],
            scene_object_id,
        )?;
        let one_direction = source
            .len()
            .checked_mul(boundary.len())
            .ok_or(SourceBoundaryNormalError::ComparisonBudgetOverflow)?;
        requested_tests = requested_tests
            .checked_add(
                one_direction
                    .checked_mul(2)
                    .ok_or(SourceBoundaryNormalError::ComparisonBudgetOverflow)?,
            )
            .ok_or(SourceBoundaryNormalError::ComparisonBudgetOverflow)?;
        prepared.push(PreparedBodyNormalComparison {
            scene_object_id,
            source,
            boundary,
        });
    }

    if requested_tests > policy.max_triangle_pair_tests {
        return Err(SourceBoundaryNormalError::ComparisonBudgetExceeded {
            requested: requested_tests,
            limit: policy.max_triangle_pair_tests,
        });
    }

    let mut bodies = Vec::with_capacity(prepared.len());
    for body in prepared {
        let (max_forward_distance, min_forward_opposition) = compare_direction(
            body.scene_object_id,
            NormalComparisonDirection::SourceToBoundary,
            &body.source,
            &body.boundary,
            policy,
        )?;
        let (max_reverse_distance, min_reverse_opposition) = compare_direction(
            body.scene_object_id,
            NormalComparisonDirection::BoundaryToSource,
            &body.boundary,
            &body.source,
            policy,
        )?;
        bodies.push(SourceBoundaryNormalBodyReport {
            scene_object_id: body.scene_object_id,
            source_triangle_count: body.source.len(),
            boundary_triangle_count: body.boundary.len(),
            max_source_to_boundary_centroid_distance: max_forward_distance,
            max_boundary_to_source_centroid_distance: max_reverse_distance,
            min_source_to_boundary_opposition_cosine: min_forward_opposition,
            min_boundary_to_source_opposition_cosine: min_reverse_opposition,
        });
    }

    Ok(SourceBoundaryNormalReport {
        bodies,
        triangle_pair_tests: requested_tests,
    })
}

fn compare_direction(
    scene_object_id: u64,
    direction: NormalComparisonDirection,
    source: &[NormalTriangle],
    target: &[NormalTriangle],
    policy: SourceBoundaryNormalPolicy,
) -> Result<(f64, f64), SourceBoundaryNormalError> {
    let mut max_distance = 0.0_f64;
    let mut min_opposition = 1.0_f64;
    for (triangle_index, triangle) in source.iter().enumerate() {
        let (nearest_index, distance_squared) = target
            .iter()
            .enumerate()
            .fold((0_usize, f64::INFINITY), |best, (index, candidate)| {
                let distance = point_triangle_distance_squared(triangle.centroid, candidate.points);
                if distance < best.1 {
                    (index, distance)
                } else {
                    best
                }
            });
        let distance = distance_squared.sqrt();
        if !distance.is_finite() || distance > policy.distance_tolerance {
            return Err(SourceBoundaryNormalError::DistanceToleranceExceeded {
                scene_object_id,
                direction,
                triangle: triangle_index,
                distance,
                tolerance: policy.distance_tolerance,
            });
        }
        max_distance = max_distance.max(distance);

        let opposition = (-dot(
            triangle.unit_normal,
            target[nearest_index].unit_normal,
        ))
        .clamp(-1.0, 1.0);
        if opposition < policy.minimum_opposition_cosine {
            return Err(SourceBoundaryNormalError::OppositionBelowThreshold {
                scene_object_id,
                direction,
                triangle: triangle_index,
                opposition_cosine: opposition,
                minimum: policy.minimum_opposition_cosine,
            });
        }
        min_opposition = min_opposition.min(opposition);
    }
    Ok((max_distance, min_opposition))
}

fn validate_source_still_audited(
    source: &AuditedImportedSurfaceBody,
) -> Result<(), SourceBoundaryNormalError> {
    let topology = source.mesh.topology_report().map_err(|error| {
        SourceBoundaryNormalError::SourceSurfaceAuditInvalid {
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
        return Err(SourceBoundaryNormalError::SourceSurfaceAuditInvalid {
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

fn source_normal_triangles(
    source: &AuditedImportedSurfaceBody,
    scene_object_id: u64,
) -> Result<Vec<NormalTriangle>, SourceBoundaryNormalError> {
    source
        .mesh
        .triangles
        .iter()
        .enumerate()
        .map(|(triangle_index, &vertices)| {
            let points = [
                source.mesh.positions[vertices[0] as usize],
                source.mesh.positions[vertices[1] as usize],
                source.mesh.positions[vertices[2] as usize],
            ];
            normal_triangle(points).ok_or(SourceBoundaryNormalError::DegenerateSourceTriangle {
                scene_object_id,
                triangle: triangle_index,
            })
        })
        .collect()
}

fn boundary_normal_triangles(
    mesh: &VolumeMesh,
    triangles: &[[u32; 3]],
    scene_object_id: u64,
) -> Result<Vec<NormalTriangle>, SourceBoundaryNormalError> {
    triangles
        .iter()
        .enumerate()
        .map(|(triangle_index, &vertices)| {
            let points = [
                mesh.points[vertices[0] as usize],
                mesh.points[vertices[1] as usize],
                mesh.points[vertices[2] as usize],
            ];
            normal_triangle(points).ok_or(SourceBoundaryNormalError::DegenerateBoundaryTriangle {
                scene_object_id,
                triangle: triangle_index,
            })
        })
        .collect()
}

fn normal_triangle(points: [[f64; 3]; 3]) -> Option<NormalTriangle> {
    let normal = cross(sub(points[1], points[0]), sub(points[2], points[0]));
    let length_squared = dot(normal, normal);
    if !length_squared.is_finite() || length_squared <= 0.0 {
        return None;
    }
    let inverse_length = length_squared.sqrt().recip();
    let unit_normal = [
        normal[0] * inverse_length,
        normal[1] * inverse_length,
        normal[2] * inverse_length,
    ];
    if !unit_normal.iter().all(|value| value.is_finite()) {
        return None;
    }
    Some(NormalTriangle {
        points,
        centroid: triangle_centroid(&points),
        unit_normal,
    })
}

fn triangle_centroid(triangle: &[[f64; 3]; 3]) -> [f64; 3] {
    [
        (triangle[0][0] + triangle[1][0] + triangle[2][0]) / 3.0,
        (triangle[0][1] + triangle[1][1] + triangle[2][1]) / 3.0,
        (triangle[0][2] + triangle[1][2] + triangle[2][2]) / 3.0,
    ]
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
        let projection = add(a, scale(ab, v));
        return dot(sub(point, projection), sub(point, projection));
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
        let projection = add(a, scale(ac, w));
        return dot(sub(point, projection), sub(point, projection));
    }

    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let edge = sub(c, b);
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        let projection = add(b, scale(edge, w));
        return dot(sub(point, projection), sub(point, projection));
    }

    let denominator = (va + vb + vc).recip();
    let v = vb * denominator;
    let w = vc * denominator;
    let projection = add(a, add(scale(ab, v), scale(ac, w)));
    dot(sub(point, projection), sub(point, projection))
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
    use crate::su2_mesh::{
        BoundaryRole, DomainAxis, DomainSide, Su2MarkerBinding,
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

    fn cube_surface() -> SurfaceMesh {
        let (x0, y0, z0) = (1.0, 1.0, 1.0);
        let (x1, y1, z1) = (2.0, 2.0, 2.0);
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

    fn fixture(source_surface: SurfaceMesh) -> (VolumeMesh, Su2MarkerMap, AuditedImportedSurfaceBody) {
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
            &source_surface,
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap();
        (mesh, provenance.marker_map, source)
    }

    fn policy() -> SourceBoundaryNormalPolicy {
        SourceBoundaryNormalPolicy {
            distance_tolerance: 1.0e-10,
            minimum_opposition_cosine: 0.999_999,
            max_triangle_pair_tests: 10_000,
        }
    }

    #[test]
    fn exact_body_surface_passes_with_canonical_opposite_normals_even_if_raw_winding_is_reversed() {
        let (mut mesh, marker_map, source) = fixture(cube_surface());
        for boundary in &mut mesh.boundary {
            boundary.vertices.swap(1, 2);
        }
        mesh.audit().unwrap();

        let report = validate_source_boundary_normal_alignment(
            &mesh,
            &marker_map,
            &[source],
            policy(),
        )
        .unwrap();

        assert_eq!(report.bodies.len(), 1);
        assert_eq!(report.bodies[0].scene_object_id, 42);
        assert_eq!(report.bodies[0].source_triangle_count, 12);
        assert_eq!(report.bodies[0].boundary_triangle_count, 12);
        assert_eq!(report.triangle_pair_tests, 288);
        assert!(report.bodies[0].max_source_to_boundary_centroid_distance <= 1.0e-12);
        assert!(report.bodies[0].max_boundary_to_source_centroid_distance <= 1.0e-12);
        assert!(report.bodies[0].min_source_to_boundary_opposition_cosine > 0.999_999_999);
        assert!(report.bodies[0].min_boundary_to_source_opposition_cosine > 0.999_999_999);
    }

    #[test]
    fn tilted_nearby_source_is_rejected_by_normal_threshold() {
        let mut tilted = cube_surface();
        tilted.positions[6][2] += 0.1;
        let (mesh, marker_map, source) = fixture(tilted);
        let error = validate_source_boundary_normal_alignment(
            &mesh,
            &marker_map,
            &[source],
            SourceBoundaryNormalPolicy {
                distance_tolerance: 0.2,
                minimum_opposition_cosine: 0.999_9,
                max_triangle_pair_tests: 10_000,
            },
        )
        .unwrap_err();

        assert!(matches!(
            error,
            SourceBoundaryNormalError::OppositionBelowThreshold {
                scene_object_id: 42,
                ..
            }
        ));
    }

    #[test]
    fn pair_budget_is_preflighted_before_normal_comparison() {
        let (mesh, marker_map, source) = fixture(cube_surface());
        let error = validate_source_boundary_normal_alignment(
            &mesh,
            &marker_map,
            &[source],
            SourceBoundaryNormalPolicy {
                max_triangle_pair_tests: 287,
                ..policy()
            },
        )
        .unwrap_err();

        assert_eq!(
            error,
            SourceBoundaryNormalError::ComparisonBudgetExceeded {
                requested: 288,
                limit: 287,
            }
        );
    }
}
