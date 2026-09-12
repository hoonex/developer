use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::BoundaryMarkerId;

use crate::imported_surface::AuditedImportedSurfaceBody;
use crate::scene_provenance::{
    build_scene_owner_marker_provenance, SceneOwnerProvenanceError,
};
use crate::su2_mesh::{
    BoundaryRole, BoundarySource, DomainAxis, DomainSide, Su2MarkerBinding, Su2MarkerMap,
};

/// Owned, canonicalized input contract for a future source-surface-driven exterior mesher.
///
/// Construction requires one finite axis-aligned outer domain with exactly one binding for each
/// of its six faces and at least one audited source body strictly inside that domain. Source bodies
/// are canonicalized by stable `SceneObject.id`, and the returned marker map deterministically
/// allocates one wall marker for every source body after the caller-owned domain marker range.
///
/// Holding this value establishes input ownership/provenance and strict source-AABB containment
/// only. It does not establish source-shell intersection freedom, positive body-to-body clearance,
/// a generated volume mesh, body-fittedness, feature preservation, boundary-layer quality, or CFD
/// accuracy.
#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedExteriorMesherInput {
    pub domain_min: [f64; 3],
    pub domain_max: [f64; 3],
    pub audited_sources: Vec<AuditedImportedSurfaceBody>,
    pub marker_map: Su2MarkerMap,
}

impl ValidatedExteriorMesherInput {
    pub fn marker_for_scene_object(&self, scene_object_id: u64) -> Option<BoundaryMarkerId> {
        self.marker_map.bindings.iter().find_map(|binding| {
            match &binding.source {
                BoundarySource::SceneObject {
                    scene_object_id: candidate,
                } if *candidate == scene_object_id => Some(binding.marker),
                _ => None,
            }
        })
    }

    pub fn marker_for_domain_face(
        &self,
        axis: DomainAxis,
        side: DomainSide,
    ) -> Option<BoundaryMarkerId> {
        self.marker_map.bindings.iter().find_map(|binding| {
            match &binding.source {
                BoundarySource::DomainFace {
                    axis: candidate_axis,
                    side: candidate_side,
                } if *candidate_axis == axis && *candidate_side == side => Some(binding.marker),
                _ => None,
            }
        })
    }

    pub fn scene_object_ids(&self) -> Vec<u64> {
        self.audited_sources
            .iter()
            .map(|source| source.scene_object_id)
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExteriorMesherInputError {
    InvalidDomainBounds {
        axis: usize,
        min: f64,
        max: f64,
    },
    MissingSourceBody,
    InvalidDomainMarkerTag {
        tag: String,
    },
    NonDomainBoundaryBinding {
        tag: String,
    },
    CustomDomainBoundary {
        tag: String,
    },
    DuplicateDomainFace {
        axis: DomainAxis,
        side: DomainSide,
    },
    MissingDomainFace {
        axis: DomainAxis,
        side: DomainSide,
    },
    DuplicateSourceSceneObject {
        scene_object_id: u64,
    },
    SourceTouchesOrLeavesDomain {
        scene_object_id: u64,
        axis: usize,
        source_min: f64,
        source_max: f64,
        domain_min: f64,
        domain_max: f64,
    },
    Provenance(SceneOwnerProvenanceError),
}

impl Display for ExteriorMesherInputError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDomainBounds { axis, min, max } => write!(
                f,
                "exterior mesher domain {} bounds must be finite and strictly increasing; got [{min}, {max}]",
                axis_name(*axis)
            ),
            Self::MissingSourceBody => write!(
                f,
                "source-surface-driven exterior mesher input requires at least one audited body"
            ),
            Self::InvalidDomainMarkerTag { tag } => write!(
                f,
                "exterior mesher domain marker tag `{tag}` is not a safe SU2 token"
            ),
            Self::NonDomainBoundaryBinding { tag } => write!(
                f,
                "exterior mesher domain binding `{tag}` must use DomainFace provenance"
            ),
            Self::CustomDomainBoundary { tag } => write!(
                f,
                "exterior mesher domain binding `{tag}` uses Custom role without a physical boundary model"
            ),
            Self::DuplicateDomainFace { axis, side } => write!(
                f,
                "exterior mesher domain face {} {} is bound more than once",
                domain_axis_name(*axis),
                domain_side_name(*side)
            ),
            Self::MissingDomainFace { axis, side } => write!(
                f,
                "exterior mesher domain is missing required face {} {}",
                domain_axis_name(*axis),
                domain_side_name(*side)
            ),
            Self::DuplicateSourceSceneObject { scene_object_id } => write!(
                f,
                "exterior mesher input received duplicate audited sources for SceneObject {scene_object_id}"
            ),
            Self::SourceTouchesOrLeavesDomain {
                scene_object_id,
                axis,
                source_min,
                source_max,
                domain_min,
                domain_max,
            } => write!(
                f,
                "SceneObject {scene_object_id} source bounds [{source_min}, {source_max}] on {} must lie strictly inside exterior domain [{domain_min}, {domain_max}]",
                axis_name(*axis)
            ),
            Self::Provenance(error) => write!(
                f,
                "exterior mesher marker/provenance allocation failed: {error}"
            ),
        }
    }
}

impl Error for ExteriorMesherInputError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Provenance(error) => Some(error),
            _ => None,
        }
    }
}

impl From<SceneOwnerProvenanceError> for ExteriorMesherInputError {
    fn from(value: SceneOwnerProvenanceError) -> Self {
        Self::Provenance(value)
    }
}

/// Validates and owns one canonical source-surface-driven exterior-mesher request.
///
/// Domain bindings are canonicalized to `x-/x+/y-/y+/z-/z+`. Audited source bodies are
/// canonicalized by stable `SceneObject.id`; duplicate IDs fail closed. Every source AABB must lie
/// strictly inside the finite axis-aligned domain so a body touching or crossing the outer boundary
/// never reaches a mesher through this contract. Stable body wall markers are allocated
/// deterministically from the canonical body order.
///
/// This is deliberately an input contract, not a meshing result. Source self/inter-body
/// intersection validation remains an explicit policy-controlled gate, and any candidate mesh must
/// still pass `validate_candidate_exterior_mesher_handoff` before solver preparation.
pub fn build_validated_exterior_mesher_input(
    domain_min: [f64; 3],
    domain_max: [f64; 3],
    domain_bindings: Vec<Su2MarkerBinding>,
    mut audited_sources: Vec<AuditedImportedSurfaceBody>,
) -> Result<ValidatedExteriorMesherInput, ExteriorMesherInputError> {
    for axis in 0..3 {
        let min = domain_min[axis];
        let max = domain_max[axis];
        if !min.is_finite() || !max.is_finite() || min >= max {
            return Err(ExteriorMesherInputError::InvalidDomainBounds {
                axis,
                min,
                max,
            });
        }
    }

    if audited_sources.is_empty() {
        return Err(ExteriorMesherInputError::MissingSourceBody);
    }

    let mut canonical_faces: [Option<Su2MarkerBinding>; 6] = std::array::from_fn(|_| None);
    for binding in domain_bindings {
        if !safe_marker_tag(&binding.tag) {
            return Err(ExteriorMesherInputError::InvalidDomainMarkerTag {
                tag: binding.tag,
            });
        }
        if binding.role == BoundaryRole::Custom {
            return Err(ExteriorMesherInputError::CustomDomainBoundary {
                tag: binding.tag,
            });
        }
        let (axis, side) = match &binding.source {
            BoundarySource::DomainFace { axis, side } => (*axis, *side),
            _ => {
                return Err(ExteriorMesherInputError::NonDomainBoundaryBinding {
                    tag: binding.tag,
                })
            }
        };
        let index = domain_face_index(axis, side);
        if canonical_faces[index].is_some() {
            return Err(ExteriorMesherInputError::DuplicateDomainFace { axis, side });
        }
        canonical_faces[index] = Some(binding);
    }

    for (index, binding) in canonical_faces.iter().enumerate() {
        if binding.is_none() {
            let (axis, side) = domain_face_from_index(index);
            return Err(ExteriorMesherInputError::MissingDomainFace { axis, side });
        }
    }
    let canonical_domain_bindings = canonical_faces
        .into_iter()
        .map(Option::unwrap)
        .collect::<Vec<_>>();

    audited_sources.sort_by_key(|source| source.scene_object_id);
    if let Some(pair) = audited_sources
        .windows(2)
        .find(|pair| pair[0].scene_object_id == pair[1].scene_object_id)
    {
        return Err(ExteriorMesherInputError::DuplicateSourceSceneObject {
            scene_object_id: pair[0].scene_object_id,
        });
    }

    for source in &audited_sources {
        for axis in 0..3 {
            let source_min = source.bounds.min[axis];
            let source_max = source.bounds.max[axis];
            if !source_min.is_finite()
                || !source_max.is_finite()
                || source_min <= domain_min[axis]
                || source_max >= domain_max[axis]
            {
                return Err(ExteriorMesherInputError::SourceTouchesOrLeavesDomain {
                    scene_object_id: source.scene_object_id,
                    axis,
                    source_min,
                    source_max,
                    domain_min: domain_min[axis],
                    domain_max: domain_max[axis],
                });
            }
        }
    }

    let scene_object_ids = audited_sources
        .iter()
        .map(|source| source.scene_object_id)
        .collect::<Vec<_>>();
    let provenance =
        build_scene_owner_marker_provenance(&scene_object_ids, canonical_domain_bindings)?;

    Ok(ValidatedExteriorMesherInput {
        domain_min,
        domain_max,
        audited_sources,
        marker_map: provenance.marker_map,
    })
}

fn safe_marker_tag(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

fn domain_face_index(axis: DomainAxis, side: DomainSide) -> usize {
    let axis_index = match axis {
        DomainAxis::X => 0,
        DomainAxis::Y => 1,
        DomainAxis::Z => 2,
    };
    axis_index * 2
        + match side {
            DomainSide::Min => 0,
            DomainSide::Max => 1,
        }
}

fn domain_face_from_index(index: usize) -> (DomainAxis, DomainSide) {
    let axis = match index / 2 {
        0 => DomainAxis::X,
        1 => DomainAxis::Y,
        _ => DomainAxis::Z,
    };
    let side = if index % 2 == 0 {
        DomainSide::Min
    } else {
        DomainSide::Max
    };
    (axis, side)
}

fn axis_name(axis: usize) -> &'static str {
    match axis {
        0 => "X",
        1 => "Y",
        _ => "Z",
    }
}

fn domain_axis_name(axis: DomainAxis) -> &'static str {
    match axis {
        DomainAxis::X => "X",
        DomainAxis::Y => "Y",
        DomainAxis::Z => "Z",
    }
}

fn domain_side_name(side: DomainSide) -> &'static str {
    match side {
        DomainSide::Min => "Min",
        DomainSide::Max => "Max",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeroforge_geometry_core::SurfaceMesh;

    use crate::imported_surface::{
        audit_imported_surface_for_accurate_meshing, AccurateImportedSurfacePolicy,
    };

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

    fn audited(scene_object_id: u64, offset: [f64; 3]) -> AuditedImportedSurfaceBody {
        audit_imported_surface_for_accurate_meshing(
            scene_object_id,
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
            binding(6, "z_max", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Max),
            binding(2, "outlet", BoundaryRole::Outlet, DomainAxis::X, DomainSide::Max),
            binding(4, "y_max", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Max),
            binding(1, "inlet", BoundaryRole::Inlet, DomainAxis::X, DomainSide::Min),
            binding(5, "z_min", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Min),
            binding(3, "y_min", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Min),
        ]
    }

    #[test]
    fn canonical_input_owns_sources_and_deterministic_marker_provenance() {
        let input = build_validated_exterior_mesher_input(
            [-1.0, -1.0, -1.0],
            [4.0, 4.0, 4.0],
            domain_bindings(),
            vec![audited(77, [2.0, 2.0, 2.0]), audited(42, [0.0, 0.0, 0.0])],
        )
        .unwrap();

        assert_eq!(input.scene_object_ids(), vec![42, 77]);
        assert_eq!(
            input.marker_for_domain_face(DomainAxis::X, DomainSide::Min),
            Some(BoundaryMarkerId(1))
        );
        assert_eq!(input.marker_for_scene_object(42), Some(BoundaryMarkerId(7)));
        assert_eq!(input.marker_for_scene_object(77), Some(BoundaryMarkerId(8)));
        assert!(matches!(
            input.marker_map.bindings[6].source,
            BoundarySource::SceneObject {
                scene_object_id: 42
            }
        ));
    }

    #[test]
    fn all_six_domain_faces_are_required_exactly_once() {
        let mut missing = domain_bindings();
        missing.retain(|binding| {
            !matches!(
                binding.source,
                BoundarySource::DomainFace {
                    axis: DomainAxis::Z,
                    side: DomainSide::Max
                }
            )
        });
        assert!(matches!(
            build_validated_exterior_mesher_input(
                [-1.0; 3],
                [4.0; 3],
                missing,
                vec![audited(42, [0.0; 3])]
            ),
            Err(ExteriorMesherInputError::MissingDomainFace {
                axis: DomainAxis::Z,
                side: DomainSide::Max
            })
        ));

        let mut duplicate = domain_bindings();
        duplicate.push(binding(
            9,
            "x_min_duplicate",
            BoundaryRole::Wall,
            DomainAxis::X,
            DomainSide::Min,
        ));
        assert!(matches!(
            build_validated_exterior_mesher_input(
                [-1.0; 3],
                [4.0; 3],
                duplicate,
                vec![audited(42, [0.0; 3])]
            ),
            Err(ExteriorMesherInputError::DuplicateDomainFace {
                axis: DomainAxis::X,
                side: DomainSide::Min
            })
        ));
    }

    #[test]
    fn source_body_must_be_strictly_inside_domain() {
        let error = build_validated_exterior_mesher_input(
            [-1.0; 3],
            [4.0; 3],
            domain_bindings(),
            vec![audited(42, [-1.0, 0.0, 0.0])],
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ExteriorMesherInputError::SourceTouchesOrLeavesDomain {
                scene_object_id: 42,
                axis: 0,
                ..
            }
        ));
    }

    #[test]
    fn duplicate_source_identity_fails_before_marker_allocation() {
        let error = build_validated_exterior_mesher_input(
            [-1.0; 3],
            [4.0; 3],
            domain_bindings(),
            vec![audited(42, [0.0; 3]), audited(42, [2.0; 3])],
        )
        .unwrap_err();
        assert_eq!(
            error,
            ExteriorMesherInputError::DuplicateSourceSceneObject {
                scene_object_id: 42
            }
        );
    }

    #[test]
    fn domain_binding_provenance_and_role_fail_closed() {
        let mut wrong_source = domain_bindings();
        wrong_source[0].source = BoundarySource::SceneObject {
            scene_object_id: 999,
        };
        assert!(matches!(
            build_validated_exterior_mesher_input(
                [-1.0; 3],
                [4.0; 3],
                wrong_source,
                vec![audited(42, [0.0; 3])]
            ),
            Err(ExteriorMesherInputError::NonDomainBoundaryBinding { .. })
        ));

        let mut custom = domain_bindings();
        custom[0].role = BoundaryRole::Custom;
        assert!(matches!(
            build_validated_exterior_mesher_input(
                [-1.0; 3],
                [4.0; 3],
                custom,
                vec![audited(42, [0.0; 3])]
            ),
            Err(ExteriorMesherInputError::CustomDomainBoundary { .. })
        ));
    }

    #[test]
    fn malformed_domain_or_empty_source_set_fails_closed() {
        let mut invalid_tag = domain_bindings();
        invalid_tag[0].tag = "bad tag".into();
        assert!(matches!(
            build_validated_exterior_mesher_input(
                [-1.0; 3],
                [4.0; 3],
                invalid_tag,
                vec![audited(42, [0.0; 3])]
            ),
            Err(ExteriorMesherInputError::InvalidDomainMarkerTag { .. })
        ));
        assert!(matches!(
            build_validated_exterior_mesher_input(
                [0.0, 0.0, 0.0],
                [0.0, 1.0, 1.0],
                domain_bindings(),
                vec![audited(42, [0.0; 3])]
            ),
            Err(ExteriorMesherInputError::InvalidDomainBounds { axis: 0, .. })
        ));
        assert_eq!(
            build_validated_exterior_mesher_input(
                [-1.0; 3],
                [4.0; 3],
                domain_bindings(),
                Vec::new()
            )
            .unwrap_err(),
            ExteriorMesherInputError::MissingSourceBody
        );
    }
}
