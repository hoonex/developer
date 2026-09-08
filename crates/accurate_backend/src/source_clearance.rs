use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_geometry_core::SurfaceMesh;

use crate::source_containment::ContainmentValidatedExteriorMesherInput;

/// Explicit positive separation requirement and bounded work budget for distinct audited source
/// bodies.
///
/// `minimum_clearance` is expressed in the same coordinate units as the source surfaces and must be
/// finite and strictly positive. The complete inter-body triangle-pair work is reserved before any
/// distance evaluation begins; AeroForge does not silently sample or truncate this evidence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceInterBodyClearancePolicy {
    pub minimum_clearance: f64,
    pub max_triangle_pair_tests: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceInterBodyClearancePairReport {
    pub first_scene_object_id: u64,
    pub second_scene_object_id: u64,
    pub first_triangle_count: usize,
    pub second_triangle_count: usize,
    pub minimum_clearance: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceInterBodyClearanceReport {
    pub pairs: Vec<SourceInterBodyClearancePairReport>,
    pub triangle_pair_tests: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SourceInterBodyClearanceError {
    InvalidMinimumClearance { value: f64 },
    ZeroPairBudget,
    PairBudgetOverflow,
    PairBudgetExceeded { requested: usize, limit: usize },
    NonFiniteDistance {
        first_scene_object_id: u64,
        second_scene_object_id: u64,
        first_triangle: usize,
        second_triangle: usize,
    },
    InsufficientClearance {
        first_scene_object_id: u64,
        second_scene_object_id: u64,
        observed: f64,
        required: f64,
    },
}

impl Display for SourceInterBodyClearanceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMinimumClearance { value } => write!(
                f,
                "source inter-body minimum clearance must be finite and positive; got {value}"
            ),
            Self::ZeroPairBudget => write!(
                f,
                "source inter-body clearance validation requires a non-zero triangle-pair budget"
            ),
            Self::PairBudgetOverflow => write!(
                f,
                "source inter-body clearance triangle-pair count overflowed usize"
            ),
            Self::PairBudgetExceeded { requested, limit } => write!(
                f,
                "source inter-body clearance validation requires {requested} triangle-pair tests, above the explicit limit {limit}"
            ),
            Self::NonFiniteDistance {
                first_scene_object_id,
                second_scene_object_id,
                first_triangle,
                second_triangle,
            } => write!(
                f,
                "source inter-body clearance became non-finite between SceneObject {first_scene_object_id} triangle {first_triangle} and SceneObject {second_scene_object_id} triangle {second_triangle}"
            ),
            Self::InsufficientClearance {
                first_scene_object_id,
                second_scene_object_id,
                observed,
                required,
            } => write!(
                f,
                "source bodies for SceneObject {first_scene_object_id} and SceneObject {second_scene_object_id} have minimum clearance {observed}, below the explicit required clearance {required}"
            ),
        }
    }
}

impl Error for SourceInterBodyClearanceError {}

/// Establishes bounded positive surface separation between every pair of distinct source bodies
/// that has already passed source intersection and containment admission.
///
/// For every SceneObject pair, this checks every triangle pair and retains the minimum Euclidean
/// triangle-to-triangle distance. Triangle distance includes vertex-to-triangle and edge-to-edge
/// closest approaches, so skew edge interiors are not missed. SceneObject pairs are reported in
/// stable ascending ID order.
///
/// The containment-admitted state is a required precondition: surface distance alone cannot
/// distinguish two disjoint bodies from one closed body nested wholly inside another. Passing this
/// gate therefore extends the existing no-contact/non-nesting source evidence with an explicit
/// caller-selected positive clearance floor.
///
/// Passing does not establish a universal engineering separation threshold, mesher feature
/// preservation, body-fitted output, boundary-layer quality, or CFD accuracy.
pub fn validate_source_inter_body_clearance(
    input: &ContainmentValidatedExteriorMesherInput,
    policy: SourceInterBodyClearancePolicy,
) -> Result<SourceInterBodyClearanceReport, SourceInterBodyClearanceError> {
    if !policy.minimum_clearance.is_finite() || policy.minimum_clearance <= 0.0 {
        return Err(SourceInterBodyClearanceError::InvalidMinimumClearance {
            value: policy.minimum_clearance,
        });
    }
    if policy.max_triangle_pair_tests == 0 {
        return Err(SourceInterBodyClearanceError::ZeroPairBudget);
    }

    let mut sources = input.admission().audited_sources().iter().collect::<Vec<_>>();
    sources.sort_by_key(|source| source.scene_object_id);

    let mut requested_pairs = 0_usize;
    for first in 0..sources.len() {
        for second in first + 1..sources.len() {
            requested_pairs = requested_pairs
                .checked_add(
                    sources[first]
                        .mesh
                        .triangles
                        .len()
                        .checked_mul(sources[second].mesh.triangles.len())
                        .ok_or(SourceInterBodyClearanceError::PairBudgetOverflow)?,
                )
                .ok_or(SourceInterBodyClearanceError::PairBudgetOverflow)?;
        }
    }
    if requested_pairs > policy.max_triangle_pair_tests {
        return Err(SourceInterBodyClearanceError::PairBudgetExceeded {
            requested: requested_pairs,
            limit: policy.max_triangle_pair_tests,
        });
    }

    let mut pairs = Vec::new();
    for first in 0..sources.len() {
        for second in first + 1..sources.len() {
            let first_source = sources[first];
            let second_source = sources[second];
            let mut minimum_squared = f64::INFINITY;

            for (first_triangle, &first_indices) in
                first_source.mesh.triangles.iter().enumerate()
            {
                let first_points = triangle_points(&first_source.mesh, first_indices);
                for (second_triangle, &second_indices) in
                    second_source.mesh.triangles.iter().enumerate()
                {
                    let second_points = triangle_points(&second_source.mesh, second_indices);
                    let distance_squared =
                        triangle_triangle_distance_squared(first_points, second_points);
                    if !distance_squared.is_finite() || distance_squared < 0.0 {
                        return Err(SourceInterBodyClearanceError::NonFiniteDistance {
                            first_scene_object_id: first_source.scene_object_id,
                            second_scene_object_id: second_source.scene_object_id,
                            first_triangle,
                            second_triangle,
                        });
                    }
                    minimum_squared = minimum_squared.min(distance_squared);
                }
            }

            let minimum_clearance = minimum_squared.sqrt();
            if !minimum_clearance.is_finite() {
                return Err(SourceInterBodyClearanceError::NonFiniteDistance {
                    first_scene_object_id: first_source.scene_object_id,
                    second_scene_object_id: second_source.scene_object_id,
                    first_triangle: 0,
                    second_triangle: 0,
                });
            }
            if minimum_clearance < policy.minimum_clearance {
                return Err(SourceInterBodyClearanceError::InsufficientClearance {
                    first_scene_object_id: first_source.scene_object_id,
                    second_scene_object_id: second_source.scene_object_id,
                    observed: minimum_clearance,
                    required: policy.minimum_clearance,
                });
            }

            pairs.push(SourceInterBodyClearancePairReport {
                first_scene_object_id: first_source.scene_object_id,
                second_scene_object_id: second_source.scene_object_id,
                first_triangle_count: first_source.mesh.triangles.len(),
                second_triangle_count: second_source.mesh.triangles.len(),
                minimum_clearance,
            });
        }
    }

    Ok(SourceInterBodyClearanceReport {
        pairs,
        triangle_pair_tests: requested_pairs,
    })
}

fn triangle_points(mesh: &SurfaceMesh, triangle: [u32; 3]) -> [[f64; 3]; 3] {
    [
        mesh.positions[triangle[0] as usize],
        mesh.positions[triangle[1] as usize],
        mesh.positions[triangle[2] as usize],
    ]
}

fn triangle_triangle_distance_squared(first: [[f64; 3]; 3], second: [[f64; 3]; 3]) -> f64 {
    let mut minimum = f64::INFINITY;

    for point in first {
        minimum = minimum.min(point_triangle_distance_squared(point, second));
    }
    for point in second {
        minimum = minimum.min(point_triangle_distance_squared(point, first));
    }
    for first_edge in triangle_edges(first) {
        for second_edge in triangle_edges(second) {
            minimum = minimum.min(segment_segment_distance_squared(first_edge, second_edge));
        }
    }

    minimum.max(0.0)
}

fn point_triangle_distance_squared(point: [f64; 3], triangle: [[f64; 3]; 3]) -> f64 {
    let a = triangle[0];
    let b = triangle[1];
    let c = triangle[2];
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
        let delta = sub(point, add(a, scale(ab, v)));
        return dot(delta, delta);
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
        let delta = sub(point, add(a, scale(ac, w)));
        return dot(delta, delta);
    }

    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let bc = sub(c, b);
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        let delta = sub(point, add(b, scale(bc, w)));
        return dot(delta, delta);
    }

    let denominator = va + vb + vc;
    let inverse = 1.0 / denominator;
    let v = vb * inverse;
    let w = vc * inverse;
    let closest = add(a, add(scale(ab, v), scale(ac, w)));
    let delta = sub(point, closest);
    dot(delta, delta)
}

fn segment_segment_distance_squared(first: [[f64; 3]; 2], second: [[f64; 3]; 2]) -> f64 {
    let d1 = sub(first[1], first[0]);
    let d2 = sub(second[1], second[0]);
    let r = sub(first[0], second[0]);
    let a = dot(d1, d1);
    let e = dot(d2, d2);
    let f = dot(d2, r);

    let (s, t) = if a <= f64::EPSILON && e <= f64::EPSILON {
        (0.0, 0.0)
    } else if a <= f64::EPSILON {
        (0.0, clamp01(f / e))
    } else {
        let c = dot(d1, r);
        if e <= f64::EPSILON {
            (clamp01(-c / a), 0.0)
        } else {
            let b = dot(d1, d2);
            let denominator = a * e - b * b;
            let mut s = if denominator.abs() > f64::EPSILON {
                clamp01((b * f - c * e) / denominator)
            } else {
                0.0
            };
            let mut t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = clamp01(-c / a);
            } else if t > 1.0 {
                t = 1.0;
                s = clamp01((b - c) / a);
            }
            (s, t)
        }
    };

    let closest_first = add(first[0], scale(d1, s));
    let closest_second = add(second[0], scale(d2, t));
    let delta = sub(closest_first, closest_second);
    dot(delta, delta)
}

fn triangle_edges(triangle: [[f64; 3]; 3]) -> [[[f64; 3]; 2]; 3] {
    [
        [triangle[0], triangle[1]],
        [triangle[1], triangle[2]],
        [triangle[2], triangle[0]],
    ]
}

fn clamp01(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(value: [f64; 3], scalar: f64) -> [f64; 3] {
    [value[0] * scalar, value[1] * scalar, value[2] * scalar]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeroforge_volume_core::BoundaryMarkerId;

    use crate::exterior_mesher_admission::validate_exterior_mesher_input_intersections;
    use crate::exterior_mesher_input::build_validated_exterior_mesher_input;
    use crate::imported_surface::{
        audit_imported_surface_for_accurate_meshing, AccurateImportedSurfacePolicy,
        AuditedImportedSurfaceBody,
    };
    use crate::source_containment::{
        validate_exterior_mesher_source_containment, SourceContainmentPolicy,
    };
    use crate::source_intersection::SourceSurfaceIntersectionPolicy;
    use crate::su2_mesh::{
        BoundaryRole, BoundarySource, DomainAxis, DomainSide, Su2MarkerBinding,
    };

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

    fn audited(
        scene_object_id: u64,
        min: [f64; 3],
        max: [f64; 3],
    ) -> AuditedImportedSurfaceBody {
        audit_imported_surface_for_accurate_meshing(
            scene_object_id,
            &cube_surface(min, max),
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap()
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

    fn containment_admitted(
        mut sources: Vec<AuditedImportedSurfaceBody>,
    ) -> ContainmentValidatedExteriorMesherInput {
        sources.sort_by_key(|source| source.scene_object_id);
        let base = build_validated_exterior_mesher_input(
            [0.0, 0.0, 0.0],
            [6.0, 6.0, 6.0],
            domain_bindings(),
            sources,
        )
        .unwrap();
        let intersected = validate_exterior_mesher_input_intersections(
            base,
            SourceSurfaceIntersectionPolicy {
                geometric_epsilon: 1.0e-10,
                max_triangle_pair_tests: 100_000,
            },
        )
        .unwrap();
        validate_exterior_mesher_source_containment(
            intersected,
            SourceContainmentPolicy {
                geometric_epsilon: 1.0e-10,
                max_point_triangle_tests: 100_000,
            },
        )
        .unwrap()
    }

    fn policy(minimum_clearance: f64) -> SourceInterBodyClearancePolicy {
        SourceInterBodyClearancePolicy {
            minimum_clearance,
            max_triangle_pair_tests: 100_000,
        }
    }

    #[test]
    fn separated_cubes_report_exact_surface_gap_in_stable_id_order() {
        let first = audited(3, [1.0, 1.0, 1.0], [2.0, 2.0, 2.0]);
        let second = audited(9, [3.0, 1.0, 1.0], [4.0, 2.0, 2.0]);
        let input = containment_admitted(vec![second, first]);

        let report = validate_source_inter_body_clearance(&input, policy(0.5)).unwrap();
        assert_eq!(report.triangle_pair_tests, 144);
        assert_eq!(report.pairs.len(), 1);
        assert_eq!(report.pairs[0].first_scene_object_id, 3);
        assert_eq!(report.pairs[0].second_scene_object_id, 9);
        assert!((report.pairs[0].minimum_clearance - 1.0).abs() < 1.0e-12);
    }

    #[test]
    fn positive_but_insufficient_gap_fails_closed() {
        let first = audited(3, [1.0, 1.0, 1.0], [2.0, 2.0, 2.0]);
        let second = audited(9, [2.25, 1.0, 1.0], [3.25, 2.0, 2.0]);
        let input = containment_admitted(vec![first, second]);

        assert!(matches!(
            validate_source_inter_body_clearance(&input, policy(0.5)),
            Err(SourceInterBodyClearanceError::InsufficientClearance {
                first_scene_object_id: 3,
                second_scene_object_id: 9,
                observed,
                required: 0.5,
            }) if (observed - 0.25).abs() < 1.0e-12
        ));
    }

    #[test]
    fn complete_pair_budget_is_reserved_before_distance_work() {
        let first = audited(3, [1.0, 1.0, 1.0], [2.0, 2.0, 2.0]);
        let second = audited(9, [3.0, 1.0, 1.0], [4.0, 2.0, 2.0]);
        let input = containment_admitted(vec![first, second]);

        assert!(matches!(
            validate_source_inter_body_clearance(
                &input,
                SourceInterBodyClearancePolicy {
                    minimum_clearance: 0.5,
                    max_triangle_pair_tests: 143,
                },
            ),
            Err(SourceInterBodyClearanceError::PairBudgetExceeded {
                requested: 144,
                limit: 143,
            })
        ));
    }

    #[test]
    fn single_body_has_no_inter_body_work_or_claimed_pair_distance() {
        let input = containment_admitted(vec![audited(
            42,
            [1.0, 1.0, 1.0],
            [2.0, 2.0, 2.0],
        )]);

        let report = validate_source_inter_body_clearance(&input, policy(0.5)).unwrap();
        assert_eq!(report.triangle_pair_tests, 0);
        assert!(report.pairs.is_empty());
    }

    #[test]
    fn zero_or_non_finite_clearance_policy_is_rejected() {
        let input = containment_admitted(vec![audited(
            42,
            [1.0, 1.0, 1.0],
            [2.0, 2.0, 2.0],
        )]);

        assert!(matches!(
            validate_source_inter_body_clearance(&input, policy(0.0)),
            Err(SourceInterBodyClearanceError::InvalidMinimumClearance { value: 0.0 })
        ));
        assert!(matches!(
            validate_source_inter_body_clearance(&input, policy(f64::NAN)),
            Err(SourceInterBodyClearanceError::InvalidMinimumClearance { value }) if value.is_nan()
        ));
    }
}
