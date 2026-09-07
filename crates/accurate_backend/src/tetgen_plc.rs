use std::error::Error;
use std::f64::consts::PI;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::BoundaryMarkerId;

use crate::imported_surface::AuditedImportedSurfaceBody;
use crate::source_containment::ContainmentValidatedExteriorMesherInput;
use crate::su2_mesh::{BoundarySource, DomainAxis, DomainSide};

/// TetGen switches paired with [`prepare_tetgen_plc`].
///
/// `-p` consumes the generated PLC, `-Y` prevents boundary facet splitting, `-z` keeps all output
/// indices zero-based, `-C` asks TetGen to check the final mesh, `-Q` keeps stdout concise, and `-I`
/// suppresses iteration suffixes so a private working directory has deterministic output names.
/// No quality or maximum-volume claim is made by this baseline switch set.
pub const TETGEN_BASELINE_SWITCHES: &str = "-pYzCQI";

/// Explicit bounded policy for finding one strictly interior volume-hole point per solid body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TetgenHoleSeedPolicy {
    /// Geometric boundary tolerance in source-mesh coordinate units. Must be finite and positive.
    pub geometric_epsilon: f64,
    /// Initial inward displacement as a fraction of the selected triangle's shortest edge.
    /// Must be finite and in `(0, 0.25]`.
    pub initial_inward_edge_fraction: f64,
    /// Maximum number of deterministic inward-offset attempts. Each failed attempt halves the
    /// previous displacement. Must be non-zero.
    pub max_attempts: usize,
    /// Explicit worst-case point/triangle work budget for winding validation of all hole seeds.
    pub max_point_triangle_tests: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TetgenHoleSeed {
    pub scene_object_id: u64,
    pub point: [f64; 3],
    pub source_triangle: usize,
    pub inward_offset: f64,
    pub attempts: usize,
}

/// Immutable prepared PLC text plus deterministic evidence needed by a later external TetGen runner.
#[derive(Clone, Debug, PartialEq)]
pub struct PreparedTetgenPlc {
    poly_text: String,
    switches: String,
    hole_seeds: Vec<TetgenHoleSeed>,
    point_count: usize,
    facet_count: usize,
    reserved_point_triangle_tests: usize,
    executed_point_triangle_tests: usize,
}

impl PreparedTetgenPlc {
    pub fn poly_text(&self) -> &str {
        &self.poly_text
    }

    pub fn switches(&self) -> &str {
        &self.switches
    }

    pub fn hole_seeds(&self) -> &[TetgenHoleSeed] {
        &self.hole_seeds
    }

    pub fn point_count(&self) -> usize {
        self.point_count
    }

    pub fn facet_count(&self) -> usize {
        self.facet_count
    }

    pub fn reserved_point_triangle_tests(&self) -> usize {
        self.reserved_point_triangle_tests
    }

    pub fn executed_point_triangle_tests(&self) -> usize {
        self.executed_point_triangle_tests
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TetgenPlcError {
    InvalidGeometricEpsilon { value: f64 },
    InvalidInitialInwardEdgeFraction { value: f64 },
    ZeroHoleSeedAttempts,
    ZeroPointTriangleBudget,
    WorkBudgetOverflow,
    WorkBudgetExceeded { requested: usize, limit: usize },
    PointIndexOverflow,
    FacetCountOverflow,
    MissingDomainMarker { axis: DomainAxis, side: DomainSide },
    MissingBodyMarker { scene_object_id: u64 },
    MarkerOutOfRange { marker: u32 },
    MissingUsableTriangle { scene_object_id: u64 },
    NonFiniteHoleSeedEvaluation { scene_object_id: u64 },
    HoleSeedNotFound {
        scene_object_id: u64,
        source_triangle: usize,
        attempts: usize,
    },
}

impl Display for TetgenPlcError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidGeometricEpsilon { value } => write!(
                f,
                "TetGen hole-seed epsilon must be finite and positive; got {value}"
            ),
            Self::InvalidInitialInwardEdgeFraction { value } => write!(
                f,
                "TetGen initial inward edge fraction must be finite and in (0, 0.25]; got {value}"
            ),
            Self::ZeroHoleSeedAttempts => write!(
                f,
                "TetGen hole-seed search requires at least one deterministic attempt"
            ),
            Self::ZeroPointTriangleBudget => write!(
                f,
                "TetGen hole-seed validation requires a non-zero point/triangle budget"
            ),
            Self::WorkBudgetOverflow => write!(
                f,
                "TetGen hole-seed point/triangle work reservation overflowed usize"
            ),
            Self::WorkBudgetExceeded { requested, limit } => write!(
                f,
                "TetGen hole-seed validation reserves {requested} point/triangle tests, above the explicit limit {limit}"
            ),
            Self::PointIndexOverflow => write!(
                f,
                "TetGen PLC point count exceeds AeroForge's u32 downstream index contract"
            ),
            Self::FacetCountOverflow => write!(f, "TetGen PLC facet count overflowed usize"),
            Self::MissingDomainMarker { axis, side } => write!(
                f,
                "TetGen PLC is missing authoritative marker provenance for domain face {axis:?} {side:?}"
            ),
            Self::MissingBodyMarker { scene_object_id } => write!(
                f,
                "TetGen PLC is missing authoritative wall marker provenance for SceneObject {scene_object_id}"
            ),
            Self::MarkerOutOfRange { marker } => write!(
                f,
                "TetGen PLC boundary marker {marker} exceeds signed 32-bit marker range"
            ),
            Self::MissingUsableTriangle { scene_object_id } => write!(
                f,
                "SceneObject {scene_object_id} has no finite non-degenerate triangle for deterministic hole-seed construction"
            ),
            Self::NonFiniteHoleSeedEvaluation { scene_object_id } => write!(
                f,
                "SceneObject {scene_object_id} produced a non-finite winding evaluation while locating a TetGen volume-hole point"
            ),
            Self::HoleSeedNotFound {
                scene_object_id,
                source_triangle,
                attempts,
            } => write!(
                f,
                "SceneObject {scene_object_id} did not yield a strictly interior TetGen volume-hole point from source triangle {source_triangle} within {attempts} bounded attempts"
            ),
        }
    }
}

impl Error for TetgenPlcError {}

/// Converts containment-admitted exterior geometry into a deterministic marked TetGen `.poly` PLC.
///
/// The outer domain is emitted as six marked quadrilateral facets. Every audited source triangle is
/// emitted unchanged as one marked internal facet using that SceneObject's authoritative wall marker.
/// One strictly interior volume-hole point is generated per solid body so TetGen removes the solid
/// volume and retains its surface as an exterior-fluid boundary.
///
/// Hole points are not guessed from an AABB center. For each positively oriented audited body, the
/// largest-area source triangle (stable first-index tie break) is selected, its outward normal is
/// reversed, and a candidate is displaced inward from the triangle centroid. Failed candidates are
/// retried only by deterministic halving. Every candidate is checked against the complete source
/// shell with a solid-angle winding test and explicit boundary tolerance. Worst-case winding work is
/// reserved before any search begins; no random sampling or silent work reduction occurs.
///
/// This function prepares input text only. It does not execute TetGen, parse a volume mesh, or make
/// any body-fitted/quality/CFD claim.
pub fn prepare_tetgen_plc(
    input: &ContainmentValidatedExteriorMesherInput,
    hole_seed_policy: TetgenHoleSeedPolicy,
) -> Result<PreparedTetgenPlc, TetgenPlcError> {
    validate_policy(hole_seed_policy)?;

    let admission = input.admission();
    let sources = admission.audited_sources();
    let marker_map = admission.marker_map();

    let source_point_count = sources.iter().try_fold(0_usize, |total, source| {
        total
            .checked_add(source.mesh.positions.len())
            .ok_or(TetgenPlcError::PointIndexOverflow)
    })?;
    let point_count = 8_usize
        .checked_add(source_point_count)
        .ok_or(TetgenPlcError::PointIndexOverflow)?;
    if point_count > u32::MAX as usize {
        return Err(TetgenPlcError::PointIndexOverflow);
    }

    let source_facet_count = sources.iter().try_fold(0_usize, |total, source| {
        total
            .checked_add(source.mesh.triangles.len())
            .ok_or(TetgenPlcError::FacetCountOverflow)
    })?;
    let facet_count = 6_usize
        .checked_add(source_facet_count)
        .ok_or(TetgenPlcError::FacetCountOverflow)?;

    let reserved_point_triangle_tests = sources.iter().try_fold(0_usize, |total, source| {
        let source_work = source
            .mesh
            .triangles
            .len()
            .checked_mul(hole_seed_policy.max_attempts)
            .ok_or(TetgenPlcError::WorkBudgetOverflow)?;
        total
            .checked_add(source_work)
            .ok_or(TetgenPlcError::WorkBudgetOverflow)
    })?;
    if reserved_point_triangle_tests > hole_seed_policy.max_point_triangle_tests {
        return Err(TetgenPlcError::WorkBudgetExceeded {
            requested: reserved_point_triangle_tests,
            limit: hole_seed_policy.max_point_triangle_tests,
        });
    }

    let domain_markers = [
        domain_marker(marker_map, DomainAxis::X, DomainSide::Min)?,
        domain_marker(marker_map, DomainAxis::X, DomainSide::Max)?,
        domain_marker(marker_map, DomainAxis::Y, DomainSide::Min)?,
        domain_marker(marker_map, DomainAxis::Y, DomainSide::Max)?,
        domain_marker(marker_map, DomainAxis::Z, DomainSide::Min)?,
        domain_marker(marker_map, DomainAxis::Z, DomainSide::Max)?,
    ];
    for marker in domain_markers {
        validate_tetgen_marker(marker)?;
    }
    let body_markers = sources
        .iter()
        .map(|source| {
            let marker = body_marker(marker_map, source.scene_object_id)?;
            validate_tetgen_marker(marker)?;
            Ok(marker)
        })
        .collect::<Result<Vec<_>, TetgenPlcError>>()?;

    let mut executed_point_triangle_tests = 0_usize;
    let mut hole_seeds = Vec::with_capacity(sources.len());
    for source in sources {
        hole_seeds.push(find_hole_seed(
            source,
            hole_seed_policy,
            &mut executed_point_triangle_tests,
        )?);
    }

    let domain_min = admission.domain_min();
    let domain_max = admission.domain_max();
    let domain_points = [
        [domain_min[0], domain_min[1], domain_min[2]],
        [domain_max[0], domain_min[1], domain_min[2]],
        [domain_max[0], domain_max[1], domain_min[2]],
        [domain_min[0], domain_max[1], domain_min[2]],
        [domain_min[0], domain_min[1], domain_max[2]],
        [domain_max[0], domain_min[1], domain_max[2]],
        [domain_max[0], domain_max[1], domain_max[2]],
        [domain_min[0], domain_max[1], domain_max[2]],
    ];

    let mut poly = String::new();
    poly.push_str(&format!("{point_count} 3 0 0\n"));
    for (index, point) in domain_points.iter().enumerate() {
        push_point(&mut poly, index, *point);
    }
    let mut point_offset = 8_usize;
    let mut source_offsets = Vec::with_capacity(sources.len());
    for source in sources {
        source_offsets.push(point_offset);
        for point in &source.mesh.positions {
            push_point(&mut poly, point_offset, *point);
            point_offset += 1;
        }
    }
    debug_assert_eq!(point_offset, point_count);

    poly.push_str(&format!("{facet_count} 1\n"));
    let domain_facets: [([usize; 4], BoundaryMarkerId); 6] = [
        ([0, 4, 7, 3], domain_markers[0]),
        ([1, 2, 6, 5], domain_markers[1]),
        ([0, 1, 5, 4], domain_markers[2]),
        ([3, 7, 6, 2], domain_markers[3]),
        ([0, 3, 2, 1], domain_markers[4]),
        ([4, 5, 6, 7], domain_markers[5]),
    ];
    for (vertices, marker) in domain_facets {
        push_facet(&mut poly, &vertices, marker);
    }
    for ((source, &offset), &marker) in sources
        .iter()
        .zip(source_offsets.iter())
        .zip(body_markers.iter())
    {
        for triangle in &source.mesh.triangles {
            let vertices = [
                offset + triangle[0] as usize,
                offset + triangle[1] as usize,
                offset + triangle[2] as usize,
            ];
            push_facet(&mut poly, &vertices, marker);
        }
    }

    poly.push_str(&format!("{}\n", hole_seeds.len()));
    for (index, seed) in hole_seeds.iter().enumerate() {
        poly.push_str(&format!(
            "{} {} {} {}\n",
            index,
            fmt_float(seed.point[0]),
            fmt_float(seed.point[1]),
            fmt_float(seed.point[2])
        ));
    }
    poly.push_str("0\n");

    Ok(PreparedTetgenPlc {
        poly_text: poly,
        switches: TETGEN_BASELINE_SWITCHES.to_owned(),
        hole_seeds,
        point_count,
        facet_count,
        reserved_point_triangle_tests,
        executed_point_triangle_tests,
    })
}

fn validate_policy(policy: TetgenHoleSeedPolicy) -> Result<(), TetgenPlcError> {
    if !policy.geometric_epsilon.is_finite() || policy.geometric_epsilon <= 0.0 {
        return Err(TetgenPlcError::InvalidGeometricEpsilon {
            value: policy.geometric_epsilon,
        });
    }
    if !policy.initial_inward_edge_fraction.is_finite()
        || policy.initial_inward_edge_fraction <= 0.0
        || policy.initial_inward_edge_fraction > 0.25
    {
        return Err(TetgenPlcError::InvalidInitialInwardEdgeFraction {
            value: policy.initial_inward_edge_fraction,
        });
    }
    if policy.max_attempts == 0 {
        return Err(TetgenPlcError::ZeroHoleSeedAttempts);
    }
    if policy.max_point_triangle_tests == 0 {
        return Err(TetgenPlcError::ZeroPointTriangleBudget);
    }
    Ok(())
}

fn domain_marker(
    marker_map: &crate::su2_mesh::Su2MarkerMap,
    axis: DomainAxis,
    side: DomainSide,
) -> Result<BoundaryMarkerId, TetgenPlcError> {
    marker_map
        .bindings
        .iter()
        .find_map(|binding| match &binding.source {
            BoundarySource::DomainFace {
                axis: candidate_axis,
                side: candidate_side,
            } if *candidate_axis == axis && *candidate_side == side => Some(binding.marker),
            _ => None,
        })
        .ok_or(TetgenPlcError::MissingDomainMarker { axis, side })
}

fn body_marker(
    marker_map: &crate::su2_mesh::Su2MarkerMap,
    scene_object_id: u64,
) -> Result<BoundaryMarkerId, TetgenPlcError> {
    marker_map
        .bindings
        .iter()
        .find_map(|binding| match &binding.source {
            BoundarySource::SceneObject {
                scene_object_id: candidate,
            } if *candidate == scene_object_id => Some(binding.marker),
            _ => None,
        })
        .ok_or(TetgenPlcError::MissingBodyMarker { scene_object_id })
}

fn validate_tetgen_marker(marker: BoundaryMarkerId) -> Result<(), TetgenPlcError> {
    if marker.0 > i32::MAX as u32 {
        Err(TetgenPlcError::MarkerOutOfRange { marker: marker.0 })
    } else {
        Ok(())
    }
}

fn push_point(out: &mut String, index: usize, point: [f64; 3]) {
    out.push_str(&format!(
        "{index} {} {} {}\n",
        fmt_float(point[0]),
        fmt_float(point[1]),
        fmt_float(point[2])
    ));
}

fn push_facet(out: &mut String, vertices: &[usize], marker: BoundaryMarkerId) {
    out.push_str(&format!("1 0 {}\n", marker.0));
    out.push_str(&vertices.len().to_string());
    for vertex in vertices {
        out.push(' ');
        out.push_str(&vertex.to_string());
    }
    out.push('\n');
}

fn fmt_float(value: f64) -> String {
    format!("{value:.17e}")
}

fn find_hole_seed(
    source: &AuditedImportedSurfaceBody,
    policy: TetgenHoleSeedPolicy,
    executed_tests: &mut usize,
) -> Result<TetgenHoleSeed, TetgenPlcError> {
    let (triangle_index, triangle, normal, shortest_edge) = select_seed_triangle(source)?;
    let [a, b, c] = triangle_points(source, triangle);
    let centroid = [
        (a[0] + b[0] + c[0]) / 3.0,
        (a[1] + b[1] + c[1]) / 3.0,
        (a[2] + b[2] + c[2]) / 3.0,
    ];
    let normal_length = norm(normal);
    let inward = [
        -normal[0] / normal_length,
        -normal[1] / normal_length,
        -normal[2] / normal_length,
    ];
    let initial_offset = shortest_edge * policy.initial_inward_edge_fraction;

    for attempt in 0..policy.max_attempts {
        let divisor = 2.0_f64.powi(i32::try_from(attempt).unwrap_or(i32::MAX));
        let offset = initial_offset / divisor;
        if !offset.is_finite() || offset <= policy.geometric_epsilon {
            break;
        }
        let candidate = [
            centroid[0] + inward[0] * offset,
            centroid[1] + inward[1] * offset,
            centroid[2] + inward[2] * offset,
        ];
        match classify_point(source, candidate, policy.geometric_epsilon, executed_tests) {
            Some(PointClassification::Inside) => {
                return Ok(TetgenHoleSeed {
                    scene_object_id: source.scene_object_id,
                    point: candidate,
                    source_triangle: triangle_index,
                    inward_offset: offset,
                    attempts: attempt + 1,
                });
            }
            Some(PointClassification::Outside | PointClassification::Boundary) => {}
            None => {
                return Err(TetgenPlcError::NonFiniteHoleSeedEvaluation {
                    scene_object_id: source.scene_object_id,
                });
            }
        }
    }

    Err(TetgenPlcError::HoleSeedNotFound {
        scene_object_id: source.scene_object_id,
        source_triangle: triangle_index,
        attempts: policy.max_attempts,
    })
}

fn select_seed_triangle(
    source: &AuditedImportedSurfaceBody,
) -> Result<(usize, [u32; 3], [f64; 3], f64), TetgenPlcError> {
    let mut best: Option<(usize, [u32; 3], [f64; 3], f64, f64)> = None;
    for (index, &triangle) in source.mesh.triangles.iter().enumerate() {
        let [a, b, c] = triangle_points(source, triangle);
        let ab = sub(b, a);
        let ac = sub(c, a);
        let bc = sub(c, b);
        let normal = cross(ab, ac);
        let area2 = norm(normal);
        let shortest_edge = norm(ab).min(norm(ac)).min(norm(bc));
        if !area2.is_finite()
            || area2 <= 0.0
            || !shortest_edge.is_finite()
            || shortest_edge <= 0.0
        {
            continue;
        }
        if best.as_ref().is_none_or(|candidate| area2 > candidate.4) {
            best = Some((index, triangle, normal, shortest_edge, area2));
        }
    }
    best.map(|(index, triangle, normal, shortest_edge, _)| {
        (index, triangle, normal, shortest_edge)
    })
    .ok_or(TetgenPlcError::MissingUsableTriangle {
        scene_object_id: source.scene_object_id,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PointClassification {
    Inside,
    Outside,
    Boundary,
}

fn classify_point(
    source: &AuditedImportedSurfaceBody,
    point: [f64; 3],
    epsilon: f64,
    executed_tests: &mut usize,
) -> Option<PointClassification> {
    let mut winding = 0.0_f64;
    for &triangle in &source.mesh.triangles {
        *executed_tests = executed_tests.checked_add(1)?;
        let [a, b, c] = triangle_points(source, triangle);
        if point_on_triangle(point, a, b, c, epsilon) {
            return Some(PointClassification::Boundary);
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
            return Some(PointClassification::Boundary);
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
        None
    } else if winding.abs() > 2.0 * PI {
        Some(PointClassification::Inside)
    } else {
        Some(PointClassification::Outside)
    }
}

fn triangle_points(
    source: &AuditedImportedSurfaceBody,
    triangle: [u32; 3],
) -> [[f64; 3]; 3] {
    [
        source.mesh.positions[triangle[0] as usize],
        source.mesh.positions[triangle[1] as usize],
        source.mesh.positions[triangle[2] as usize],
    ]
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
    use crate::source_containment::{
        validate_exterior_mesher_source_containment, SourceContainmentPolicy,
    };
    use crate::source_intersection::SourceSurfaceIntersectionPolicy;
    use crate::su2_mesh::{
        BoundaryRole, BoundarySource, DomainAxis, DomainSide, Su2MarkerBinding,
    };

    use super::*;

    fn tetra_surface(offset: [f64; 3]) -> SurfaceMesh {
        let [x, y, z] = offset;
        SurfaceMesh {
            positions: vec![
                [x, y, z],
                [x + 1.0, y, z],
                [x, y + 1.0, z],
                [x, y, z + 1.0],
            ],
            triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        }
    }

    fn audited(id: u64, offset: [f64; 3]) -> AuditedImportedSurfaceBody {
        audit_imported_surface_for_accurate_meshing(
            id,
            &tetra_surface(offset),
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

    fn containment_input() -> ContainmentValidatedExteriorMesherInput {
        let base = build_validated_exterior_mesher_input(
            [-1.0, -1.0, -1.0],
            [4.0, 4.0, 4.0],
            domain_bindings(),
            vec![audited(42, [0.0, 0.0, 0.0])],
        )
        .unwrap();
        let intersection = validate_exterior_mesher_input_intersections(
            base,
            SourceSurfaceIntersectionPolicy {
                geometric_epsilon: 1.0e-10,
                max_triangle_pair_tests: 1_000,
            },
        )
        .unwrap();
        validate_exterior_mesher_source_containment(
            intersection,
            SourceContainmentPolicy {
                geometric_epsilon: 1.0e-10,
                max_point_triangle_tests: 1_000,
            },
        )
        .unwrap()
    }

    fn seed_policy(limit: usize) -> TetgenHoleSeedPolicy {
        TetgenHoleSeedPolicy {
            geometric_epsilon: 1.0e-12,
            initial_inward_edge_fraction: 0.05,
            max_attempts: 12,
            max_point_triangle_tests: limit,
        }
    }

    #[test]
    fn deterministic_plc_preserves_domain_and_scene_markers() {
        let prepared = prepare_tetgen_plc(&containment_input(), seed_policy(1_000)).unwrap();

        assert_eq!(prepared.switches(), "-pYzCQI");
        assert_eq!(prepared.point_count(), 12);
        assert_eq!(prepared.facet_count(), 10);
        assert_eq!(prepared.hole_seeds().len(), 1);
        assert_eq!(prepared.hole_seeds()[0].scene_object_id, 42);
        assert!(prepared.executed_point_triangle_tests() > 0);
        assert_eq!(prepared.reserved_point_triangle_tests(), 48);

        let poly = prepared.poly_text();
        assert!(poly.starts_with("12 3 0 0\n"));
        assert!(poly.contains("10 1\n"));
        for marker in 1..=7 {
            assert!(poly.contains(&format!("1 0 {marker}\n")));
        }
        assert!(poly.ends_with("0\n"));
    }

    #[test]
    fn hole_seed_is_strictly_inside_source_shell() {
        let input = containment_input();
        let prepared = prepare_tetgen_plc(&input, seed_policy(1_000)).unwrap();
        let seed = prepared.hole_seeds()[0].point;
        let mut executed = 0;
        assert_eq!(
            classify_point(
                &input.admission().audited_sources()[0],
                seed,
                1.0e-12,
                &mut executed
            ),
            Some(PointClassification::Inside)
        );
    }

    #[test]
    fn hole_seed_work_budget_is_preflighted() {
        let error = prepare_tetgen_plc(&containment_input(), seed_policy(47)).unwrap_err();
        assert_eq!(
            error,
            TetgenPlcError::WorkBudgetExceeded {
                requested: 48,
                limit: 47,
            }
        );
    }

    #[test]
    fn invalid_seed_policy_fails_closed() {
        let mut policy = seed_policy(1_000);
        policy.initial_inward_edge_fraction = 0.5;
        assert!(matches!(
            prepare_tetgen_plc(&containment_input(), policy),
            Err(TetgenPlcError::InvalidInitialInwardEdgeFraction { .. })
        ));
    }
}
