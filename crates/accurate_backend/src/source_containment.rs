use std::error::Error;
use std::f64::consts::PI;
use std::fmt::{Display, Formatter};

use crate::exterior_mesher_admission::IntersectionValidatedExteriorMesherInput;
use crate::imported_surface::AuditedImportedSurfaceBody;

/// Explicit tolerance and work budget for rejecting nested source solids before exterior meshing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceContainmentPolicy {
    pub geometric_epsilon: f64,
    pub max_point_triangle_tests: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceContainmentReport {
    pub scene_object_ids: Vec<u64>,
    pub reserved_point_triangle_tests: usize,
    pub executed_point_triangle_tests: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SourceContainmentError {
    InvalidGeometricEpsilon { value: f64 },
    ZeroTestBudget,
    WorkBudgetOverflow,
    WorkBudgetExceeded { requested: usize, limit: usize },
    MissingSourceVertex { scene_object_id: u64 },
    NonFiniteContainment {
        candidate_inner_scene_object_id: u64,
        candidate_outer_scene_object_id: u64,
    },
    NestedOrNearContactBody {
        outer_scene_object_id: u64,
        inner_scene_object_id: u64,
    },
}

impl Display for SourceContainmentError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidGeometricEpsilon { value } => write!(
                f,
                "source-containment epsilon must be finite and non-negative; got {value}"
            ),
            Self::ZeroTestBudget => write!(
                f,
                "source-containment validation requires a non-zero point/triangle test budget"
            ),
            Self::WorkBudgetOverflow => write!(
                f,
                "source-containment point/triangle work count overflowed usize"
            ),
            Self::WorkBudgetExceeded { requested, limit } => write!(
                f,
                "source-containment validation reserves {requested} point/triangle tests, above the explicit limit {limit}"
            ),
            Self::MissingSourceVertex { scene_object_id } => write!(
                f,
                "SceneObject {scene_object_id} has no source vertex for containment classification"
            ),
            Self::NonFiniteContainment {
                candidate_inner_scene_object_id,
                candidate_outer_scene_object_id,
            } => write!(
                f,
                "containment classification became non-finite while testing SceneObject {candidate_inner_scene_object_id} against SceneObject {candidate_outer_scene_object_id}"
            ),
            Self::NestedOrNearContactBody {
                outer_scene_object_id,
                inner_scene_object_id,
            } => write!(
                f,
                "SceneObject {inner_scene_object_id} lies inside or within containment tolerance of SceneObject {outer_scene_object_id}; nested source solids are not a valid distinct exterior-fluid boundary set"
            ),
        }
    }
}

impl Error for SourceContainmentError {}

/// Source-shell input promoted after intersection admission and explicit nested-solid rejection.
///
/// For two connected, closed, non-intersecting shells, testing one boundary vertex from each shell
/// against the opposite volume is sufficient to detect complete nesting: moving between inside and
/// outside states on a connected shell would otherwise require a surface intersection. The prior
/// `IntersectionValidatedExteriorMesherInput` state is therefore a required precondition.
///
/// The admitted state remains private and is exposed only through shared accessors. This still does
/// not establish a positive minimum body-to-body clearance, constrained tetrahedralization,
/// body-fittedness, boundary-layer quality, or CFD accuracy.
#[derive(Clone, Debug, PartialEq)]
pub struct ContainmentValidatedExteriorMesherInput {
    input: IntersectionValidatedExteriorMesherInput,
    containment_policy: SourceContainmentPolicy,
    containment_report: SourceContainmentReport,
}

impl ContainmentValidatedExteriorMesherInput {
    pub fn admission(&self) -> &IntersectionValidatedExteriorMesherInput {
        &self.input
    }

    pub fn scene_object_ids(&self) -> Vec<u64> {
        self.input.scene_object_ids()
    }

    pub fn containment_policy(&self) -> SourceContainmentPolicy {
        self.containment_policy
    }

    pub fn containment_report(&self) -> &SourceContainmentReport {
        &self.containment_report
    }
}

/// Rejects one audited solid lying wholly inside another after source-shell intersections have
/// already been excluded.
///
/// Work is bounded before any winding evaluation begins. The reservation is the worst-case number
/// of point/triangle evaluations for testing one representative boundary vertex in each direction
/// for every source-body pair. AABB rejection can reduce the executed count but never raises the
/// reserved budget.
pub fn validate_exterior_mesher_source_containment(
    input: IntersectionValidatedExteriorMesherInput,
    policy: SourceContainmentPolicy,
) -> Result<ContainmentValidatedExteriorMesherInput, SourceContainmentError> {
    if !policy.geometric_epsilon.is_finite() || policy.geometric_epsilon < 0.0 {
        return Err(SourceContainmentError::InvalidGeometricEpsilon {
            value: policy.geometric_epsilon,
        });
    }
    if policy.max_point_triangle_tests == 0 {
        return Err(SourceContainmentError::ZeroTestBudget);
    }

    let sources = input.audited_sources();
    let mut reserved = 0_usize;
    for first in 0..sources.len() {
        for second in first + 1..sources.len() {
            reserved = reserved
                .checked_add(sources[first].mesh.triangles.len())
                .and_then(|value| value.checked_add(sources[second].mesh.triangles.len()))
                .ok_or(SourceContainmentError::WorkBudgetOverflow)?;
        }
    }
    if reserved > policy.max_point_triangle_tests {
        return Err(SourceContainmentError::WorkBudgetExceeded {
            requested: reserved,
            limit: policy.max_point_triangle_tests,
        });
    }

    let mut executed = 0_usize;
    for first in 0..sources.len() {
        for second in first + 1..sources.len() {
            let first_body = &sources[first];
            let second_body = &sources[second];
            let first_point = *first_body
                .mesh
                .positions
                .first()
                .ok_or(SourceContainmentError::MissingSourceVertex {
                    scene_object_id: first_body.scene_object_id,
                })?;
            let second_point = *second_body
                .mesh
                .positions
                .first()
                .ok_or(SourceContainmentError::MissingSourceVertex {
                    scene_object_id: second_body.scene_object_id,
                })?;

            if point_may_lie_in_bounds(second_body, first_point, policy.geometric_epsilon) {
                let inside = point_inside_surface(
                    second_body,
                    first_point,
                    policy.geometric_epsilon,
                    &mut executed,
                )
                .ok_or(SourceContainmentError::NonFiniteContainment {
                    candidate_inner_scene_object_id: first_body.scene_object_id,
                    candidate_outer_scene_object_id: second_body.scene_object_id,
                })?;
                if inside {
                    return Err(SourceContainmentError::NestedOrNearContactBody {
                        outer_scene_object_id: second_body.scene_object_id,
                        inner_scene_object_id: first_body.scene_object_id,
                    });
                }
            }

            if point_may_lie_in_bounds(first_body, second_point, policy.geometric_epsilon) {
                let inside = point_inside_surface(
                    first_body,
                    second_point,
                    policy.geometric_epsilon,
                    &mut executed,
                )
                .ok_or(SourceContainmentError::NonFiniteContainment {
                    candidate_inner_scene_object_id: second_body.scene_object_id,
                    candidate_outer_scene_object_id: first_body.scene_object_id,
                })?;
                if inside {
                    return Err(SourceContainmentError::NestedOrNearContactBody {
                        outer_scene_object_id: first_body.scene_object_id,
                        inner_scene_object_id: second_body.scene_object_id,
                    });
                }
            }
        }
    }

    Ok(ContainmentValidatedExteriorMesherInput {
        containment_report: SourceContainmentReport {
            scene_object_ids: input.scene_object_ids(),
            reserved_point_triangle_tests: reserved,
            executed_point_triangle_tests: executed,
        },
        input,
        containment_policy: policy,
    })
}

fn point_may_lie_in_bounds(
    body: &AuditedImportedSurfaceBody,
    point: [f64; 3],
    epsilon: f64,
) -> bool {
    (0..3).all(|axis| {
        point[axis] >= body.bounds.min[axis] - epsilon
            && point[axis] <= body.bounds.max[axis] + epsilon
    })
}

/// Returns `None` only when floating-point evaluation becomes non-finite.
fn point_inside_surface(
    body: &AuditedImportedSurfaceBody,
    point: [f64; 3],
    epsilon: f64,
    executed: &mut usize,
) -> Option<bool> {
    let mut winding = 0.0_f64;
    for triangle in &body.mesh.triangles {
        *executed = executed.checked_add(1)?;
        let a = body.mesh.positions[triangle[0] as usize];
        let b = body.mesh.positions[triangle[1] as usize];
        let c = body.mesh.positions[triangle[2] as usize];
        if point_on_triangle(point, a, b, c, epsilon) {
            return Some(true);
        }

        let va = sub(a, point);
        let vb = sub(b, point);
        let vc = sub(c, point);
        let la = norm(va);
        let lb = norm(vb);
        let lc = norm(vc);
        if !la.is_finite() || !lb.is_finite() || !lc.is_finite() {
            return None;
        }
        if la <= epsilon || lb <= epsilon || lc <= epsilon {
            return Some(true);
        }
        let numerator = dot(va, cross(vb, vc));
        let denominator = la * lb * lc
            + dot(va, vb) * lc
            + dot(vb, vc) * la
            + dot(vc, va) * lb;
        let angle = 2.0 * numerator.atan2(denominator);
        if !angle.is_finite() {
            return None;
        }
        winding += angle;
    }
    if !winding.is_finite() {
        return None;
    }
    Some(winding.abs() > 2.0 * PI)
}

fn point_on_triangle(
    point: [f64; 3],
    a: [f64; 3],
    b: [f64; 3],
    c: [f64; 3],
    epsilon: f64,
) -> bool {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let normal = cross(ab, ac);
    let normal_norm = norm(normal);
    if !normal_norm.is_finite() || normal_norm <= 0.0 {
        return false;
    }
    let ap = sub(point, a);
    if dot(ap, normal).abs() > epsilon * normal_norm {
        return false;
    }

    let d00 = dot(ab, ab);
    let d01 = dot(ab, ac);
    let d11 = dot(ac, ac);
    let d20 = dot(ap, ab);
    let d21 = dot(ap, ac);
    let denominator = d00 * d11 - d01 * d01;
    if !denominator.is_finite() || denominator <= 0.0 {
        return false;
    }
    let v = (d11 * d20 - d01 * d21) / denominator;
    let w = (d00 * d21 - d01 * d20) / denominator;
    let u = 1.0 - v - w;
    let barycentric_epsilon = 256.0 * f64::EPSILON;
    u >= -barycentric_epsilon && v >= -barycentric_epsilon && w >= -barycentric_epsilon
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(v: [f64; 3]) -> f64 {
    dot(v, v).sqrt()
}

#[cfg(test)]
mod tests {
    use aeroforge_geometry_core::SurfaceMesh;
    use aeroforge_volume_core::BoundaryMarkerId;

    use crate::exterior_mesher_admission::validate_exterior_mesher_input_intersections;
    use crate::exterior_mesher_input::build_validated_exterior_mesher_input;
    use crate::imported_surface::{
        audit_imported_surface_for_accurate_meshing, AccurateImportedSurfacePolicy,
        AuditedImportedSurfaceBody,
    };
    use crate::source_intersection::SourceSurfaceIntersectionPolicy;
    use crate::su2_mesh::{
        BoundaryRole, BoundarySource, DomainAxis, DomainSide, Su2MarkerBinding,
    };

    use super::*;

    fn tetra_surface(scale: f64, offset: [f64; 3]) -> SurfaceMesh {
        let p = |x: f64, y: f64, z: f64| {
            [
                offset[0] + scale * x,
                offset[1] + scale * y,
                offset[2] + scale * z,
            ]
        };
        SurfaceMesh {
            positions: vec![
                p(0.0, 0.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(0.0, 1.0, 0.0),
                p(0.0, 0.0, 1.0),
            ],
            triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        }
    }

    fn audited(id: u64, scale: f64, offset: [f64; 3]) -> AuditedImportedSurfaceBody {
        audit_imported_surface_for_accurate_meshing(
            id,
            &tetra_surface(scale, offset),
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap()
    }

    fn binding(
        marker: u32,
        tag: &str,
        role: BoundaryRole,
        axis: DomainAxis,
        side: DomainSide,
    ) -> Su2MarkerBinding {
        Su2MarkerBinding {
            marker: BoundaryMarkerId(marker),
            tag: tag.into(),
            role,
            source: BoundarySource::DomainFace { axis, side },
        }
    }

    fn domain_bindings() -> Vec<Su2MarkerBinding> {
        vec![
            binding(1, "x_min", BoundaryRole::Inlet, DomainAxis::X, DomainSide::Min),
            binding(2, "x_max", BoundaryRole::Outlet, DomainAxis::X, DomainSide::Max),
            binding(3, "y_min", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Min),
            binding(4, "y_max", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Max),
            binding(5, "z_min", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Min),
            binding(6, "z_max", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Max),
        ]
    }

    fn admitted(bodies: Vec<AuditedImportedSurfaceBody>) -> IntersectionValidatedExteriorMesherInput {
        let input = build_validated_exterior_mesher_input(
            [-1.0, -1.0, -1.0],
            [6.0, 6.0, 6.0],
            domain_bindings(),
            bodies,
        )
        .unwrap();
        validate_exterior_mesher_input_intersections(
            input,
            SourceSurfaceIntersectionPolicy {
                geometric_epsilon: 1.0e-10,
                max_triangle_pair_tests: 10_000,
            },
        )
        .unwrap()
    }

    fn policy(limit: usize) -> SourceContainmentPolicy {
        SourceContainmentPolicy {
            geometric_epsilon: 1.0e-10,
            max_point_triangle_tests: limit,
        }
    }

    #[test]
    fn disjoint_shells_pass_with_explicit_work_evidence() {
        let input = admitted(vec![
            audited(42, 1.0, [0.0, 0.0, 0.0]),
            audited(77, 1.0, [3.0, 3.0, 3.0]),
        ]);
        let validated = validate_exterior_mesher_source_containment(input, policy(100)).unwrap();

        assert_eq!(validated.scene_object_ids(), vec![42, 77]);
        assert_eq!(validated.containment_report().reserved_point_triangle_tests, 8);
        assert_eq!(validated.containment_report().executed_point_triangle_tests, 0);
    }

    #[test]
    fn fully_nested_closed_body_fails_before_meshing() {
        let input = admitted(vec![
            audited(42, 4.0, [0.0, 0.0, 0.0]),
            audited(77, 0.4, [0.5, 0.5, 0.5]),
        ]);

        assert_eq!(
            validate_exterior_mesher_source_containment(input, policy(100)).unwrap_err(),
            SourceContainmentError::NestedOrNearContactBody {
                outer_scene_object_id: 42,
                inner_scene_object_id: 77,
            }
        );
    }

    #[test]
    fn work_budget_is_checked_before_winding_tests() {
        let input = admitted(vec![
            audited(42, 1.0, [0.0, 0.0, 0.0]),
            audited(77, 1.0, [3.0, 3.0, 3.0]),
        ]);

        assert_eq!(
            validate_exterior_mesher_source_containment(input, policy(7)).unwrap_err(),
            SourceContainmentError::WorkBudgetExceeded {
                requested: 8,
                limit: 7,
            }
        );
    }
}
