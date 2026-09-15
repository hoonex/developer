use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::BoundaryMarkerId;

use crate::boundary_layer::GeneratedTetrahedralBoundaryLayer;
use crate::exterior_mesher_admission::{
    validate_exterior_mesher_input_intersections, ExteriorMesherAdmissionError,
};
use crate::exterior_mesher_input::{
    build_validated_exterior_mesher_input, ExteriorMesherInputError,
};
use crate::imported_surface::{
    audit_imported_surface_for_accurate_meshing, AccurateImportedSurfacePolicy,
    ImportedSurfaceAuditError,
};
use crate::source_containment::{
    validate_exterior_mesher_source_containment, ContainmentValidatedExteriorMesherInput,
    SourceContainmentError,
};
use crate::su2_mesh::BoundarySource;

/// Errors from replacing physical source shells with generated outer boundary-layer interfaces for
/// a second-stage TetGen far-field tetrahedralization.
#[derive(Clone, Debug, PartialEq)]
pub enum BoundaryLayerTetgenInputError {
    DuplicateLayer { scene_object_id: u64 },
    MissingLayer { scene_object_id: u64 },
    UnknownLayer { scene_object_id: u64 },
    MissingSceneMarker { scene_object_id: u64 },
    WallMarkerMismatch {
        scene_object_id: u64,
        expected: BoundaryMarkerId,
        actual: BoundaryMarkerId,
    },
    OuterSurfaceAudit(ImportedSurfaceAuditError),
    BaseInput(ExteriorMesherInputError),
    Admission(ExteriorMesherAdmissionError),
    Containment(SourceContainmentError),
    MarkerProvenanceChanged,
}

impl Display for BoundaryLayerTetgenInputError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateLayer { scene_object_id } => write!(
                f,
                "more than one boundary-layer block was supplied for SceneObject {scene_object_id}"
            ),
            Self::MissingLayer { scene_object_id } => write!(
                f,
                "no boundary-layer block was supplied for admitted SceneObject {scene_object_id}"
            ),
            Self::UnknownLayer { scene_object_id } => write!(
                f,
                "boundary-layer block references unknown SceneObject {scene_object_id}"
            ),
            Self::MissingSceneMarker { scene_object_id } => write!(
                f,
                "admitted exterior input has no wall marker for SceneObject {scene_object_id}"
            ),
            Self::WallMarkerMismatch {
                scene_object_id,
                expected,
                actual,
            } => write!(
                f,
                "SceneObject {scene_object_id} boundary-layer wall marker {:?} does not match admitted marker {:?}",
                actual, expected
            ),
            Self::OuterSurfaceAudit(error) => {
                write!(f, "generated outer boundary-layer interface audit failed: {error}")
            }
            Self::BaseInput(error) => write!(
                f,
                "generated outer interfaces no longer satisfy the exterior-mesher base contract: {error}"
            ),
            Self::Admission(error) => write!(
                f,
                "generated outer interfaces failed exterior-mesher intersection admission: {error}"
            ),
            Self::Containment(error) => write!(
                f,
                "generated outer interfaces failed exterior-mesher containment admission: {error}"
            ),
            Self::MarkerProvenanceChanged => write!(
                f,
                "rebuilding TetGen input around boundary-layer interfaces changed canonical marker provenance"
            ),
        }
    }
}

impl Error for BoundaryLayerTetgenInputError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::OuterSurfaceAudit(error) => Some(error),
            Self::BaseInput(error) => Some(error),
            Self::Admission(error) => Some(error),
            Self::Containment(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ImportedSurfaceAuditError> for BoundaryLayerTetgenInputError {
    fn from(value: ImportedSurfaceAuditError) -> Self {
        Self::OuterSurfaceAudit(value)
    }
}

impl From<ExteriorMesherInputError> for BoundaryLayerTetgenInputError {
    fn from(value: ExteriorMesherInputError) -> Self {
        Self::BaseInput(value)
    }
}

impl From<ExteriorMesherAdmissionError> for BoundaryLayerTetgenInputError {
    fn from(value: ExteriorMesherAdmissionError) -> Self {
        Self::Admission(value)
    }
}

impl From<SourceContainmentError> for BoundaryLayerTetgenInputError {
    fn from(value: SourceContainmentError) -> Self {
        Self::Containment(value)
    }
}

/// Rebuilds a previously admitted exterior-mesher request so TetGen sees each generated outer
/// boundary-layer shell instead of the original physical wall.
///
/// The original layer block remains the owner of the physical wall. This function only constructs
/// the *far-field* TetGen side of the future merge contract. Every outer interface is independently
/// re-audited as a closed positive-volume shell, then the canonical domain/source input,
/// self/inter-body intersection gate, and nested-body containment gate are all rerun. Stable scene
/// identity and the complete marker map must remain unchanged.
///
/// Passing does not merge the layer block with TetGen output, prove a positive post-extrusion body
/// clearance, establish y+ adequacy, or promote mesh fidelity. Those remain later gates.
pub fn rebuild_tetgen_input_around_boundary_layers(
    input: &ContainmentValidatedExteriorMesherInput,
    layers: &[GeneratedTetrahedralBoundaryLayer],
    outer_surface_audit_policy: AccurateImportedSurfacePolicy,
) -> Result<ContainmentValidatedExteriorMesherInput, BoundaryLayerTetgenInputError> {
    let admission = input.admission();
    let source_ids = admission.scene_object_ids().into_iter().collect::<BTreeSet<_>>();
    let mut by_scene = BTreeMap::<u64, &GeneratedTetrahedralBoundaryLayer>::new();
    for layer in layers {
        let scene_object_id = layer.report.scene_object_id;
        if !source_ids.contains(&scene_object_id) {
            return Err(BoundaryLayerTetgenInputError::UnknownLayer { scene_object_id });
        }
        if by_scene.insert(scene_object_id, layer).is_some() {
            return Err(BoundaryLayerTetgenInputError::DuplicateLayer { scene_object_id });
        }
    }

    let mut outer_sources = Vec::with_capacity(admission.audited_sources().len());
    for source in admission.audited_sources() {
        let scene_object_id = source.scene_object_id;
        let layer = by_scene
            .get(&scene_object_id)
            .copied()
            .ok_or(BoundaryLayerTetgenInputError::MissingLayer { scene_object_id })?;
        let expected_marker = scene_marker(input, scene_object_id)
            .ok_or(BoundaryLayerTetgenInputError::MissingSceneMarker { scene_object_id })?;
        if layer.wall_marker != expected_marker {
            return Err(BoundaryLayerTetgenInputError::WallMarkerMismatch {
                scene_object_id,
                expected: expected_marker,
                actual: layer.wall_marker,
            });
        }
        outer_sources.push(audit_imported_surface_for_accurate_meshing(
            scene_object_id,
            &layer.outer_surface,
            outer_surface_audit_policy,
        )?);
    }

    let domain_bindings = admission
        .marker_map()
        .bindings
        .iter()
        .filter(|binding| matches!(&binding.source, BoundarySource::DomainFace { .. }))
        .cloned()
        .collect();
    let rebuilt = build_validated_exterior_mesher_input(
        admission.domain_min(),
        admission.domain_max(),
        domain_bindings,
        outer_sources,
    )?;
    if &rebuilt.marker_map != admission.marker_map() {
        return Err(BoundaryLayerTetgenInputError::MarkerProvenanceChanged);
    }

    let rebuilt = validate_exterior_mesher_input_intersections(
        rebuilt,
        admission.source_intersection_policy(),
    )?;
    Ok(validate_exterior_mesher_source_containment(
        rebuilt,
        input.containment_policy(),
    )?)
}

fn scene_marker(
    input: &ContainmentValidatedExteriorMesherInput,
    scene_object_id: u64,
) -> Option<BoundaryMarkerId> {
    input
        .admission()
        .marker_map()
        .bindings
        .iter()
        .find_map(|binding| match &binding.source {
            BoundarySource::SceneObject {
                scene_object_id: candidate,
            } if *candidate == scene_object_id => Some(binding.marker),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeroforge_geometry_core::SurfaceMesh;

    use crate::boundary_layer::{
        generate_tetrahedral_boundary_layer, TetrahedralBoundaryLayerPolicy,
    };
    use crate::exterior_mesher_input::build_validated_exterior_mesher_input;
    use crate::source_intersection::SourceSurfaceIntersectionPolicy;
    use crate::su2_mesh::{
        BoundaryRole, DomainAxis, DomainSide, Su2MarkerBinding,
    };
    use crate::tetgen_plc::{prepare_tetgen_plc, TetgenHoleSeedPolicy};

    fn cube_surface() -> SurfaceMesh {
        SurfaceMesh {
            positions: vec![
                [1.0, 1.0, 1.0],
                [2.0, 1.0, 1.0],
                [2.0, 2.0, 1.0],
                [1.0, 2.0, 1.0],
                [1.0, 1.0, 2.0],
                [2.0, 1.0, 2.0],
                [2.0, 2.0, 2.0],
                [1.0, 2.0, 2.0],
            ],
            triangles: vec![
                [0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7],
                [0, 1, 5], [0, 5, 4], [3, 7, 6], [3, 6, 2],
                [0, 4, 7], [0, 7, 3], [1, 2, 6], [1, 6, 5],
            ],
        }
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

    fn admitted(domain_min: [f64; 3], domain_max: [f64; 3]) -> ContainmentValidatedExteriorMesherInput {
        let body = audit_imported_surface_for_accurate_meshing(
            42,
            &cube_surface(),
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap();
        let base = build_validated_exterior_mesher_input(
            domain_min,
            domain_max,
            domain_bindings(),
            vec![body],
        )
        .unwrap();
        let intersection = validate_exterior_mesher_input_intersections(
            base,
            SourceSurfaceIntersectionPolicy {
                geometric_epsilon: 1.0e-10,
                max_triangle_pair_tests: 10_000,
            },
        )
        .unwrap();
        validate_exterior_mesher_source_containment(
            intersection,
            crate::source_containment::SourceContainmentPolicy {
                geometric_epsilon: 1.0e-10,
                max_point_triangle_tests: 10_000,
            },
        )
        .unwrap()
    }

    fn layer(input: &ContainmentValidatedExteriorMesherInput, first: f64, count: usize) -> GeneratedTetrahedralBoundaryLayer {
        let body = &input.admission().audited_sources()[0];
        generate_tetrahedral_boundary_layer(
            body,
            BoundaryMarkerId(7),
            BoundaryMarkerId(99),
            TetrahedralBoundaryLayerPolicy {
                first_layer_thickness: first,
                growth_ratio: 1.0,
                layer_count: count,
                maximum_total_thickness: first * count as f64 * 1.000_001,
                maximum_adjacent_face_normal_angle_radians: std::f64::consts::PI,
                minimum_tetrahedron_volume: 1.0e-12,
                max_generated_tetrahedra: 10_000,
                overlap_geometric_epsilon: 1.0e-10,
                max_overlap_pair_tests: 1_000_000,
            },
        )
        .unwrap()
    }

    #[test]
    fn outer_interface_reaches_tetgen_plc_with_stable_scene_marker() {
        let input = admitted([0.0, 0.0, 0.0], [3.0, 3.0, 3.0]);
        let layer = layer(&input, 0.05, 2);
        let rebuilt = rebuild_tetgen_input_around_boundary_layers(
            &input,
            std::slice::from_ref(&layer),
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap();

        assert_eq!(rebuilt.admission().marker_map(), input.admission().marker_map());
        assert_eq!(rebuilt.scene_object_ids(), vec![42]);
        assert_eq!(
            rebuilt.admission().audited_sources()[0].mesh.positions,
            layer.outer_surface.positions
        );
        assert_eq!(layer.mesh.boundary.iter().filter(|face| face.marker == BoundaryMarkerId(7)).count(), 12);

        let plc = prepare_tetgen_plc(
            &rebuilt,
            TetgenHoleSeedPolicy {
                geometric_epsilon: 1.0e-12,
                initial_inward_edge_fraction: 0.05,
                max_attempts: 12,
                max_point_triangle_tests: 10_000,
            },
        )
        .unwrap();
        assert_eq!(plc.point_count(), 16);
        assert_eq!(plc.facet_count(), 18);
        assert!(plc.poly_text().contains("1 0 7\n"));
    }

    #[test]
    fn expanded_outer_interface_leaving_domain_fails_closed() {
        let input = admitted([0.9, 0.9, 0.9], [2.1, 2.1, 2.1]);
        let layer = layer(&input, 0.2, 1);
        let error = rebuild_tetgen_input_around_boundary_layers(
            &input,
            &[layer],
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            BoundaryLayerTetgenInputError::BaseInput(
                ExteriorMesherInputError::SourceTouchesOrLeavesDomain { .. }
            )
        ));
    }

    #[test]
    fn layer_wall_marker_must_match_admitted_scene_marker() {
        let input = admitted([0.0, 0.0, 0.0], [3.0, 3.0, 3.0]);
        let body = &input.admission().audited_sources()[0];
        let wrong = generate_tetrahedral_boundary_layer(
            body,
            BoundaryMarkerId(8),
            BoundaryMarkerId(99),
            TetrahedralBoundaryLayerPolicy {
                first_layer_thickness: 0.05,
                growth_ratio: 1.0,
                layer_count: 1,
                maximum_total_thickness: 0.05,
                maximum_adjacent_face_normal_angle_radians: std::f64::consts::PI,
                minimum_tetrahedron_volume: 1.0e-12,
                max_generated_tetrahedra: 10_000,
                overlap_geometric_epsilon: 1.0e-10,
                max_overlap_pair_tests: 1_000_000,
            },
        )
        .unwrap();
        assert!(matches!(
            rebuild_tetgen_input_around_boundary_layers(
                &input,
                &[wrong],
                AccurateImportedSurfacePolicy::default(),
            ),
            Err(BoundaryLayerTetgenInputError::WallMarkerMismatch { .. })
        ));
    }
}
