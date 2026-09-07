use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_geometry_core::SurfaceMesh;

use crate::imported_surface::AuditedImportedSurfaceBody;

/// Explicit geometric tolerance and work budget for source-surface intersection checks.
///
/// `geometric_epsilon` is expressed in the same coordinate units as the source surfaces. The
/// triangle-pair budget is evaluated before geometric intersection work begins; AeroForge does not
/// silently sample or truncate the check when the budget is exceeded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceSurfaceIntersectionPolicy {
    pub geometric_epsilon: f64,
    pub max_triangle_pair_tests: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceSurfaceIntersectionReport {
    pub scene_object_ids: Vec<u64>,
    pub triangle_pair_tests: usize,
    pub skipped_shared_edge_pairs: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SourceSurfaceIntersectionError {
    InvalidGeometricEpsilon { value: f64 },
    ZeroPairBudget,
    DuplicateSourceSceneObject { scene_object_id: u64 },
    SourceSurfaceAuditInvalid {
        scene_object_id: u64,
        message: String,
    },
    PairBudgetOverflow,
    PairBudgetExceeded {
        requested: usize,
        limit: usize,
    },
    SelfIntersection {
        scene_object_id: u64,
        triangle_a: usize,
        triangle_b: usize,
    },
    InterBodyIntersection {
        first_scene_object_id: u64,
        second_scene_object_id: u64,
        first_triangle: usize,
        second_triangle: usize,
    },
}

impl Display for SourceSurfaceIntersectionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidGeometricEpsilon { value } => write!(
                f,
                "source-surface intersection epsilon must be finite and non-negative; got {value}"
            ),
            Self::ZeroPairBudget => write!(
                f,
                "source-surface intersection validation requires a non-zero triangle-pair budget"
            ),
            Self::DuplicateSourceSceneObject { scene_object_id } => write!(
                f,
                "source-surface intersection validation received duplicate audited sources for SceneObject {scene_object_id}"
            ),
            Self::SourceSurfaceAuditInvalid {
                scene_object_id,
                message,
            } => write!(
                f,
                "source surface for SceneObject {scene_object_id} no longer satisfies the accurate imported-surface audit: {message}"
            ),
            Self::PairBudgetOverflow => write!(
                f,
                "source-surface intersection triangle-pair count overflowed usize"
            ),
            Self::PairBudgetExceeded { requested, limit } => write!(
                f,
                "source-surface intersection validation requires {requested} triangle-pair tests, above the explicit limit {limit}"
            ),
            Self::SelfIntersection {
                scene_object_id,
                triangle_a,
                triangle_b,
            } => write!(
                f,
                "SceneObject {scene_object_id} source surface self-intersects between triangles {triangle_a} and {triangle_b}"
            ),
            Self::InterBodyIntersection {
                first_scene_object_id,
                second_scene_object_id,
                first_triangle,
                second_triangle,
            } => write!(
                f,
                "source surfaces for SceneObject {first_scene_object_id} and SceneObject {second_scene_object_id} intersect between triangles {first_triangle} and {second_triangle}"
            ),
        }
    }
}

impl Error for SourceSurfaceIntersectionError {}

/// Rejects self-intersecting audited source shells and intersections between distinct source bodies.
///
/// For one body's own triangulation, pairs sharing a complete topological edge are skipped because
/// that adjacency is already required by the watertight two-manifold audit. Pairs sharing only one
/// vertex are still checked, while the shared vertex itself is treated as the one legal contact.
/// Every pair between distinct SceneObjects is checked and any contact/intersection fails closed.
///
/// The test uses expanded AABB rejection, plane/segment checks, and a dominant-axis 2D path for
/// coplanar cases. Passing this contract does not prove CAD quality, minimum body separation,
/// volumetric tetrahedron non-overlap, body-fitted meshing, or engineering CFD quality.
pub fn validate_source_surface_intersections(
    audited_sources: &[AuditedImportedSurfaceBody],
    policy: SourceSurfaceIntersectionPolicy,
) -> Result<SourceSurfaceIntersectionReport, SourceSurfaceIntersectionError> {
    if !policy.geometric_epsilon.is_finite() || policy.geometric_epsilon < 0.0 {
        return Err(SourceSurfaceIntersectionError::InvalidGeometricEpsilon {
            value: policy.geometric_epsilon,
        });
    }
    if policy.max_triangle_pair_tests == 0 {
        return Err(SourceSurfaceIntersectionError::ZeroPairBudget);
    }

    let mut sources = BTreeMap::<u64, &AuditedImportedSurfaceBody>::new();
    for source in audited_sources {
        if sources.insert(source.scene_object_id, source).is_some() {
            return Err(SourceSurfaceIntersectionError::DuplicateSourceSceneObject {
                scene_object_id: source.scene_object_id,
            });
        }
        validate_source_still_audited(source)?;
    }

    let ordered = sources.into_iter().collect::<Vec<_>>();
    let mut requested_pairs = 0_usize;
    let mut skipped_shared_edge_pairs = 0_usize;

    for (_, source) in &ordered {
        for first in 0..source.mesh.triangles.len() {
            for second in first + 1..source.mesh.triangles.len() {
                if shared_vertex_indices(
                    source.mesh.triangles[first],
                    source.mesh.triangles[second],
                )
                .len()
                    >= 2
                {
                    skipped_shared_edge_pairs = skipped_shared_edge_pairs
                        .checked_add(1)
                        .ok_or(SourceSurfaceIntersectionError::PairBudgetOverflow)?;
                } else {
                    requested_pairs = requested_pairs
                        .checked_add(1)
                        .ok_or(SourceSurfaceIntersectionError::PairBudgetOverflow)?;
                }
            }
        }
    }

    for first_body in 0..ordered.len() {
        for second_body in first_body + 1..ordered.len() {
            let first_triangles = ordered[first_body].1.mesh.triangles.len();
            let second_triangles = ordered[second_body].1.mesh.triangles.len();
            requested_pairs = requested_pairs
                .checked_add(
                    first_triangles
                        .checked_mul(second_triangles)
                        .ok_or(SourceSurfaceIntersectionError::PairBudgetOverflow)?,
                )
                .ok_or(SourceSurfaceIntersectionError::PairBudgetOverflow)?;
        }
    }

    if requested_pairs > policy.max_triangle_pair_tests {
        return Err(SourceSurfaceIntersectionError::PairBudgetExceeded {
            requested: requested_pairs,
            limit: policy.max_triangle_pair_tests,
        });
    }

    for &(scene_object_id, source) in &ordered {
        for first in 0..source.mesh.triangles.len() {
            for second in first + 1..source.mesh.triangles.len() {
                let first_indices = source.mesh.triangles[first];
                let second_indices = source.mesh.triangles[second];
                let shared = shared_vertex_indices(first_indices, second_indices);
                if shared.len() >= 2 {
                    continue;
                }
                let ignore_point = shared
                    .first()
                    .map(|&index| source.mesh.positions[index as usize]);
                if triangles_intersect(
                    triangle_points(&source.mesh, first_indices),
                    triangle_points(&source.mesh, second_indices),
                    policy.geometric_epsilon,
                    ignore_point,
                ) {
                    return Err(SourceSurfaceIntersectionError::SelfIntersection {
                        scene_object_id,
                        triangle_a: first,
                        triangle_b: second,
                    });
                }
            }
        }
    }

    for first_body in 0..ordered.len() {
        for second_body in first_body + 1..ordered.len() {
            let (first_scene_object_id, first_source) = ordered[first_body];
            let (second_scene_object_id, second_source) = ordered[second_body];
            for (first_triangle, &first_indices) in
                first_source.mesh.triangles.iter().enumerate()
            {
                for (second_triangle, &second_indices) in
                    second_source.mesh.triangles.iter().enumerate()
                {
                    if triangles_intersect(
                        triangle_points(&first_source.mesh, first_indices),
                        triangle_points(&second_source.mesh, second_indices),
                        policy.geometric_epsilon,
                        None,
                    ) {
                        return Err(SourceSurfaceIntersectionError::InterBodyIntersection {
                            first_scene_object_id,
                            second_scene_object_id,
                            first_triangle,
                            second_triangle,
                        });
                    }
                }
            }
        }
    }

    Ok(SourceSurfaceIntersectionReport {
        scene_object_ids: ordered.iter().map(|(id, _)| *id).collect(),
        triangle_pair_tests: requested_pairs,
        skipped_shared_edge_pairs,
    })
}

fn validate_source_still_audited(
    source: &AuditedImportedSurfaceBody,
) -> Result<(), SourceSurfaceIntersectionError> {
    let topology = source.mesh.topology_report().map_err(|error| {
        SourceSurfaceIntersectionError::SourceSurfaceAuditInvalid {
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
        return Err(SourceSurfaceIntersectionError::SourceSurfaceAuditInvalid {
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

fn shared_vertex_indices(first: [u32; 3], second: [u32; 3]) -> Vec<u32> {
    first
        .into_iter()
        .filter(|index| second.contains(index))
        .collect()
}

fn triangle_points(mesh: &SurfaceMesh, triangle: [u32; 3]) -> [[f64; 3]; 3] {
    [
        mesh.positions[triangle[0] as usize],
        mesh.positions[triangle[1] as usize],
        mesh.positions[triangle[2] as usize],
    ]
}

fn triangles_intersect(
    first: [[f64; 3]; 3],
    second: [[f64; 3]; 3],
    epsilon: f64,
    ignore_point: Option<[f64; 3]>,
) -> bool {
    if !aabb_overlap(first, second, epsilon) {
        return false;
    }

    let first_normal = cross(sub(first[1], first[0]), sub(first[2], first[0]));
    let second_normal = cross(sub(second[1], second[0]), sub(second[2], second[0]));
    let first_normal_length = length(first_normal);
    let second_normal_length = length(second_normal);
    if first_normal_length == 0.0 || second_normal_length == 0.0 {
        return true;
    }

    let second_to_first = second.map(|point| {
        dot(first_normal, sub(point, first[0])) / first_normal_length
    });
    let first_to_second = first.map(|point| {
        dot(second_normal, sub(point, second[0])) / second_normal_length
    });

    if same_strict_side(second_to_first, epsilon) || same_strict_side(first_to_second, epsilon) {
        return false;
    }

    let coplanar = second_to_first.iter().all(|distance| distance.abs() <= epsilon)
        && first_to_second.iter().all(|distance| distance.abs() <= epsilon);
    if coplanar {
        return coplanar_triangles_intersect(first, second, first_normal, epsilon, ignore_point);
    }

    for edge in triangle_edges(first) {
        if segment_intersects_triangle(edge, second, epsilon, ignore_point) {
            return true;
        }
    }
    for edge in triangle_edges(second) {
        if segment_intersects_triangle(edge, first, epsilon, ignore_point) {
            return true;
        }
    }
    false
}

fn aabb_overlap(first: [[f64; 3]; 3], second: [[f64; 3]; 3], epsilon: f64) -> bool {
    for axis in 0..3 {
        let first_min = first.iter().map(|point| point[axis]).fold(f64::INFINITY, f64::min);
        let first_max = first.iter().map(|point| point[axis]).fold(f64::NEG_INFINITY, f64::max);
        let second_min = second.iter().map(|point| point[axis]).fold(f64::INFINITY, f64::min);
        let second_max = second.iter().map(|point| point[axis]).fold(f64::NEG_INFINITY, f64::max);
        if first_max + epsilon < second_min || second_max + epsilon < first_min {
            return false;
        }
    }
    true
}

fn same_strict_side(distances: [f64; 3], epsilon: f64) -> bool {
    distances.iter().all(|distance| *distance > epsilon)
        || distances.iter().all(|distance| *distance < -epsilon)
}

fn triangle_edges(triangle: [[f64; 3]; 3]) -> [[[f64; 3]; 2]; 3] {
    [
        [triangle[0], triangle[1]],
        [triangle[1], triangle[2]],
        [triangle[2], triangle[0]],
    ]
}

fn segment_intersects_triangle(
    segment: [[f64; 3]; 2],
    triangle: [[f64; 3]; 3],
    epsilon: f64,
    ignore_point: Option<[f64; 3]>,
) -> bool {
    let normal = cross(sub(triangle[1], triangle[0]), sub(triangle[2], triangle[0]));
    let normal_length = length(normal);
    let mut d0 = dot(normal, sub(segment[0], triangle[0])) / normal_length;
    let mut d1 = dot(normal, sub(segment[1], triangle[0])) / normal_length;

    // `ignore_point` is supplied only for a topologically shared mesh vertex. If one endpoint is
    // exactly that shared vertex, it is mathematically on the opposite triangle plane. Floating
    // evaluation of the normalized plane equation can nevertheless produce a tiny non-zero signed
    // distance, which shifts the reconstructed hit just beyond the caller's geometric epsilon and
    // turns a legal vertex fan into a false self-intersection. Snap only the exact shared endpoint
    // to zero plane distance; do not widen epsilon or suppress any other segment contact.
    if ignore_point.is_some_and(|point| point == segment[0]) {
        d0 = 0.0;
    }
    if ignore_point.is_some_and(|point| point == segment[1]) {
        d1 = 0.0;
    }

    if d0.abs() <= epsilon && d1.abs() <= epsilon {
        return coplanar_segment_intersects_triangle(
            segment,
            triangle,
            normal,
            epsilon,
            ignore_point,
        );
    }
    if (d0 > epsilon && d1 > epsilon) || (d0 < -epsilon && d1 < -epsilon) {
        return false;
    }

    let denominator = d0 - d1;
    if denominator.abs() <= f64::EPSILON {
        return false;
    }
    let t = d0 / denominator;
    if t < 0.0 || t > 1.0 {
        return false;
    }
    let hit = add(segment[0], scale(sub(segment[1], segment[0]), t));
    point_in_triangle_3d(hit, triangle, normal, epsilon) && !ignored_hit(hit, ignore_point, epsilon)
}

fn point_in_triangle_3d(
    point: [f64; 3],
    triangle: [[f64; 3]; 3],
    normal: [f64; 3],
    epsilon: f64,
) -> bool {
    let axis = dominant_axis(normal);
    point_in_triangle_2d(
        project(point, axis),
        triangle.map(|vertex| project(vertex, axis)),
        projected_area_epsilon(triangle, epsilon),
    )
}

fn coplanar_triangles_intersect(
    first: [[f64; 3]; 3],
    second: [[f64; 3]; 3],
    normal: [f64; 3],
    epsilon: f64,
    ignore_point: Option<[f64; 3]>,
) -> bool {
    let axis = dominant_axis(normal);
    let first_2d = first.map(|point| project(point, axis));
    let second_2d = second.map(|point| project(point, axis));
    let ignore_2d = ignore_point.map(|point| project(point, axis));
    let area_epsilon = projected_area_epsilon(first, epsilon)
        .max(projected_area_epsilon(second, epsilon));

    for (index, point) in first_2d.iter().copied().enumerate() {
        if point_in_triangle_2d(point, second_2d, area_epsilon)
            && !ignored_hit_2d(point, ignore_2d, epsilon)
            && !ignored_hit(first[index], ignore_point, epsilon)
        {
            return true;
        }
    }
    for (index, point) in second_2d.iter().copied().enumerate() {
        if point_in_triangle_2d(point, first_2d, area_epsilon)
            && !ignored_hit_2d(point, ignore_2d, epsilon)
            && !ignored_hit(second[index], ignore_point, epsilon)
        {
            return true;
        }
    }

    for first_edge in triangle_edges_2d(first_2d) {
        for second_edge in triangle_edges_2d(second_2d) {
            if segments_intersect_2d_beyond_ignore(
                first_edge,
                second_edge,
                area_epsilon,
                epsilon,
                ignore_2d,
            ) {
                return true;
            }
        }
    }
    false
}

fn coplanar_segment_intersects_triangle(
    segment: [[f64; 3]; 2],
    triangle: [[f64; 3]; 3],
    normal: [f64; 3],
    epsilon: f64,
    ignore_point: Option<[f64; 3]>,
) -> bool {
    let axis = dominant_axis(normal);
    let segment_2d = segment.map(|point| project(point, axis));
    let triangle_2d = triangle.map(|point| project(point, axis));
    let ignore_2d = ignore_point.map(|point| project(point, axis));
    let area_epsilon = projected_area_epsilon(triangle, epsilon);

    for (index, point) in segment_2d.iter().copied().enumerate() {
        if point_in_triangle_2d(point, triangle_2d, area_epsilon)
            && !ignored_hit_2d(point, ignore_2d, epsilon)
            && !ignored_hit(segment[index], ignore_point, epsilon)
        {
            return true;
        }
    }
    for edge in triangle_edges_2d(triangle_2d) {
        if segments_intersect_2d_beyond_ignore(
            segment_2d,
            edge,
            area_epsilon,
            epsilon,
            ignore_2d,
        ) {
            return true;
        }
    }
    false
}

fn point_in_triangle_2d(point: [f64; 2], triangle: [[f64; 2]; 3], epsilon: f64) -> bool {
    let first = orient_2d(triangle[0], triangle[1], point);
    let second = orient_2d(triangle[1], triangle[2], point);
    let third = orient_2d(triangle[2], triangle[0], point);
    let has_negative = first < -epsilon || second < -epsilon || third < -epsilon;
    let has_positive = first > epsilon || second > epsilon || third > epsilon;
    !(has_negative && has_positive)
}

fn segments_intersect_2d_beyond_ignore(
    first: [[f64; 2]; 2],
    second: [[f64; 2]; 2],
    area_epsilon: f64,
    distance_epsilon: f64,
    ignore_point: Option<[f64; 2]>,
) -> bool {
    let first_direction = sub_2d(first[1], first[0]);
    let second_direction = sub_2d(second[1], second[0]);
    let denominator = cross_2d(first_direction, second_direction);
    let offset = sub_2d(second[0], first[0]);

    if denominator.abs() > area_epsilon {
        let t = cross_2d(offset, second_direction) / denominator;
        let u = cross_2d(offset, first_direction) / denominator;
        if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
            let hit = add_2d(first[0], scale_2d(first_direction, t));
            return !ignored_hit_2d(hit, ignore_point, distance_epsilon);
        }
        return false;
    }

    if orient_2d(first[0], first[1], second[0]).abs() > area_epsilon {
        return false;
    }

    for point in [first[0], first[1], second[0], second[1]] {
        if point_on_segment_2d(point, first, area_epsilon)
            && point_on_segment_2d(point, second, area_epsilon)
            && !ignored_hit_2d(point, ignore_point, distance_epsilon)
        {
            return true;
        }
    }
    false
}

fn point_on_segment_2d(point: [f64; 2], segment: [[f64; 2]; 2], epsilon: f64) -> bool {
    orient_2d(segment[0], segment[1], point).abs() <= epsilon
        && point[0] >= segment[0][0].min(segment[1][0]) - epsilon
        && point[0] <= segment[0][0].max(segment[1][0]) + epsilon
        && point[1] >= segment[0][1].min(segment[1][1]) - epsilon
        && point[1] <= segment[0][1].max(segment[1][1]) + epsilon
}

fn projected_area_epsilon(triangle: [[f64; 3]; 3], distance_epsilon: f64) -> f64 {
    let max_edge = triangle_edges(triangle)
        .iter()
        .map(|edge| length(sub(edge[1], edge[0])))
        .fold(0.0_f64, f64::max);
    distance_epsilon * max_edge.max(1.0)
}

fn dominant_axis(normal: [f64; 3]) -> usize {
    let absolute = normal.map(f64::abs);
    if absolute[0] >= absolute[1] && absolute[0] >= absolute[2] {
        0
    } else if absolute[1] >= absolute[2] {
        1
    } else {
        2
    }
}

fn project(point: [f64; 3], axis: usize) -> [f64; 2] {
    match axis {
        0 => [point[1], point[2]],
        1 => [point[0], point[2]],
        _ => [point[0], point[1]],
    }
}

fn triangle_edges_2d(triangle: [[f64; 2]; 3]) -> [[[f64; 2]; 2]; 3] {
    [
        [triangle[0], triangle[1]],
        [triangle[1], triangle[2]],
        [triangle[2], triangle[0]],
    ]
}

fn orient_2d(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    cross_2d(sub_2d(b, a), sub_2d(c, a))
}

fn ignored_hit(point: [f64; 3], ignore_point: Option<[f64; 3]>, epsilon: f64) -> bool {
    ignore_point.is_some_and(|ignore| length(sub(point, ignore)) <= epsilon)
}

fn ignored_hit_2d(point: [f64; 2], ignore_point: Option<[f64; 2]>, epsilon: f64) -> bool {
    ignore_point.is_some_and(|ignore| {
        let delta = sub_2d(point, ignore);
        (delta[0] * delta[0] + delta[1] * delta[1]).sqrt() <= epsilon
    })
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

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn length(value: [f64; 3]) -> f64 {
    dot(value, value).sqrt()
}

fn add_2d(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

fn sub_2d(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn scale_2d(value: [f64; 2], scalar: f64) -> [f64; 2] {
    [value[0] * scalar, value[1] * scalar]
}

fn cross_2d(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeroforge_geometry_core::SurfaceMesh;

    use crate::imported_surface::{
        audit_imported_surface_for_accurate_meshing, AccurateImportedSurfacePolicy,
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

    fn audited(id: u64, min: [f64; 3], max: [f64; 3]) -> AuditedImportedSurfaceBody {
        audit_imported_surface_for_accurate_meshing(
            id,
            &cube_surface(min, max),
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap()
    }

    fn policy() -> SourceSurfaceIntersectionPolicy {
        SourceSurfaceIntersectionPolicy {
            geometric_epsilon: 1.0e-10,
            max_triangle_pair_tests: 10_000,
        }
    }

    #[test]
    fn clean_closed_cube_has_no_non_topological_self_intersection() {
        let source = audited(42, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let report = validate_source_surface_intersections(&[source], policy()).unwrap();
        assert_eq!(report.scene_object_ids, vec![42]);
        assert!(report.triangle_pair_tests > 0);
        assert!(report.skipped_shared_edge_pairs > 0);
    }

    #[test]
    fn shared_vertex_endpoint_roundoff_is_not_a_false_intersection() {
        // These coordinates are the exact f32->f64 desktop sphere vertices that previously made
        // two legal vertex-fan triangles report a hit about 1.08e-10 m away from their shared
        // endpoint under a 1e-10 m policy, solely because the shared endpoint's normalized plane
        // distance rounded to a tiny non-zero value.
        let shared = [0.2414814531803131, 3.433012694120407, 0.0647047609090805];
        let first = [
            [0.125, 3.482962906360626, 0.03349364921450615],
            [0.1120719313621521, 3.482962906360626, 0.0647047609090805],
            shared,
        ];
        let second = [
            shared,
            [0.21650634706020355, 3.433012694120407, 0.125],
            [0.34150633215904236, 3.353553384542465, 0.09150634706020355],
        ];

        assert!(!triangles_intersect(first, second, 1.0e-10, Some(shared)));
    }

    #[test]
    fn shared_vertex_does_not_hide_coplanar_overlap_away_from_shared_point() {
        let shared = [0.0, 0.0, 0.0];
        let first = [shared, [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]];
        let second = [shared, [1.5, 0.5, 0.0], [0.5, 1.5, 0.0]];

        assert!(triangles_intersect(first, second, 1.0e-10, Some(shared)));
    }

    #[test]
    fn deformed_topologically_valid_cube_self_intersection_fails_closed() {
        let mut source = audited(42, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        source.mesh.positions[6] = [-0.5, 0.5, 0.5];
        let topology = source.mesh.topology_report().unwrap();
        assert!(topology.watertight_two_manifold);
        assert!(topology.consistently_oriented);
        assert!(topology.signed_volume.is_some_and(|volume| volume > 0.0));

        assert!(matches!(
            validate_source_surface_intersections(&[source], policy()),
            Err(SourceSurfaceIntersectionError::SelfIntersection {
                scene_object_id: 42,
                ..
            })
        ));
    }

    #[test]
    fn distinct_overlapping_body_surfaces_fail_closed() {
        let first = audited(3, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let second = audited(9, [0.5, 0.5, 0.5], [1.5, 1.5, 1.5]);
        assert!(matches!(
            validate_source_surface_intersections(&[second, first], policy()),
            Err(SourceSurfaceIntersectionError::InterBodyIntersection {
                first_scene_object_id: 3,
                second_scene_object_id: 9,
                ..
            })
        ));
    }

    #[test]
    fn separated_bodies_pass_in_stable_scene_id_order() {
        let first = audited(3, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        let second = audited(9, [2.0, 2.0, 2.0], [3.0, 3.0, 3.0]);
        let report = validate_source_surface_intersections(&[second, first], policy()).unwrap();
        assert_eq!(report.scene_object_ids, vec![3, 9]);
    }

    #[test]
    fn pair_budget_fails_before_silent_sampling() {
        let source = audited(42, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        assert!(matches!(
            validate_source_surface_intersections(
                &[source],
                SourceSurfaceIntersectionPolicy {
                    max_triangle_pair_tests: 1,
                    ..policy()
                },
            ),
            Err(SourceSurfaceIntersectionError::PairBudgetExceeded {
                requested,
                limit: 1,
            }) if requested > 1
        ));
    }
}
