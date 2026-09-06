use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::BoundaryMarkerId;

use crate::su2_mesh::{BoundaryRole, BoundarySource, Su2MarkerBinding, Su2MarkerMap};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneOwnerMarkerProvenance {
    /// Compact owner label N -> allocated boundary marker.
    pub owner_markers: BTreeMap<u32, BoundaryMarkerId>,
    /// Domain bindings followed by deterministic object-wall bindings.
    pub marker_map: Su2MarkerMap,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SceneOwnerProvenanceError {
    ZeroDomainMarker,
    DuplicateDomainMarker(u32),
    DuplicateTag(String),
    OwnerIdsNotStrictlyIncreasing,
    TooManyOwners,
    MarkerIdOverflow,
}

impl Display for SceneOwnerProvenanceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroDomainMarker => write!(f, "domain boundary marker id 0 is reserved"),
            Self::DuplicateDomainMarker(marker) => {
                write!(f, "domain boundary marker id {marker} is duplicated")
            }
            Self::DuplicateTag(tag) => write!(f, "boundary tag `{tag}` is duplicated"),
            Self::OwnerIdsNotStrictlyIncreasing => write!(
                f,
                "scene owner ids must be strictly increasing to match compact raster ownership labels"
            ),
            Self::TooManyOwners => write!(f, "scene has too many owners for compact u32 labels"),
            Self::MarkerIdOverflow => write!(f, "boundary marker id allocation overflowed u32"),
        }
    }
}

impl Error for SceneOwnerProvenanceError {}

/// Extends already-defined domain boundary bindings with deterministic wall bindings for the
/// compact solid-owner labels used by the preview rasterizer. `owner_object_ids[N - 1]` is the
/// stable `SceneObject.id` represented by compact owner label N.
///
/// Object marker ids are allocated consecutively above the largest domain marker id and tags are
/// `body_<scene_object_id>`. Requiring strictly increasing scene ids mirrors the rasterizer's
/// stable-id sort and makes the provenance mapping independent of scene-vector ordering.
pub fn build_scene_owner_marker_provenance(
    owner_object_ids: &[u64],
    domain_bindings: Vec<Su2MarkerBinding>,
) -> Result<SceneOwnerMarkerProvenance, SceneOwnerProvenanceError> {
    if owner_object_ids.len() >= u32::MAX as usize {
        return Err(SceneOwnerProvenanceError::TooManyOwners);
    }
    if owner_object_ids
        .windows(2)
        .any(|pair| pair[0] >= pair[1])
    {
        return Err(SceneOwnerProvenanceError::OwnerIdsNotStrictlyIncreasing);
    }

    let mut used_markers = BTreeSet::<u32>::new();
    let mut used_tags = BTreeSet::<String>::new();
    let mut largest_marker = 0_u32;
    for binding in &domain_bindings {
        if binding.marker.0 == 0 {
            return Err(SceneOwnerProvenanceError::ZeroDomainMarker);
        }
        if !used_markers.insert(binding.marker.0) {
            return Err(SceneOwnerProvenanceError::DuplicateDomainMarker(binding.marker.0));
        }
        if !used_tags.insert(binding.tag.clone()) {
            return Err(SceneOwnerProvenanceError::DuplicateTag(binding.tag.clone()));
        }
        largest_marker = largest_marker.max(binding.marker.0);
    }

    let mut bindings = domain_bindings;
    let mut owner_markers = BTreeMap::<u32, BoundaryMarkerId>::new();
    for (owner_index, &scene_object_id) in owner_object_ids.iter().enumerate() {
        let owner = u32::try_from(owner_index + 1)
            .map_err(|_| SceneOwnerProvenanceError::TooManyOwners)?;
        largest_marker = largest_marker
            .checked_add(1)
            .ok_or(SceneOwnerProvenanceError::MarkerIdOverflow)?;
        let marker = BoundaryMarkerId(largest_marker);
        let tag = format!("body_{scene_object_id}");
        if !used_tags.insert(tag.clone()) {
            return Err(SceneOwnerProvenanceError::DuplicateTag(tag));
        }
        owner_markers.insert(owner, marker);
        bindings.push(Su2MarkerBinding {
            marker,
            tag,
            role: BoundaryRole::Wall,
            source: BoundarySource::SceneObject { scene_object_id },
        });
    }

    Ok(SceneOwnerMarkerProvenance {
        owner_markers,
        marker_map: Su2MarkerMap { bindings },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::su2_mesh::{DomainAxis, DomainSide};

    fn domain_bindings() -> Vec<Su2MarkerBinding> {
        vec![
            Su2MarkerBinding {
                marker: BoundaryMarkerId(1),
                tag: "inlet".into(),
                role: BoundaryRole::Inlet,
                source: BoundarySource::DomainFace {
                    axis: DomainAxis::X,
                    side: DomainSide::Min,
                },
            },
            Su2MarkerBinding {
                marker: BoundaryMarkerId(9),
                tag: "outlet".into(),
                role: BoundaryRole::Outlet,
                source: BoundarySource::DomainFace {
                    axis: DomainAxis::X,
                    side: DomainSide::Max,
                },
            },
        ]
    }

    #[test]
    fn compact_owner_labels_map_to_stable_scene_ids_and_noncolliding_markers() {
        let result = build_scene_owner_marker_provenance(
            &[7, 0x1_0000_0001],
            domain_bindings(),
        )
        .unwrap();
        assert_eq!(result.owner_markers[&1], BoundaryMarkerId(10));
        assert_eq!(result.owner_markers[&2], BoundaryMarkerId(11));
        assert_eq!(result.marker_map.bindings[2].tag, "body_7");
        assert_eq!(result.marker_map.bindings[3].tag, "body_4294967297");
        assert_eq!(
            result.marker_map.bindings[3].source,
            BoundarySource::SceneObject {
                scene_object_id: 0x1_0000_0001,
            }
        );
    }

    #[test]
    fn unsorted_or_duplicate_scene_ids_are_rejected() {
        assert_eq!(
            build_scene_owner_marker_provenance(&[4, 3], domain_bindings()),
            Err(SceneOwnerProvenanceError::OwnerIdsNotStrictlyIncreasing)
        );
        assert_eq!(
            build_scene_owner_marker_provenance(&[4, 4], domain_bindings()),
            Err(SceneOwnerProvenanceError::OwnerIdsNotStrictlyIncreasing)
        );
    }

    #[test]
    fn duplicate_domain_marker_is_rejected_before_object_allocation() {
        let mut bindings = domain_bindings();
        bindings[1].marker = BoundaryMarkerId(1);
        assert_eq!(
            build_scene_owner_marker_provenance(&[5], bindings),
            Err(SceneOwnerProvenanceError::DuplicateDomainMarker(1))
        );
    }
}
