use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_geometry_core::repair::{repair_surface, RepairPolicy, RepairReport};
use aeroforge_geometry_core::{MeshBounds, SurfaceMesh, TopologyReport};

/// Explicit preprocessing policy for imported surfaces entering the accurate-meshing path.
///
/// `weld_tolerance` is expressed in the imported mesh coordinate units. AeroForge deliberately
/// does not infer a tolerance from mesh scale or assume that imported units are SI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AccurateImportedSurfacePolicy {
    pub weld_tolerance: f64,
}

impl Default for AccurateImportedSurfacePolicy {
    fn default() -> Self {
        Self {
            weld_tolerance: 0.0,
        }
    }
}

/// A repaired surface that satisfies the topology prerequisites currently required before a
/// future exterior-fluid volume mesher may consume it.
///
/// This audit is intentionally narrower than body-fitted meshing readiness: it does not detect
/// triangle/triangle self-intersections, CAD defects beyond the bounded repair pass, or prove that
/// a valid exterior fluid volume can be generated around the body.
#[derive(Clone, Debug, PartialEq)]
pub struct AuditedImportedSurfaceBody {
    pub scene_object_id: u64,
    pub mesh: SurfaceMesh,
    pub repair: RepairReport,
    pub topology: TopologyReport,
    pub bounds: MeshBounds,
    pub enclosed_volume: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ImportedSurfaceAuditError {
    Repair(String),
    MultipleConnectedComponents { count: usize },
    MissingPositiveFiniteVolume { value: Option<f64> },
    Bounds(String),
}

impl Display for ImportedSurfaceAuditError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Repair(message) => write!(f, "imported surface repair/audit failed: {message}"),
            Self::MultipleConnectedComponents { count } => write!(
                f,
                "accurate imported body requires exactly one connected closed surface; got {count} components"
            ),
            Self::MissingPositiveFiniteVolume { value } => write!(
                f,
                "accurate imported body requires a positive finite enclosed volume; got {value:?}"
            ),
            Self::Bounds(message) => write!(f, "imported surface bounds failed: {message}"),
        }
    }
}

impl Error for ImportedSurfaceAuditError {}

/// Runs the deterministic bounded repair pass and promotes only a single connected, watertight,
/// consistently oriented, positive-volume surface.
///
/// Promotion is fail-closed. The returned `scene_object_id` is carried directly as provenance;
/// callers must not recover object identity from filenames or future SU2 marker strings.
pub fn audit_imported_surface_for_accurate_meshing(
    scene_object_id: u64,
    surface: &SurfaceMesh,
    policy: AccurateImportedSurfacePolicy,
) -> Result<AuditedImportedSurfaceBody, ImportedSurfaceAuditError> {
    let repaired = repair_surface(
        surface,
        RepairPolicy {
            weld_tolerance: policy.weld_tolerance,
            drop_degenerate_triangles: true,
            drop_duplicate_triangles: true,
            orient_manifold_components: true,
            require_watertight: true,
        },
    )
    .map_err(|error| ImportedSurfaceAuditError::Repair(error.to_string()))?;

    if repaired.topology.connected_components != 1 {
        return Err(ImportedSurfaceAuditError::MultipleConnectedComponents {
            count: repaired.topology.connected_components,
        });
    }

    let enclosed_volume = repaired.topology.signed_volume.ok_or(
        ImportedSurfaceAuditError::MissingPositiveFiniteVolume { value: None },
    )?;
    if !enclosed_volume.is_finite() || enclosed_volume <= 0.0 {
        return Err(ImportedSurfaceAuditError::MissingPositiveFiniteVolume {
            value: Some(enclosed_volume),
        });
    }

    let bounds = repaired
        .mesh
        .bounds()
        .map_err(|error| ImportedSurfaceAuditError::Bounds(error.to_string()))?;

    Ok(AuditedImportedSurfaceBody {
        scene_object_id,
        mesh: repaired.mesh,
        repair: repaired.repair,
        topology: repaired.topology,
        bounds,
        enclosed_volume,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tetra_surface(offset_x: f64) -> SurfaceMesh {
        SurfaceMesh {
            positions: vec![
                [offset_x, 0.0, 0.0],
                [offset_x + 1.0, 0.0, 0.0],
                [offset_x, 1.0, 0.0],
                [offset_x, 0.0, 1.0],
            ],
            triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        }
    }

    #[test]
    fn closed_single_component_surface_is_promoted_with_scene_provenance() {
        let audited = audit_imported_surface_for_accurate_meshing(
            42,
            &tetra_surface(0.0),
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap();

        assert_eq!(audited.scene_object_id, 42);
        assert!(audited.topology.watertight_two_manifold);
        assert!(audited.topology.consistently_oriented);
        assert_eq!(audited.topology.connected_components, 1);
        assert!((audited.enclosed_volume - 1.0 / 6.0).abs() < 1.0e-12);
        assert_eq!(audited.bounds.min, [0.0, 0.0, 0.0]);
        assert_eq!(audited.bounds.max, [1.0, 1.0, 1.0]);
    }

    #[test]
    fn open_surface_fails_closed() {
        let mut open = tetra_surface(0.0);
        open.triangles.pop();
        let error = audit_imported_surface_for_accurate_meshing(
            7,
            &open,
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap_err();

        assert!(matches!(error, ImportedSurfaceAuditError::Repair(_)));
        assert!(error.to_string().contains("open boundary"));
    }

    #[test]
    fn disconnected_closed_shells_are_not_silently_treated_as_one_body() {
        let first = tetra_surface(0.0);
        let second = tetra_surface(3.0);
        let mut combined = SurfaceMesh {
            positions: first.positions,
            triangles: first.triangles,
        };
        let base = combined.positions.len() as u32;
        combined.positions.extend(second.positions);
        combined.triangles.extend(second.triangles.into_iter().map(|triangle| {
            [triangle[0] + base, triangle[1] + base, triangle[2] + base]
        }));

        let error = audit_imported_surface_for_accurate_meshing(
            9,
            &combined,
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap_err();

        assert_eq!(
            error,
            ImportedSurfaceAuditError::MultipleConnectedComponents { count: 2 }
        );
    }

    #[test]
    fn non_finite_weld_tolerance_is_rejected_by_repair_contract() {
        let error = audit_imported_surface_for_accurate_meshing(
            3,
            &tetra_surface(0.0),
            AccurateImportedSurfacePolicy {
                weld_tolerance: f64::NAN,
            },
        )
        .unwrap_err();

        assert!(matches!(error, ImportedSurfaceAuditError::Repair(_)));
        assert!(error.to_string().contains("weld tolerance"));
    }
}
