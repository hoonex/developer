use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::fmt::{Display, Formatter};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use gltf::buffer::Source as BufferSource;
use gltf::mesh::Mode;

use crate::SurfaceMesh;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GltfSourceFormat {
    Json,
    Binary,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GltfSurfaceImport {
    pub mesh: SurfaceMesh,
    pub format: GltfSourceFormat,
    pub scene_index: usize,
    /// External buffer URIs consumed from the caller-provided map, in document order.
    pub external_buffer_uris: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GltfImportError {
    Parse(String),
    MissingScene,
    MissingBinaryBlob { buffer: usize },
    MissingExternalBuffer { buffer: usize, uri: String },
    UnsupportedDataUri { buffer: usize },
    InvalidDataUriBase64 { buffer: usize, message: String },
    BufferTooShort { buffer: usize, expected: usize, actual: usize },
    MissingPositions { mesh: usize, primitive: usize },
    UnsupportedPrimitiveMode { mesh: usize, primitive: usize, mode: String },
    SkinnedNode { node: usize },
    MorphTargets { mesh: usize, primitive: usize },
    VertexIndexOverflow,
    PrimitiveIndexOutOfRange { mesh: usize, primitive: usize, index: u32, vertices: usize },
    EmptySceneGeometry,
}

impl Display for GltfImportError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(message) => write!(f, "glTF parse/validation failed: {message}"),
            Self::MissingScene => write!(f, "glTF contains no scene to import"),
            Self::MissingBinaryBlob { buffer } => {
                write!(f, "glTF buffer {buffer} requires a missing GLB BIN chunk")
            }
            Self::MissingExternalBuffer { buffer, uri } => {
                write!(f, "glTF buffer {buffer} references unresolved external URI `{uri}`")
            }
            Self::UnsupportedDataUri { buffer } => write!(
                f,
                "glTF buffer {buffer} uses a non-base64 data URI; only base64 buffer data URIs are accepted"
            ),
            Self::InvalidDataUriBase64 { buffer, message } => {
                write!(f, "glTF buffer {buffer} has invalid base64 data: {message}")
            }
            Self::BufferTooShort { buffer, expected, actual } => write!(
                f,
                "glTF buffer {buffer} is shorter than declared: expected at least {expected} bytes, got {actual}"
            ),
            Self::MissingPositions { mesh, primitive } => {
                write!(f, "glTF mesh {mesh} primitive {primitive} has no POSITION attribute")
            }
            Self::UnsupportedPrimitiveMode { mesh, primitive, mode } => write!(
                f,
                "glTF mesh {mesh} primitive {primitive} uses unsupported non-surface mode {mode}"
            ),
            Self::SkinnedNode { node } => write!(
                f,
                "glTF node {node} is skinned; CFD surface import requires static undeformed geometry"
            ),
            Self::MorphTargets { mesh, primitive } => write!(
                f,
                "glTF mesh {mesh} primitive {primitive} has morph targets; CFD surface import requires an explicit baked shape"
            ),
            Self::VertexIndexOverflow => write!(f, "imported glTF surface exceeds u32 vertex indexing"),
            Self::PrimitiveIndexOutOfRange { mesh, primitive, index, vertices } => write!(
                f,
                "glTF mesh {mesh} primitive {primitive} index {index} is outside {vertices} positions"
            ),
            Self::EmptySceneGeometry => write!(f, "selected glTF scene contains no triangle surface geometry"),
        }
    }
}

impl Error for GltfImportError {}

/// Imports the document's default scene, or the first scene when no default is declared.
///
/// Node transforms are accumulated through the hierarchy and baked into the returned positions.
/// Triangle lists, strips and fans are converted to triangles. Static CFD geometry intentionally
/// rejects skins and morph targets rather than silently importing an undeformed render mesh.
/// External `.bin` buffers must be supplied by their exact glTF URI key. Base64 data URIs and GLB
/// BIN chunks are resolved internally.
pub fn import_gltf_surface(
    bytes: &[u8],
    external_buffers: &BTreeMap<String, Vec<u8>>,
) -> Result<GltfSurfaceImport, GltfImportError> {
    let format = if bytes.starts_with(b"glTF") {
        GltfSourceFormat::Binary
    } else {
        GltfSourceFormat::Json
    };
    let gltf = gltf::Gltf::from_slice(bytes).map_err(|error| GltfImportError::Parse(error.to_string()))?;
    let (buffers, external_buffer_uris) = resolve_buffers(&gltf, external_buffers)?;
    let scene = gltf
        .document
        .default_scene()
        .or_else(|| gltf.document.scenes().next())
        .ok_or(GltfImportError::MissingScene)?;
    let scene_index = scene.index();

    let mut mesh = SurfaceMesh::default();
    let mut interned = HashMap::<(u64, u64, u64), u32>::new();
    for node in scene.nodes() {
        flatten_node(
            node,
            identity_matrix(),
            &buffers,
            &mut mesh,
            &mut interned,
        )?;
    }
    if mesh.positions.is_empty() || mesh.triangles.is_empty() {
        return Err(GltfImportError::EmptySceneGeometry);
    }

    Ok(GltfSurfaceImport {
        mesh,
        format,
        scene_index,
        external_buffer_uris,
    })
}

fn resolve_buffers(
    gltf: &gltf::Gltf,
    external_buffers: &BTreeMap<String, Vec<u8>>,
) -> Result<(Vec<Vec<u8>>, Vec<String>), GltfImportError> {
    let mut resolved = Vec::new();
    let mut used_external = Vec::new();
    for buffer in gltf.document.buffers() {
        let data = match buffer.source() {
            BufferSource::Bin => gltf
                .blob
                .as_ref()
                .cloned()
                .ok_or(GltfImportError::MissingBinaryBlob {
                    buffer: buffer.index(),
                })?,
            BufferSource::Uri(uri) if uri.starts_with("data:") => {
                decode_buffer_data_uri(buffer.index(), uri)?
            }
            BufferSource::Uri(uri) => {
                let data = external_buffers.get(uri).cloned().ok_or_else(|| {
                    GltfImportError::MissingExternalBuffer {
                        buffer: buffer.index(),
                        uri: uri.to_owned(),
                    }
                })?;
                used_external.push(uri.to_owned());
                data
            }
        };
        if data.len() < buffer.length() {
            return Err(GltfImportError::BufferTooShort {
                buffer: buffer.index(),
                expected: buffer.length(),
                actual: data.len(),
            });
        }
        resolved.push(data);
    }
    Ok((resolved, used_external))
}

fn decode_buffer_data_uri(buffer: usize, uri: &str) -> Result<Vec<u8>, GltfImportError> {
    let Some((header, encoded)) = uri.split_once(',') else {
        return Err(GltfImportError::UnsupportedDataUri { buffer });
    };
    if !header.ends_with(";base64") {
        return Err(GltfImportError::UnsupportedDataUri { buffer });
    }
    STANDARD
        .decode(encoded.as_bytes())
        .map_err(|error| GltfImportError::InvalidDataUriBase64 {
            buffer,
            message: error.to_string(),
        })
}

fn flatten_node(
    node: gltf::Node<'_>,
    parent_world: [[f64; 4]; 4],
    buffers: &[Vec<u8>],
    output: &mut SurfaceMesh,
    interned: &mut HashMap<(u64, u64, u64), u32>,
) -> Result<(), GltfImportError> {
    if node.skin().is_some() {
        return Err(GltfImportError::SkinnedNode { node: node.index() });
    }
    let local = matrix_f32_to_f64(node.transform().matrix());
    let world = matrix_mul(parent_world, local);

    if let Some(mesh) = node.mesh() {
        let mesh_index = mesh.index();
        for primitive in mesh.primitives() {
            let primitive_index = primitive.index();
            if primitive.morph_targets().next().is_some() {
                return Err(GltfImportError::MorphTargets {
                    mesh: mesh_index,
                    primitive: primitive_index,
                });
            }
            let mode = primitive.mode();
            if !matches!(mode, Mode::Triangles | Mode::TriangleStrip | Mode::TriangleFan) {
                return Err(GltfImportError::UnsupportedPrimitiveMode {
                    mesh: mesh_index,
                    primitive: primitive_index,
                    mode: format!("{mode:?}"),
                });
            }

            let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(Vec::as_slice));
            let local_positions = reader
                .read_positions()
                .ok_or(GltfImportError::MissingPositions {
                    mesh: mesh_index,
                    primitive: primitive_index,
                })?
                .collect::<Vec<_>>();
            let mut global_positions = Vec::with_capacity(local_positions.len());
            for position in local_positions.iter().copied() {
                let transformed = transform_point(world, [
                    position[0] as f64,
                    position[1] as f64,
                    position[2] as f64,
                ]);
                if !transformed.iter().all(|value| value.is_finite()) {
                    return Err(GltfImportError::Parse(format!(
                        "node {} produced a non-finite transformed position",
                        node.index()
                    )));
                }
                let key = (
                    transformed[0].to_bits(),
                    transformed[1].to_bits(),
                    transformed[2].to_bits(),
                );
                let global = if let Some(&index) = interned.get(&key) {
                    index
                } else {
                    let index = u32::try_from(output.positions.len())
                        .map_err(|_| GltfImportError::VertexIndexOverflow)?;
                    output.positions.push(transformed);
                    interned.insert(key, index);
                    index
                };
                global_positions.push(global);
            }

            let sequence = if let Some(indices) = reader.read_indices() {
                indices.into_u32().collect::<Vec<_>>()
            } else {
                (0..local_positions.len())
                    .map(|index| u32::try_from(index).map_err(|_| GltfImportError::VertexIndexOverflow))
                    .collect::<Result<Vec<_>, _>>()?
            };
            for &index in &sequence {
                if index as usize >= global_positions.len() {
                    return Err(GltfImportError::PrimitiveIndexOutOfRange {
                        mesh: mesh_index,
                        primitive: primitive_index,
                        index,
                        vertices: global_positions.len(),
                    });
                }
            }
            append_triangles(
                mode,
                &sequence,
                &global_positions,
                &mut output.triangles,
                mesh_index,
                primitive_index,
            )?;
        }
    }

    for child in node.children() {
        flatten_node(child, world, buffers, output, interned)?;
    }
    Ok(())
}

fn append_triangles(
    mode: Mode,
    sequence: &[u32],
    global_positions: &[u32],
    triangles: &mut Vec<[u32; 3]>,
    mesh: usize,
    primitive: usize,
) -> Result<(), GltfImportError> {
    let mapped = |index: u32| global_positions[index as usize];
    match mode {
        Mode::Triangles => {
            if sequence.len() % 3 != 0 {
                return Err(GltfImportError::Parse(format!(
                    "glTF mesh {mesh} primitive {primitive} triangle-list index count {} is not divisible by 3",
                    sequence.len()
                )));
            }
            for face in sequence.chunks_exact(3) {
                triangles.push([mapped(face[0]), mapped(face[1]), mapped(face[2])]);
            }
        }
        Mode::TriangleStrip => {
            for index in 0..sequence.len().saturating_sub(2) {
                let a = sequence[index];
                let b = sequence[index + 1];
                let c = sequence[index + 2];
                let face = if index % 2 == 0 { [a, b, c] } else { [b, a, c] };
                triangles.push([mapped(face[0]), mapped(face[1]), mapped(face[2])]);
            }
        }
        Mode::TriangleFan => {
            if let Some(&center) = sequence.first() {
                for index in 1..sequence.len().saturating_sub(1) {
                    triangles.push([
                        mapped(center),
                        mapped(sequence[index]),
                        mapped(sequence[index + 1]),
                    ]);
                }
            }
        }
        _ => {
            return Err(GltfImportError::UnsupportedPrimitiveMode {
                mesh,
                primitive,
                mode: format!("{mode:?}"),
            });
        }
    }
    Ok(())
}

fn identity_matrix() -> [[f64; 4]; 4] {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn matrix_f32_to_f64(matrix: [[f32; 4]; 4]) -> [[f64; 4]; 4] {
    matrix.map(|column| column.map(f64::from))
}

/// Multiplies column-major matrices so `matrix_mul(parent, local)` produces a world transform.
fn matrix_mul(a: [[f64; 4]; 4], b: [[f64; 4]; 4]) -> [[f64; 4]; 4] {
    let mut result = [[0.0_f64; 4]; 4];
    for column in 0..4 {
        for row in 0..4 {
            result[column][row] = (0..4).map(|k| a[k][row] * b[column][k]).sum();
        }
    }
    result
}

fn transform_point(matrix: [[f64; 4]; 4], point: [f64; 3]) -> [f64; 3] {
    [
        matrix[0][0] * point[0]
            + matrix[1][0] * point[1]
            + matrix[2][0] * point[2]
            + matrix[3][0],
        matrix[0][1] * point[0]
            + matrix[1][1] * point[1]
            + matrix[2][1] * point[2]
            + matrix[3][1],
        matrix[0][2] * point[0]
            + matrix[1][2] * point[1]
            + matrix[2][2] * point[2]
            + matrix[3][2],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle_payload() -> Vec<u8> {
        let mut payload = Vec::new();
        for position in [[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for component in position {
                payload.extend_from_slice(&component.to_le_bytes());
            }
        }
        for index in [0_u16, 1, 2] {
            payload.extend_from_slice(&index.to_le_bytes());
        }
        payload
    }

    fn triangle_gltf(buffer_uri: &str) -> String {
        format!(
            r#"{{
  "asset": {{"version": "2.0"}},
  "buffers": [{{"uri": "{buffer_uri}", "byteLength": 42}}],
  "bufferViews": [
    {{"buffer": 0, "byteOffset": 0, "byteLength": 36}},
    {{"buffer": 0, "byteOffset": 36, "byteLength": 6}}
  ],
  "accessors": [
    {{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "max": [1,1,0], "min": [0,0,0]}},
    {{"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"}}
  ],
  "meshes": [{{"primitives": [{{"attributes": {{"POSITION": 0}}, "indices": 1}}]}}],
  "nodes": [{{"mesh": 0, "translation": [2, 3, 4]}}],
  "scenes": [{{"nodes": [0]}}],
  "scene": 0
}}"#
        )
    }

    #[test]
    fn data_uri_scene_bakes_node_transform() {
        let payload = triangle_payload();
        let uri = format!(
            "data:application/octet-stream;base64,{}",
            STANDARD.encode(&payload)
        );
        let json = triangle_gltf(&uri);
        let imported = import_gltf_surface(json.as_bytes(), &BTreeMap::new()).unwrap();
        assert_eq!(imported.format, GltfSourceFormat::Json);
        assert_eq!(imported.scene_index, 0);
        assert_eq!(imported.external_buffer_uris, Vec::<String>::new());
        assert_eq!(
            imported.mesh.positions,
            vec![[2.0, 3.0, 4.0], [3.0, 3.0, 4.0], [2.0, 4.0, 4.0]]
        );
        assert_eq!(imported.mesh.triangles, vec![[0, 1, 2]]);
    }

    #[test]
    fn external_buffer_uri_is_explicit_and_recorded() {
        let json = triangle_gltf("mesh.bin");
        let mut external = BTreeMap::new();
        external.insert("mesh.bin".to_owned(), triangle_payload());
        let imported = import_gltf_surface(json.as_bytes(), &external).unwrap();
        assert_eq!(imported.external_buffer_uris, vec!["mesh.bin"]);
        assert_eq!(imported.mesh.triangles, vec![[0, 1, 2]]);
    }

    #[test]
    fn missing_external_buffer_fails_closed() {
        let json = triangle_gltf("missing.bin");
        let error = import_gltf_surface(json.as_bytes(), &BTreeMap::new()).unwrap_err();
        assert!(matches!(
            error,
            GltfImportError::MissingExternalBuffer { buffer: 0, ref uri } if uri == "missing.bin"
        ));
    }

    #[test]
    fn nested_transform_composition_is_parent_times_local() {
        let parent = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [10.0, 0.0, 0.0, 1.0],
        ];
        let child = [
            [2.0, 0.0, 0.0, 0.0],
            [0.0, 2.0, 0.0, 0.0],
            [0.0, 0.0, 2.0, 0.0],
            [0.0, 3.0, 0.0, 1.0],
        ];
        let world = matrix_mul(parent, child);
        assert_eq!(transform_point(world, [1.0, 0.0, 0.0]), [12.0, 3.0, 0.0]);
    }

    #[test]
    fn strip_and_fan_winding_are_deterministic() {
        let globals = [10, 11, 12, 13];
        let sequence = [0, 1, 2, 3];
        let mut strip = Vec::new();
        append_triangles(Mode::TriangleStrip, &sequence, &globals, &mut strip, 0, 0).unwrap();
        assert_eq!(strip, vec![[10, 11, 12], [12, 11, 13]]);

        let mut fan = Vec::new();
        append_triangles(Mode::TriangleFan, &sequence, &globals, &mut fan, 0, 0).unwrap();
        assert_eq!(fan, vec![[10, 11, 12], [10, 12, 13]]);
    }
}
