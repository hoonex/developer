use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::{SurfaceMesh, TopologyReport};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RepairPolicy {
    /// Maximum Euclidean distance for vertex welding. `0.0` means exact-coordinate welding only.
    /// Units are the imported mesh units; AeroForge deliberately does not guess a physical tolerance.
    pub weld_tolerance: f64,
    pub drop_degenerate_triangles: bool,
    pub drop_duplicate_triangles: bool,
    pub orient_manifold_components: bool,
    pub require_watertight: bool,
}

impl Default for RepairPolicy {
    fn default() -> Self {
        Self {
            weld_tolerance: 0.0,
            drop_degenerate_triangles: true,
            drop_duplicate_triangles: true,
            orient_manifold_components: true,
            require_watertight: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RepairReport {
    pub input_vertices: usize,
    pub output_vertices: usize,
    pub welded_vertices: usize,
    pub input_triangles: usize,
    pub output_triangles: usize,
    pub dropped_degenerate_triangles: usize,
    pub dropped_duplicate_triangles: usize,
    pub flipped_triangles: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RepairedSurface {
    pub mesh: SurfaceMesh,
    pub repair: RepairReport,
    pub topology: TopologyReport,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RepairError {
    InvalidTolerance,
    EmptyMesh,
    NonFiniteVertex { vertex: usize },
    IndexOutOfBounds { triangle: usize, index: u32 },
    QuantizationOverflow { vertex: usize, axis: usize },
    DegenerateTriangles { count: usize },
    DuplicateTriangles { count: usize },
    NonManifoldEdges { count: usize },
    NonOrientableComponent,
    OpenBoundary { edges: usize },
    InconsistentOrientation { edges: usize },
    Topology(String),
    VertexIndexOverflow,
}

impl Display for RepairError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTolerance => write!(f, "weld tolerance must be finite and non-negative"),
            Self::EmptyMesh => write!(f, "surface repair requires vertices and triangles"),
            Self::NonFiniteVertex { vertex } => write!(f, "vertex {vertex} is not finite"),
            Self::IndexOutOfBounds { triangle, index } => {
                write!(f, "triangle {triangle} references missing vertex {index}")
            }
            Self::QuantizationOverflow { vertex, axis } => write!(
                f,
                "vertex {vertex} axis {axis} is too large for the requested weld tolerance"
            ),
            Self::DegenerateTriangles { count } => {
                write!(f, "surface contains {count} degenerate triangles")
            }
            Self::DuplicateTriangles { count } => {
                write!(f, "surface contains {count} duplicate triangles")
            }
            Self::NonManifoldEdges { count } => write!(
                f,
                "surface contains {count} non-manifold edges; automatic orientation repair is refused"
            ),
            Self::NonOrientableComponent => write!(
                f,
                "surface orientation constraints are contradictory; component cannot be oriented consistently"
            ),
            Self::OpenBoundary { edges } => {
                write!(f, "surface has {edges} open boundary edges")
            }
            Self::InconsistentOrientation { edges } => {
                write!(f, "surface has {edges} inconsistently oriented shared edges")
            }
            Self::Topology(message) => write!(f, "topology audit failed after repair: {message}"),
            Self::VertexIndexOverflow => write!(f, "repaired surface exceeds u32 vertex indexing"),
        }
    }
}

impl Error for RepairError {}

/// Performs a bounded, deterministic topology repair pass.
///
/// The pass can weld coincident/nearby vertices, remove degenerate and duplicate triangles,
/// and make each manifold connected component consistently oriented. Closed components are
/// additionally flipped as a whole when necessary so their signed volume is positive.
///
/// This is *not* a self-intersection repair or CAD healing algorithm. A topologically watertight
/// two-manifold can still self-intersect or otherwise be unsuitable for volume meshing; later
/// volume-mesh validation must check those geometric conditions explicitly.
pub fn repair_surface(
    input: &SurfaceMesh,
    policy: RepairPolicy,
) -> Result<RepairedSurface, RepairError> {
    validate_input(input, policy.weld_tolerance)?;

    let input_vertices = input.positions.len();
    let input_triangles = input.triangles.len();
    let (positions, remap) = weld_vertices(&input.positions, policy.weld_tolerance)?;

    let mut triangles = Vec::with_capacity(input.triangles.len());
    let mut dropped_degenerate_triangles = 0_usize;
    for triangle in &input.triangles {
        let mapped = [
            remap[triangle[0] as usize],
            remap[triangle[1] as usize],
            remap[triangle[2] as usize],
        ];
        if triangle_is_degenerate(&positions, mapped) {
            dropped_degenerate_triangles += 1;
            if !policy.drop_degenerate_triangles {
                continue;
            }
            continue;
        }
        triangles.push(mapped);
    }
    if dropped_degenerate_triangles > 0 && !policy.drop_degenerate_triangles {
        return Err(RepairError::DegenerateTriangles {
            count: dropped_degenerate_triangles,
        });
    }

    let mut unique = HashSet::<[u32; 3]>::with_capacity(triangles.len());
    let mut deduplicated = Vec::with_capacity(triangles.len());
    let mut dropped_duplicate_triangles = 0_usize;
    for triangle in triangles {
        let mut canonical = triangle;
        canonical.sort_unstable();
        if !unique.insert(canonical) {
            dropped_duplicate_triangles += 1;
            if policy.drop_duplicate_triangles {
                continue;
            }
        }
        deduplicated.push(triangle);
    }
    if dropped_duplicate_triangles > 0 && !policy.drop_duplicate_triangles {
        return Err(RepairError::DuplicateTriangles {
            count: dropped_duplicate_triangles,
        });
    }
    if deduplicated.is_empty() {
        return Err(RepairError::EmptyMesh);
    }

    let mut mesh = SurfaceMesh {
        positions,
        triangles: deduplicated,
    };
    let edge_uses = build_edge_uses(&mesh.triangles);
    let non_manifold_edges = edge_uses.values().filter(|uses| uses.len() > 2).count();
    if non_manifold_edges > 0 {
        return Err(RepairError::NonManifoldEdges {
            count: non_manifold_edges,
        });
    }

    let mut flipped_triangles = 0_usize;
    if policy.orient_manifold_components {
        flipped_triangles += orient_consistently(&mut mesh, &edge_uses)?;
        flipped_triangles += orient_closed_components_outward(&mut mesh)?;
    }

    let topology = mesh
        .topology_report()
        .map_err(|error| RepairError::Topology(error.to_string()))?;
    if policy.require_watertight {
        if topology.boundary_edges > 0 {
            return Err(RepairError::OpenBoundary {
                edges: topology.boundary_edges,
            });
        }
        if topology.inconsistent_oriented_edges > 0 {
            return Err(RepairError::InconsistentOrientation {
                edges: topology.inconsistent_oriented_edges,
            });
        }
        if !topology.watertight_two_manifold || !topology.consistently_oriented {
            return Err(RepairError::Topology(
                "surface did not satisfy the required watertight two-manifold contract".into(),
            ));
        }
    }

    Ok(RepairedSurface {
        repair: RepairReport {
            input_vertices,
            output_vertices: mesh.positions.len(),
            welded_vertices: input_vertices.saturating_sub(mesh.positions.len()),
            input_triangles,
            output_triangles: mesh.triangles.len(),
            dropped_degenerate_triangles,
            dropped_duplicate_triangles,
            flipped_triangles,
        },
        mesh,
        topology,
    })
}

fn validate_input(mesh: &SurfaceMesh, tolerance: f64) -> Result<(), RepairError> {
    if !tolerance.is_finite() || tolerance < 0.0 {
        return Err(RepairError::InvalidTolerance);
    }
    if mesh.positions.is_empty() || mesh.triangles.is_empty() {
        return Err(RepairError::EmptyMesh);
    }
    for (vertex, position) in mesh.positions.iter().enumerate() {
        if !position.iter().all(|value| value.is_finite()) {
            return Err(RepairError::NonFiniteVertex { vertex });
        }
    }
    for (triangle_index, triangle) in mesh.triangles.iter().enumerate() {
        for &index in triangle {
            if index as usize >= mesh.positions.len() {
                return Err(RepairError::IndexOutOfBounds {
                    triangle: triangle_index,
                    index,
                });
            }
        }
    }
    Ok(())
}

fn weld_vertices(
    input: &[[f64; 3]],
    tolerance: f64,
) -> Result<(Vec<[f64; 3]>, Vec<u32>), RepairError> {
    if tolerance == 0.0 {
        let mut positions = Vec::new();
        let mut exact = HashMap::<(u64, u64, u64), u32>::new();
        let mut remap = Vec::with_capacity(input.len());
        for &position in input {
            let key = (
                position[0].to_bits(),
                position[1].to_bits(),
                position[2].to_bits(),
            );
            let index = if let Some(&existing) = exact.get(&key) {
                existing
            } else {
                let index = u32::try_from(positions.len())
                    .map_err(|_| RepairError::VertexIndexOverflow)?;
                positions.push(position);
                exact.insert(key, index);
                index
            };
            remap.push(index);
        }
        return Ok((positions, remap));
    }

    let tolerance_sq = tolerance * tolerance;
    let mut positions = Vec::<[f64; 3]>::new();
    let mut bins = HashMap::<(i64, i64, i64), Vec<u32>>::new();
    let mut remap = Vec::with_capacity(input.len());

    for (vertex, &position) in input.iter().enumerate() {
        let key = quantize(position, tolerance, vertex)?;
        let mut best = None::<u32>;
        for dx in -1_i64..=1 {
            for dy in -1_i64..=1 {
                for dz in -1_i64..=1 {
                    let neighbor = (
                        key.0.saturating_add(dx),
                        key.1.saturating_add(dy),
                        key.2.saturating_add(dz),
                    );
                    if let Some(candidates) = bins.get(&neighbor) {
                        for &candidate in candidates {
                            if distance_squared(position, positions[candidate as usize])
                                <= tolerance_sq
                            {
                                best = Some(best.map_or(candidate, |current| current.min(candidate)));
                            }
                        }
                    }
                }
            }
        }
        let index = if let Some(existing) = best {
            existing
        } else {
            let index = u32::try_from(positions.len())
                .map_err(|_| RepairError::VertexIndexOverflow)?;
            positions.push(position);
            bins.entry(key).or_default().push(index);
            index
        };
        remap.push(index);
    }

    Ok((positions, remap))
}

fn quantize(
    position: [f64; 3],
    tolerance: f64,
    vertex: usize,
) -> Result<(i64, i64, i64), RepairError> {
    let mut quantized = [0_i64; 3];
    for axis in 0..3 {
        let value = (position[axis] / tolerance).floor();
        if value < i64::MIN as f64 || value > i64::MAX as f64 {
            return Err(RepairError::QuantizationOverflow { vertex, axis });
        }
        quantized[axis] = value as i64;
    }
    Ok((quantized[0], quantized[1], quantized[2]))
}

fn triangle_is_degenerate(positions: &[[f64; 3]], triangle: [u32; 3]) -> bool {
    if triangle[0] == triangle[1]
        || triangle[1] == triangle[2]
        || triangle[2] == triangle[0]
    {
        return true;
    }
    triangle_area_squared(
        positions[triangle[0] as usize],
        positions[triangle[1] as usize],
        positions[triangle[2] as usize],
    ) <= 1.0e-24
}

#[derive(Clone, Copy, Debug)]
struct DirectedEdgeUse {
    triangle: usize,
    direction: i8,
}

fn build_edge_uses(triangles: &[[u32; 3]]) -> BTreeMap<(u32, u32), Vec<DirectedEdgeUse>> {
    let mut edges = BTreeMap::<(u32, u32), Vec<DirectedEdgeUse>>::new();
    for (triangle_index, triangle) in triangles.iter().enumerate() {
        for [a, b] in [
            [triangle[0], triangle[1]],
            [triangle[1], triangle[2]],
            [triangle[2], triangle[0]],
        ] {
            let (key, direction) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
            edges.entry(key).or_default().push(DirectedEdgeUse {
                triangle: triangle_index,
                direction,
            });
        }
    }
    edges
}

fn orient_consistently(
    mesh: &mut SurfaceMesh,
    edge_uses: &BTreeMap<(u32, u32), Vec<DirectedEdgeUse>>,
) -> Result<usize, RepairError> {
    let mut adjacency = vec![Vec::<(usize, bool)>::new(); mesh.triangles.len()];
    for uses in edge_uses.values() {
        if uses.len() != 2 {
            continue;
        }
        let a = uses[0];
        let b = uses[1];
        // If both triangles traverse the shared edge in the same direction, exactly one must flip.
        let different_flip = a.direction == b.direction;
        adjacency[a.triangle].push((b.triangle, different_flip));
        adjacency[b.triangle].push((a.triangle, different_flip));
    }

    let mut flip = vec![None::<bool>; mesh.triangles.len()];
    for start in 0..mesh.triangles.len() {
        if flip[start].is_some() {
            continue;
        }
        flip[start] = Some(false);
        let mut queue = VecDeque::from([start]);
        while let Some(current) = queue.pop_front() {
            let current_flip = flip[current].unwrap();
            for &(neighbor, different_flip) in &adjacency[current] {
                let required = current_flip ^ different_flip;
                match flip[neighbor] {
                    Some(existing) if existing != required => {
                        return Err(RepairError::NonOrientableComponent);
                    }
                    Some(_) => {}
                    None => {
                        flip[neighbor] = Some(required);
                        queue.push_back(neighbor);
                    }
                }
            }
        }
    }

    let mut count = 0_usize;
    for (triangle, should_flip) in mesh.triangles.iter_mut().zip(flip) {
        if should_flip.unwrap_or(false) {
            triangle.swap(1, 2);
            count += 1;
        }
    }
    Ok(count)
}

fn orient_closed_components_outward(mesh: &mut SurfaceMesh) -> Result<usize, RepairError> {
    let edge_uses = build_edge_uses(&mesh.triangles);
    let mut adjacency = vec![Vec::<usize>::new(); mesh.triangles.len()];
    for uses in edge_uses.values() {
        if uses.len() == 2 {
            let a = uses[0].triangle;
            let b = uses[1].triangle;
            adjacency[a].push(b);
            adjacency[b].push(a);
        }
    }

    let mut component = vec![usize::MAX; mesh.triangles.len()];
    let mut components = Vec::<Vec<usize>>::new();
    for start in 0..mesh.triangles.len() {
        if component[start] != usize::MAX {
            continue;
        }
        let component_id = components.len();
        let mut members = Vec::new();
        let mut queue = VecDeque::from([start]);
        component[start] = component_id;
        while let Some(current) = queue.pop_front() {
            members.push(current);
            for &neighbor in &adjacency[current] {
                if component[neighbor] == usize::MAX {
                    component[neighbor] = component_id;
                    queue.push_back(neighbor);
                }
            }
        }
        components.push(members);
    }

    let mut boundary_by_component = vec![false; components.len()];
    for uses in edge_uses.values() {
        if uses.len() == 1 {
            boundary_by_component[component[uses[0].triangle]] = true;
        }
    }

    let mut flipped = 0_usize;
    for (component_id, members) in components.iter().enumerate() {
        if boundary_by_component[component_id] {
            continue;
        }
        let signed_volume = members
            .iter()
            .map(|&triangle_index| {
                let triangle = mesh.triangles[triangle_index];
                let a = mesh.positions[triangle[0] as usize];
                let b = mesh.positions[triangle[1] as usize];
                let c = mesh.positions[triangle[2] as usize];
                dot(a, cross(b, c)) / 6.0
            })
            .sum::<f64>();
        if signed_volume < 0.0 {
            for &triangle_index in members {
                mesh.triangles[triangle_index].swap(1, 2);
                flipped += 1;
            }
        }
    }
    Ok(flipped)
}

fn distance_squared(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}

fn triangle_area_squared(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let normal = cross(ab, ac);
    dot(normal, normal) * 0.25
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

    fn tetrahedron() -> SurfaceMesh {
        SurfaceMesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        }
    }

    #[test]
    fn duplicate_face_is_removed_before_manifold_audit() {
        let mut mesh = tetrahedron();
        mesh.triangles.push([0, 2, 1]);
        let repaired = repair_surface(
            &mesh,
            RepairPolicy {
                require_watertight: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(repaired.repair.dropped_duplicate_triangles, 1);
        assert_eq!(repaired.mesh.triangles.len(), 4);
        assert!(repaired.topology.watertight_two_manifold);
        assert!(repaired.topology.consistently_oriented);
    }

    #[test]
    fn one_reversed_face_is_reoriented_and_closed_volume_is_positive() {
        let mut mesh = tetrahedron();
        mesh.triangles[0].swap(1, 2);
        let repaired = repair_surface(
            &mesh,
            RepairPolicy {
                require_watertight: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(repaired.repair.flipped_triangles > 0);
        assert!(repaired.topology.consistently_oriented);
        assert!(repaired.topology.signed_volume.unwrap() > 0.0);
    }

    #[test]
    fn fully_inward_tetrahedron_is_flipped_outward() {
        let mut mesh = tetrahedron();
        for triangle in &mut mesh.triangles {
            triangle.swap(1, 2);
        }
        let repaired = repair_surface(
            &mesh,
            RepairPolicy {
                require_watertight: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(repaired.repair.flipped_triangles, 4);
        assert!((repaired.topology.signed_volume.unwrap() - 1.0 / 6.0).abs() < 1.0e-12);
    }

    #[test]
    fn missing_face_fails_when_watertight_is_required() {
        let mut mesh = tetrahedron();
        mesh.triangles.pop();
        let error = repair_surface(
            &mesh,
            RepairPolicy {
                require_watertight: true,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(error, RepairError::OpenBoundary { edges: 3 });
    }

    #[test]
    fn three_faces_sharing_one_edge_fail_closed_as_non_manifold() {
        let mesh = SurfaceMesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.0, -1.0, 0.0],
            ],
            triangles: vec![[0, 1, 2], [1, 0, 3], [0, 1, 4]],
        };
        let error = repair_surface(&mesh, RepairPolicy::default()).unwrap_err();
        assert_eq!(error, RepairError::NonManifoldEdges { count: 1 });
    }

    #[test]
    fn explicit_tolerance_welds_nearby_vertices_deterministically() {
        let mesh = SurfaceMesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0005, 0.0, 0.0],
            ],
            triangles: vec![[0, 1, 2], [3, 2, 1]],
        };
        let repaired = repair_surface(
            &mesh,
            RepairPolicy {
                weld_tolerance: 0.001,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(repaired.repair.welded_vertices, 1);
        assert_eq!(repaired.repair.dropped_duplicate_triangles, 1);
        assert_eq!(repaired.mesh.positions.len(), 3);
        assert_eq!(repaired.mesh.triangles.len(), 1);
    }

    #[test]
    fn exact_weld_has_no_hidden_physical_tolerance() {
        let mesh = SurfaceMesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            triangles: vec![[0, 2, 3], [1, 3, 2]],
        };
        let repaired = repair_surface(&mesh, RepairPolicy::default()).unwrap();
        assert_eq!(repaired.repair.welded_vertices, 1);
        assert_eq!(repaired.mesh.positions.len(), 3);
    }
}
