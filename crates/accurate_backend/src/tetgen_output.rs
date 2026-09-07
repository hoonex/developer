use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::{
    BoundaryMarkerId, BoundaryTriangle, Tetrahedron, VolumeMesh, VolumeMeshError,
};

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedTetgenVolumeMesh {
    pub mesh: VolumeMesh,
    pub input_node_ids: Vec<u64>,
    pub tetrahedron_ids: Vec<u64>,
    pub boundary_face_ids: Vec<u64>,
    pub reoriented_tetrahedra: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TetgenOutputError {
    UnexpectedEnd {
        section: &'static str,
        field: &'static str,
    },
    InvalidInteger {
        section: &'static str,
        field: &'static str,
        value: String,
    },
    InvalidFloat {
        section: &'static str,
        field: &'static str,
        value: String,
    },
    TrailingData {
        section: &'static str,
        value: String,
    },
    UnsupportedNodeDimension { dimension: usize },
    InvalidNodeBoundaryMarkerFlag { value: usize },
    InvalidFaceBoundaryMarkerFlag { value: usize },
    UnsupportedNodesPerTetrahedron { value: usize },
    EmptySection { section: &'static str },
    DuplicateNodeId { id: u64 },
    DuplicateTetrahedronId { id: u64 },
    DuplicateBoundaryFaceId { id: u64 },
    PointIndexOverflow,
    MissingNodeReference {
        section: &'static str,
        record_id: u64,
        node_id: u64,
    },
    NonFiniteNode { id: u64 },
    InvalidBoundaryMarker { face_id: u64, marker: i64 },
    DegenerateTetrahedron { id: u64, signed_volume: f64 },
    VolumeMesh(VolumeMeshError),
}

impl Display for TetgenOutputError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedEnd { section, field } => {
                write!(f, "TetGen {section} ended before required field `{field}`")
            }
            Self::InvalidInteger {
                section,
                field,
                value,
            } => write!(
                f,
                "TetGen {section} field `{field}` is not a valid integer: `{value}`"
            ),
            Self::InvalidFloat {
                section,
                field,
                value,
            } => write!(
                f,
                "TetGen {section} field `{field}` is not a valid floating-point value: `{value}`"
            ),
            Self::TrailingData { section, value } => write!(
                f,
                "TetGen {section} contains unexpected trailing token `{value}`"
            ),
            Self::UnsupportedNodeDimension { dimension } => write!(
                f,
                "TetGen .node dimension must be 3; got {dimension}"
            ),
            Self::InvalidNodeBoundaryMarkerFlag { value } => write!(
                f,
                "TetGen .node boundary-marker flag must be 0 or 1; got {value}"
            ),
            Self::InvalidFaceBoundaryMarkerFlag { value } => write!(
                f,
                "TetGen .face output must contain boundary markers (flag 1); got {value}"
            ),
            Self::UnsupportedNodesPerTetrahedron { value } => write!(
                f,
                "TetGen .ele baseline output must contain 4-node tetrahedra; got {value} nodes per element"
            ),
            Self::EmptySection { section } => write!(f, "TetGen {section} contains no records"),
            Self::DuplicateNodeId { id } => write!(f, "TetGen .node repeats node id {id}"),
            Self::DuplicateTetrahedronId { id } => {
                write!(f, "TetGen .ele repeats tetrahedron id {id}")
            }
            Self::DuplicateBoundaryFaceId { id } => {
                write!(f, "TetGen .face repeats boundary face id {id}")
            }
            Self::PointIndexOverflow => write!(
                f,
                "TetGen output contains too many nodes for AeroForge's u32 VolumeMesh index contract"
            ),
            Self::MissingNodeReference {
                section,
                record_id,
                node_id,
            } => write!(
                f,
                "TetGen {section} record {record_id} references missing node id {node_id}"
            ),
            Self::NonFiniteNode { id } => {
                write!(f, "TetGen .node record {id} contains a non-finite coordinate")
            }
            Self::InvalidBoundaryMarker { face_id, marker } => write!(
                f,
                "TetGen .face record {face_id} has invalid AeroForge boundary marker {marker}"
            ),
            Self::DegenerateTetrahedron { id, signed_volume } => write!(
                f,
                "TetGen .ele tetrahedron {id} is degenerate or non-finite (signed volume {signed_volume})"
            ),
            Self::VolumeMesh(error) => write!(f, "parsed TetGen VolumeMesh audit failed: {error}"),
        }
    }
}

impl Error for TetgenOutputError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::VolumeMesh(error) => Some(error),
            _ => None,
        }
    }
}

impl From<VolumeMeshError> for TetgenOutputError {
    fn from(value: VolumeMeshError) -> Self {
        Self::VolumeMesh(value)
    }
}

/// Parses the three baseline TetGen outputs into the canonical AeroForge `VolumeMesh` contract.
///
/// Record identifiers are resolved through explicit maps rather than treated as vector offsets.
/// Nodes, tetrahedra, and boundary faces are canonicalized by ascending TetGen record id so output
/// is deterministic even if line order changes. Point attributes and point boundary markers are
/// accepted and ignored; tetrahedron attributes are accepted and ignored. `.face` boundary markers
/// are mandatory because they carry the authoritative domain/SceneObject provenance established by
/// the PLC input.
///
/// TetGen tetrahedra with finite negative signed orientation are corrected by swapping the first two
/// corners exactly once. Zero/non-finite tetrahedra fail closed. The fully reconstructed mesh must
/// then pass `VolumeMesh::audit`, which verifies positive cell orientation, manifold volume faces,
/// complete labeled exterior faces, and nonzero boundary markers.
pub fn parse_tetgen_volume_mesh(
    node_text: &str,
    ele_text: &str,
    face_text: &str,
) -> Result<ParsedTetgenVolumeMesh, TetgenOutputError> {
    let parsed_nodes = parse_nodes(node_text)?;
    let parsed_tetrahedra = parse_tetrahedra(ele_text)?;
    let parsed_faces = parse_faces(face_text)?;

    if parsed_nodes.is_empty() {
        return Err(TetgenOutputError::EmptySection { section: ".node" });
    }
    if parsed_tetrahedra.is_empty() {
        return Err(TetgenOutputError::EmptySection { section: ".ele" });
    }
    if parsed_faces.is_empty() {
        return Err(TetgenOutputError::EmptySection { section: ".face" });
    }

    let mut points = Vec::with_capacity(parsed_nodes.len());
    let mut node_map = BTreeMap::<u64, u32>::new();
    let mut input_node_ids = Vec::with_capacity(parsed_nodes.len());
    for (&node_id, &point) in &parsed_nodes {
        let compact = u32::try_from(points.len()).map_err(|_| TetgenOutputError::PointIndexOverflow)?;
        node_map.insert(node_id, compact);
        input_node_ids.push(node_id);
        points.push(point);
    }

    let mut cells = Vec::with_capacity(parsed_tetrahedra.len());
    let mut tetrahedron_ids = Vec::with_capacity(parsed_tetrahedra.len());
    let mut reoriented_tetrahedra = 0_usize;
    for (&tetrahedron_id, node_ids) in &parsed_tetrahedra {
        let mut vertices = resolve_nodes(".ele", tetrahedron_id, *node_ids, &node_map)?;
        let signed_volume = signed_tetra_volume(
            points[vertices[0] as usize],
            points[vertices[1] as usize],
            points[vertices[2] as usize],
            points[vertices[3] as usize],
        );
        if !signed_volume.is_finite() || signed_volume == 0.0 {
            return Err(TetgenOutputError::DegenerateTetrahedron {
                id: tetrahedron_id,
                signed_volume,
            });
        }
        if signed_volume < 0.0 {
            vertices.swap(0, 1);
            reoriented_tetrahedra += 1;
        }
        cells.push(Tetrahedron { vertices });
        tetrahedron_ids.push(tetrahedron_id);
    }

    let mut boundary = Vec::with_capacity(parsed_faces.len());
    let mut boundary_face_ids = Vec::with_capacity(parsed_faces.len());
    for (&face_id, &(node_ids, marker)) in &parsed_faces {
        let vertices = resolve_nodes(".face", face_id, node_ids, &node_map)?;
        boundary.push(BoundaryTriangle { vertices, marker });
        boundary_face_ids.push(face_id);
    }

    let mesh = VolumeMesh {
        points,
        cells,
        boundary,
    };
    mesh.audit()?;

    Ok(ParsedTetgenVolumeMesh {
        mesh,
        input_node_ids,
        tetrahedron_ids,
        boundary_face_ids,
        reoriented_tetrahedra,
    })
}

fn parse_nodes(text: &str) -> Result<BTreeMap<u64, [f64; 3]>, TetgenOutputError> {
    let mut cursor = TokenCursor::new(text);
    let count = cursor.usize(".node", "point_count")?;
    let dimension = cursor.usize(".node", "dimension")?;
    if dimension != 3 {
        return Err(TetgenOutputError::UnsupportedNodeDimension { dimension });
    }
    let attributes = cursor.usize(".node", "attribute_count")?;
    let marker_flag = cursor.usize(".node", "boundary_marker_flag")?;
    if marker_flag > 1 {
        return Err(TetgenOutputError::InvalidNodeBoundaryMarkerFlag { value: marker_flag });
    }

    let mut nodes = BTreeMap::new();
    for _ in 0..count {
        let id = cursor.u64(".node", "point_id")?;
        let point = [
            cursor.f64(".node", "x")?,
            cursor.f64(".node", "y")?,
            cursor.f64(".node", "z")?,
        ];
        if !point.iter().all(|value| value.is_finite()) {
            return Err(TetgenOutputError::NonFiniteNode { id });
        }
        for _ in 0..attributes {
            cursor.skip(".node", "point_attribute")?;
        }
        if marker_flag == 1 {
            cursor.skip(".node", "point_boundary_marker")?;
        }
        if nodes.insert(id, point).is_some() {
            return Err(TetgenOutputError::DuplicateNodeId { id });
        }
    }
    cursor.finish(".node")?;
    Ok(nodes)
}

fn parse_tetrahedra(text: &str) -> Result<BTreeMap<u64, [u64; 4]>, TetgenOutputError> {
    let mut cursor = TokenCursor::new(text);
    let count = cursor.usize(".ele", "tetrahedron_count")?;
    let nodes_per_tetrahedron = cursor.usize(".ele", "nodes_per_tetrahedron")?;
    if nodes_per_tetrahedron != 4 {
        return Err(TetgenOutputError::UnsupportedNodesPerTetrahedron {
            value: nodes_per_tetrahedron,
        });
    }
    let attributes = cursor.usize(".ele", "attribute_count")?;

    let mut tetrahedra = BTreeMap::new();
    for _ in 0..count {
        let id = cursor.u64(".ele", "tetrahedron_id")?;
        let nodes = [
            cursor.u64(".ele", "node_0")?,
            cursor.u64(".ele", "node_1")?,
            cursor.u64(".ele", "node_2")?,
            cursor.u64(".ele", "node_3")?,
        ];
        for _ in 0..attributes {
            cursor.skip(".ele", "tetrahedron_attribute")?;
        }
        if tetrahedra.insert(id, nodes).is_some() {
            return Err(TetgenOutputError::DuplicateTetrahedronId { id });
        }
    }
    cursor.finish(".ele")?;
    Ok(tetrahedra)
}

fn parse_faces(
    text: &str,
) -> Result<BTreeMap<u64, ([u64; 3], BoundaryMarkerId)>, TetgenOutputError> {
    let mut cursor = TokenCursor::new(text);
    let count = cursor.usize(".face", "face_count")?;
    let marker_flag = cursor.usize(".face", "boundary_marker_flag")?;
    if marker_flag != 1 {
        return Err(TetgenOutputError::InvalidFaceBoundaryMarkerFlag { value: marker_flag });
    }

    let mut faces = BTreeMap::new();
    for _ in 0..count {
        let id = cursor.u64(".face", "face_id")?;
        let nodes = [
            cursor.u64(".face", "node_0")?,
            cursor.u64(".face", "node_1")?,
            cursor.u64(".face", "node_2")?,
        ];
        let marker = cursor.i64(".face", "boundary_marker")?;
        if marker <= 0 || marker > u32::MAX as i64 {
            return Err(TetgenOutputError::InvalidBoundaryMarker {
                face_id: id,
                marker,
            });
        }
        if faces
            .insert(id, (nodes, BoundaryMarkerId(marker as u32)))
            .is_some()
        {
            return Err(TetgenOutputError::DuplicateBoundaryFaceId { id });
        }
    }
    cursor.finish(".face")?;
    Ok(faces)
}

fn resolve_nodes<const N: usize>(
    section: &'static str,
    record_id: u64,
    node_ids: [u64; N],
    node_map: &BTreeMap<u64, u32>,
) -> Result<[u32; N], TetgenOutputError> {
    let mut resolved = [0_u32; N];
    for (slot, node_id) in resolved.iter_mut().zip(node_ids) {
        *slot = *node_map
            .get(&node_id)
            .ok_or(TetgenOutputError::MissingNodeReference {
                section,
                record_id,
                node_id,
            })?;
    }
    Ok(resolved)
}

fn signed_tetra_volume(a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) -> f64 {
    dot(sub(b, a), cross(sub(c, a), sub(d, a))) / 6.0
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

struct TokenCursor<'a> {
    tokens: Vec<&'a str>,
    position: usize,
}

impl<'a> TokenCursor<'a> {
    fn new(text: &'a str) -> Self {
        let tokens = text
            .lines()
            .flat_map(|line| line.split('#').next().unwrap_or("").split_whitespace())
            .collect();
        Self {
            tokens,
            position: 0,
        }
    }

    fn next(
        &mut self,
        section: &'static str,
        field: &'static str,
    ) -> Result<&'a str, TetgenOutputError> {
        let value = self.tokens.get(self.position).copied().ok_or(
            TetgenOutputError::UnexpectedEnd { section, field },
        )?;
        self.position += 1;
        Ok(value)
    }

    fn skip(
        &mut self,
        section: &'static str,
        field: &'static str,
    ) -> Result<(), TetgenOutputError> {
        self.next(section, field)?;
        Ok(())
    }

    fn usize(
        &mut self,
        section: &'static str,
        field: &'static str,
    ) -> Result<usize, TetgenOutputError> {
        let value = self.next(section, field)?;
        value
            .parse::<usize>()
            .map_err(|_| TetgenOutputError::InvalidInteger {
                section,
                field,
                value: value.to_owned(),
            })
    }

    fn u64(
        &mut self,
        section: &'static str,
        field: &'static str,
    ) -> Result<u64, TetgenOutputError> {
        let value = self.next(section, field)?;
        value
            .parse::<u64>()
            .map_err(|_| TetgenOutputError::InvalidInteger {
                section,
                field,
                value: value.to_owned(),
            })
    }

    fn i64(
        &mut self,
        section: &'static str,
        field: &'static str,
    ) -> Result<i64, TetgenOutputError> {
        let value = self.next(section, field)?;
        value
            .parse::<i64>()
            .map_err(|_| TetgenOutputError::InvalidInteger {
                section,
                field,
                value: value.to_owned(),
            })
    }

    fn f64(
        &mut self,
        section: &'static str,
        field: &'static str,
    ) -> Result<f64, TetgenOutputError> {
        let value = self.next(section, field)?;
        value
            .parse::<f64>()
            .map_err(|_| TetgenOutputError::InvalidFloat {
                section,
                field,
                value: value.to_owned(),
            })
    }

    fn finish(&self, section: &'static str) -> Result<(), TetgenOutputError> {
        if let Some(value) = self.tokens.get(self.position) {
            Err(TetgenOutputError::TrailingData {
                section,
                value: (*value).to_owned(),
            })
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_node() -> &'static str {
        "# four points, intentionally shuffled ids\n4 3 1 1\n30 0 1 0 9.0 3\n10 0 0 0 8.0 1\n40 0 0 1 7.0 4\n20 1 0 0 6.0 2\n"
    }

    fn sample_ele_negative_orientation() -> &'static str {
        "1 4 1\n7 10 30 20 40 123\n"
    }

    fn sample_face() -> &'static str {
        "4 1\n13 10 20 30 11\n10 10 40 20 12\n12 20 40 30 13\n11 30 40 10 14\n"
    }

    #[test]
    fn parses_id_mapped_output_and_reorients_negative_tetrahedron() {
        let parsed = parse_tetgen_volume_mesh(
            sample_node(),
            sample_ele_negative_orientation(),
            sample_face(),
        )
        .unwrap();

        assert_eq!(parsed.input_node_ids, vec![10, 20, 30, 40]);
        assert_eq!(parsed.tetrahedron_ids, vec![7]);
        assert_eq!(parsed.boundary_face_ids, vec![10, 11, 12, 13]);
        assert_eq!(parsed.reoriented_tetrahedra, 1);
        assert_eq!(parsed.mesh.audit().unwrap().total_volume, 1.0 / 6.0);
        assert_eq!(
            parsed
                .mesh
                .audit()
                .unwrap()
                .marker_triangle_counts
                .get(&BoundaryMarkerId(11)),
            Some(&1)
        );
    }

    #[test]
    fn face_markers_are_required_for_provenance() {
        let error = parse_tetgen_volume_mesh(
            sample_node(),
            sample_ele_negative_orientation(),
            "4 0\n10 10 20 30\n11 10 40 20\n12 20 40 30\n13 30 40 10\n",
        )
        .unwrap_err();
        assert_eq!(
            error,
            TetgenOutputError::InvalidFaceBoundaryMarkerFlag { value: 0 }
        );
    }

    #[test]
    fn missing_node_reference_fails_closed() {
        let error = parse_tetgen_volume_mesh(
            sample_node(),
            "1 4 0\n0 10 20 30 999\n",
            sample_face(),
        )
        .unwrap_err();
        assert_eq!(
            error,
            TetgenOutputError::MissingNodeReference {
                section: ".ele",
                record_id: 0,
                node_id: 999,
            }
        );
    }

    #[test]
    fn zero_or_negative_boundary_marker_fails_closed() {
        let error = parse_tetgen_volume_mesh(
            sample_node(),
            sample_ele_negative_orientation(),
            "4 1\n10 10 20 30 0\n11 10 40 20 12\n12 20 40 30 13\n13 30 40 10 14\n",
        )
        .unwrap_err();
        assert_eq!(
            error,
            TetgenOutputError::InvalidBoundaryMarker {
                face_id: 10,
                marker: 0,
            }
        );
    }

    #[test]
    fn higher_order_elements_are_not_silently_truncated() {
        let error = parse_tetgen_volume_mesh(
            sample_node(),
            "1 10 0\n0 10 20 30 40 10 20 30 40 10 20\n",
            sample_face(),
        )
        .unwrap_err();
        assert_eq!(
            error,
            TetgenOutputError::UnsupportedNodesPerTetrahedron { value: 10 }
        );
    }
}
