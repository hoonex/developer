pub mod gltf_import;
pub mod repair;

use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SurfaceMesh {
    pub positions: Vec<[f64; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceFormat {
    StlAscii,
    StlBinary,
    Obj,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImportedSurface {
    pub mesh: SurfaceMesh,
    pub format: SurfaceFormat,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshBounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TopologyReport {
    pub vertices: usize,
    pub triangles: usize,
    pub degenerate_triangles: usize,
    pub boundary_edges: usize,
    pub non_manifold_edges: usize,
    pub inconsistent_oriented_edges: usize,
    pub connected_components: usize,
    pub watertight_two_manifold: bool,
    pub consistently_oriented: bool,
    pub signed_volume: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeshError {
    EmptyMesh,
    NonFiniteVertex { vertex: usize },
    IndexOutOfBounds { triangle: usize, index: u32 },
    InvalidUtf8,
    Parse { line: Option<usize>, message: String },
    StlLengthOverflow,
}

impl Display for MeshError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyMesh => write!(f, "surface mesh is empty"),
            Self::NonFiniteVertex { vertex } => write!(f, "vertex {vertex} is not finite"),
            Self::IndexOutOfBounds { triangle, index } => {
                write!(f, "triangle {triangle} references missing vertex {index}")
            }
            Self::InvalidUtf8 => write!(f, "text surface file is not valid UTF-8"),
            Self::Parse { line, message } => match line {
                Some(line) => write!(f, "parse error on line {line}: {message}"),
                None => write!(f, "parse error: {message}"),
            },
            Self::StlLengthOverflow => write!(f, "binary STL triangle count overflows input length"),
        }
    }
}

impl Error for MeshError {}

impl SurfaceMesh {
    pub fn bounds(&self) -> Result<MeshBounds, MeshError> {
        self.validate_vertices()?;
        let Some(&first) = self.positions.first() else {
            return Err(MeshError::EmptyMesh);
        };
        let mut min = first;
        let mut max = first;
        for position in &self.positions[1..] {
            for axis in 0..3 {
                min[axis] = min[axis].min(position[axis]);
                max[axis] = max[axis].max(position[axis]);
            }
        }
        Ok(MeshBounds { min, max })
    }

    pub fn topology_report(&self) -> Result<TopologyReport, MeshError> {
        if self.positions.is_empty() || self.triangles.is_empty() {
            return Err(MeshError::EmptyMesh);
        }
        self.validate_vertices()?;
        self.validate_indices()?;

        #[derive(Default)]
        struct EdgeUse {
            triangles: Vec<usize>,
            orientation_sum: i32,
        }

        let mut edge_uses: BTreeMap<(u32, u32), EdgeUse> = BTreeMap::new();
        let mut degenerate_triangles = 0_usize;
        let mut active_triangle = vec![true; self.triangles.len()];

        for (triangle_index, &triangle) in self.triangles.iter().enumerate() {
            if triangle[0] == triangle[1]
                || triangle[1] == triangle[2]
                || triangle[2] == triangle[0]
                || triangle_area_squared(self.positions[triangle[0] as usize], self.positions[triangle[1] as usize], self.positions[triangle[2] as usize]) <= 1.0e-24
            {
                degenerate_triangles += 1;
                active_triangle[triangle_index] = false;
                continue;
            }

            for [a, b] in [
                [triangle[0], triangle[1]],
                [triangle[1], triangle[2]],
                [triangle[2], triangle[0]],
            ] {
                let (key, direction) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
                let entry = edge_uses.entry(key).or_default();
                entry.triangles.push(triangle_index);
                entry.orientation_sum += direction;
            }
        }

        let boundary_edges = edge_uses.values().filter(|edge| edge.triangles.len() == 1).count();
        let non_manifold_edges = edge_uses.values().filter(|edge| edge.triangles.len() > 2).count();
        let inconsistent_oriented_edges = edge_uses
            .values()
            .filter(|edge| edge.triangles.len() == 2 && edge.orientation_sum != 0)
            .count();

        let mut adjacency = vec![Vec::<usize>::new(); self.triangles.len()];
        for edge in edge_uses.values() {
            if let Some((&first, rest)) = edge.triangles.split_first() {
                for &other in rest {
                    adjacency[first].push(other);
                    adjacency[other].push(first);
                }
            }
        }
        let connected_components = count_components(&adjacency, &active_triangle);

        let watertight_two_manifold = degenerate_triangles == 0
            && boundary_edges == 0
            && non_manifold_edges == 0
            && !edge_uses.is_empty();
        let consistently_oriented = watertight_two_manifold && inconsistent_oriented_edges == 0;
        let signed_volume = consistently_oriented.then(|| self.signed_volume_unchecked());

        Ok(TopologyReport {
            vertices: self.positions.len(),
            triangles: self.triangles.len(),
            degenerate_triangles,
            boundary_edges,
            non_manifold_edges,
            inconsistent_oriented_edges,
            connected_components,
            watertight_two_manifold,
            consistently_oriented,
            signed_volume,
        })
    }

    fn validate_vertices(&self) -> Result<(), MeshError> {
        for (vertex, position) in self.positions.iter().enumerate() {
            if !position.iter().all(|value| value.is_finite()) {
                return Err(MeshError::NonFiniteVertex { vertex });
            }
        }
        Ok(())
    }

    fn validate_indices(&self) -> Result<(), MeshError> {
        for (triangle_index, triangle) in self.triangles.iter().enumerate() {
            for &index in triangle {
                if index as usize >= self.positions.len() {
                    return Err(MeshError::IndexOutOfBounds {
                        triangle: triangle_index,
                        index,
                    });
                }
            }
        }
        Ok(())
    }

    fn signed_volume_unchecked(&self) -> f64 {
        self.triangles
            .iter()
            .map(|triangle| {
                let a = self.positions[triangle[0] as usize];
                let b = self.positions[triangle[1] as usize];
                let c = self.positions[triangle[2] as usize];
                dot(a, cross(b, c)) / 6.0
            })
            .sum()
    }
}

pub fn import_stl(bytes: &[u8]) -> Result<ImportedSurface, MeshError> {
    if let Some(triangle_count) = binary_stl_triangle_count(bytes)? {
        return Ok(ImportedSurface {
            mesh: parse_binary_stl(bytes, triangle_count)?,
            format: SurfaceFormat::StlBinary,
        });
    }

    let text = std::str::from_utf8(bytes).map_err(|_| MeshError::InvalidUtf8)?;
    Ok(ImportedSurface {
        mesh: parse_ascii_stl(text)?,
        format: SurfaceFormat::StlAscii,
    })
}

pub fn import_obj(bytes: &[u8]) -> Result<ImportedSurface, MeshError> {
    let text = std::str::from_utf8(bytes).map_err(|_| MeshError::InvalidUtf8)?;
    let mut positions = Vec::<[f64; 3]>::new();
    let mut triangles = Vec::<[u32; 3]>::new();

    for (line_index, raw_line) in text.lines().enumerate() {
        let line_number = line_index + 1;
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split_whitespace();
        let Some(kind) = fields.next() else {
            continue;
        };
        match kind {
            "v" => {
                let x = parse_f64(fields.next(), line_number, "vertex x")?;
                let y = parse_f64(fields.next(), line_number, "vertex y")?;
                let z = parse_f64(fields.next(), line_number, "vertex z")?;
                if ![x, y, z].iter().all(|value| value.is_finite()) {
                    return Err(MeshError::NonFiniteVertex {
                        vertex: positions.len(),
                    });
                }
                positions.push([x, y, z]);
            }
            "f" => {
                let face = fields
                    .map(|field| parse_obj_vertex_index(field, positions.len(), line_number))
                    .collect::<Result<Vec<_>, _>>()?;
                if face.len() < 3 {
                    return Err(MeshError::Parse {
                        line: Some(line_number),
                        message: "OBJ face must contain at least three vertices".into(),
                    });
                }
                for index in 1..face.len() - 1 {
                    triangles.push([face[0], face[index], face[index + 1]]);
                }
            }
            _ => {}
        }
    }

    if positions.is_empty() || triangles.is_empty() {
        return Err(MeshError::EmptyMesh);
    }
    let mesh = SurfaceMesh {
        positions,
        triangles,
    };
    mesh.validate_vertices()?;
    mesh.validate_indices()?;
    Ok(ImportedSurface {
        mesh,
        format: SurfaceFormat::Obj,
    })
}

fn binary_stl_triangle_count(bytes: &[u8]) -> Result<Option<usize>, MeshError> {
    if bytes.len() < 84 {
        return Ok(None);
    }
    let triangle_count = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
    let payload = triangle_count
        .checked_mul(50)
        .and_then(|bytes| bytes.checked_add(84))
        .ok_or(MeshError::StlLengthOverflow)?;
    Ok((payload == bytes.len()).then_some(triangle_count))
}

fn parse_binary_stl(bytes: &[u8], triangle_count: usize) -> Result<SurfaceMesh, MeshError> {
    let mut positions = Vec::<[f64; 3]>::new();
    let mut triangles = Vec::<[u32; 3]>::with_capacity(triangle_count);
    let mut interned = HashMap::<(u32, u32, u32), u32>::new();

    for triangle_index in 0..triangle_count {
        let record = 84 + triangle_index * 50;
        let mut triangle = [0_u32; 3];
        for (corner, vertex_offset) in [12_usize, 24, 36].into_iter().enumerate() {
            let offset = record + vertex_offset;
            let xyz = [
                f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()),
                f32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()),
                f32::from_le_bytes(bytes[offset + 8..offset + 12].try_into().unwrap()),
            ];
            if !xyz.iter().all(|value| value.is_finite()) {
                return Err(MeshError::NonFiniteVertex {
                    vertex: positions.len(),
                });
            }
            let key = (xyz[0].to_bits(), xyz[1].to_bits(), xyz[2].to_bits());
            triangle[corner] = *interned.entry(key).or_insert_with(|| {
                let index = u32::try_from(positions.len()).expect("surface vertex count exceeds u32");
                positions.push([xyz[0] as f64, xyz[1] as f64, xyz[2] as f64]);
                index
            });
        }
        triangles.push(triangle);
    }

    if positions.is_empty() || triangles.is_empty() {
        return Err(MeshError::EmptyMesh);
    }
    Ok(SurfaceMesh {
        positions,
        triangles,
    })
}

fn parse_ascii_stl(text: &str) -> Result<SurfaceMesh, MeshError> {
    let mut positions = Vec::<[f64; 3]>::new();
    let mut triangles = Vec::<[u32; 3]>::new();
    let mut interned = HashMap::<(u64, u64, u64), u32>::new();
    let mut pending = Vec::<u32>::with_capacity(3);

    for (line_index, raw_line) in text.lines().enumerate() {
        let line_number = line_index + 1;
        let mut fields = raw_line.split_whitespace();
        if fields.next() != Some("vertex") {
            continue;
        }
        let x = parse_f64(fields.next(), line_number, "STL vertex x")?;
        let y = parse_f64(fields.next(), line_number, "STL vertex y")?;
        let z = parse_f64(fields.next(), line_number, "STL vertex z")?;
        if ![x, y, z].iter().all(|value| value.is_finite()) {
            return Err(MeshError::NonFiniteVertex {
                vertex: positions.len(),
            });
        }
        let key = (x.to_bits(), y.to_bits(), z.to_bits());
        let index = *interned.entry(key).or_insert_with(|| {
            let index = u32::try_from(positions.len()).expect("surface vertex count exceeds u32");
            positions.push([x, y, z]);
            index
        });
        pending.push(index);
        if pending.len() == 3 {
            triangles.push([pending[0], pending[1], pending[2]]);
            pending.clear();
        }
    }

    if !pending.is_empty() {
        return Err(MeshError::Parse {
            line: None,
            message: "ASCII STL ended with an incomplete triangle".into(),
        });
    }
    if positions.is_empty() || triangles.is_empty() {
        return Err(MeshError::EmptyMesh);
    }
    Ok(SurfaceMesh {
        positions,
        triangles,
    })
}

fn parse_obj_vertex_index(field: &str, vertex_count: usize, line: usize) -> Result<u32, MeshError> {
    let raw = field.split('/').next().unwrap_or("");
    let index = raw.parse::<i64>().map_err(|_| MeshError::Parse {
        line: Some(line),
        message: format!("invalid OBJ face vertex index `{raw}`"),
    })?;
    if index == 0 {
        return Err(MeshError::Parse {
            line: Some(line),
            message: "OBJ indices are 1-based and cannot be zero".into(),
        });
    }
    let resolved = if index > 0 {
        index - 1
    } else {
        vertex_count as i64 + index
    };
    if resolved < 0 || resolved >= vertex_count as i64 {
        return Err(MeshError::Parse {
            line: Some(line),
            message: format!("OBJ face index {index} is outside {vertex_count} vertices"),
        });
    }
    u32::try_from(resolved).map_err(|_| MeshError::Parse {
        line: Some(line),
        message: "OBJ vertex index exceeds u32".into(),
    })
}

fn parse_f64(value: Option<&str>, line: usize, field: &str) -> Result<f64, MeshError> {
    let value = value.ok_or_else(|| MeshError::Parse {
        line: Some(line),
        message: format!("missing {field}"),
    })?;
    value.parse::<f64>().map_err(|_| MeshError::Parse {
        line: Some(line),
        message: format!("invalid {field} `{value}`"),
    })
}

fn triangle_area_squared(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let cross = cross(ab, ac);
    dot(cross, cross) * 0.25
}

fn count_components(adjacency: &[Vec<usize>], active: &[bool]) -> usize {
    let mut visited = vec![false; adjacency.len()];
    let mut components = 0_usize;
    for start in 0..adjacency.len() {
        if !active[start] || visited[start] {
            continue;
        }
        components += 1;
        let mut stack = vec![start];
        visited[start] = true;
        while let Some(node) = stack.pop() {
            for &neighbor in &adjacency[node] {
                if active[neighbor] && !visited[neighbor] {
                    visited[neighbor] = true;
                    stack.push(neighbor);
                }
            }
        }
    }
    components
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
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
    fn closed_tetrahedron_is_watertight_and_oriented() {
        let mesh = tetrahedron();
        let report = mesh.topology_report().unwrap();
        assert_eq!(report.boundary_edges, 0);
        assert_eq!(report.non_manifold_edges, 0);
        assert_eq!(report.inconsistent_oriented_edges, 0);
        assert_eq!(report.connected_components, 1);
        assert!(report.watertight_two_manifold);
        assert!(report.consistently_oriented);
        assert!((report.signed_volume.unwrap() - 1.0 / 6.0).abs() < 1.0e-12);
    }

    #[test]
    fn missing_tetrahedron_face_reports_boundary() {
        let mut mesh = tetrahedron();
        mesh.triangles.pop();
        let report = mesh.topology_report().unwrap();
        assert_eq!(report.boundary_edges, 3);
        assert!(!report.watertight_two_manifold);
        assert_eq!(report.signed_volume, None);
    }

    #[test]
    fn duplicate_face_reports_non_manifold_edges() {
        let mut mesh = tetrahedron();
        mesh.triangles.push([0, 2, 1]);
        let report = mesh.topology_report().unwrap();
        assert_eq!(report.non_manifold_edges, 3);
        assert!(!report.watertight_two_manifold);
    }

    #[test]
    fn obj_import_triangulates_polygons_and_negative_indices() {
        let obj = b"v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf -4 -3 -2 -1\n";
        let imported = import_obj(obj).unwrap();
        assert_eq!(imported.format, SurfaceFormat::Obj);
        assert_eq!(imported.mesh.positions.len(), 4);
        assert_eq!(imported.mesh.triangles, vec![[0, 1, 2], [0, 2, 3]]);
        let report = imported.mesh.topology_report().unwrap();
        assert_eq!(report.boundary_edges, 4);
        assert!(!report.watertight_two_manifold);
    }

    #[test]
    fn ascii_stl_deduplicates_shared_vertices_for_topology() {
        let stl = b"solid tetra\n\
facet normal 0 0 0\n outer loop\n  vertex 0 0 0\n  vertex 0 1 0\n  vertex 1 0 0\n endloop\nendfacet\n\
facet normal 0 0 0\n outer loop\n  vertex 0 0 0\n  vertex 1 0 0\n  vertex 0 0 1\n endloop\nendfacet\n\
facet normal 0 0 0\n outer loop\n  vertex 0 0 0\n  vertex 0 0 1\n  vertex 0 1 0\n endloop\nendfacet\n\
facet normal 0 0 0\n outer loop\n  vertex 1 0 0\n  vertex 0 1 0\n  vertex 0 0 1\n endloop\nendfacet\nendsolid tetra\n";
        let imported = import_stl(stl).unwrap();
        assert_eq!(imported.format, SurfaceFormat::StlAscii);
        assert_eq!(imported.mesh.positions.len(), 4);
        assert!(imported.mesh.topology_report().unwrap().watertight_two_manifold);
    }

    #[test]
    fn binary_stl_is_detected_by_exact_record_length() {
        let mut bytes = vec![0_u8; 84 + 50];
        bytes[80..84].copy_from_slice(&1_u32.to_le_bytes());
        let record = 84;
        for (corner, xyz) in [
            [0.0_f32, 0.0, 0.0],
            [1.0_f32, 0.0, 0.0],
            [0.0_f32, 1.0, 0.0],
        ]
        .into_iter()
        .enumerate()
        {
            let offset = record + [12_usize, 24, 36][corner];
            bytes[offset..offset + 4].copy_from_slice(&xyz[0].to_le_bytes());
            bytes[offset + 4..offset + 8].copy_from_slice(&xyz[1].to_le_bytes());
            bytes[offset + 8..offset + 12].copy_from_slice(&xyz[2].to_le_bytes());
        }
        let imported = import_stl(&bytes).unwrap();
        assert_eq!(imported.format, SurfaceFormat::StlBinary);
        assert_eq!(imported.mesh.positions.len(), 3);
        assert_eq!(imported.mesh.triangles, vec![[0, 1, 2]]);
    }
}
