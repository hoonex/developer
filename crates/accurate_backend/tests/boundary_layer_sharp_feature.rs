use aeroforge_accurate_backend::{
    audit_imported_surface_for_accurate_meshing, generate_tetrahedral_boundary_layer,
    AccurateImportedSurfacePolicy, TetrahedralBoundaryLayerPolicy,
};
use aeroforge_geometry_core::SurfaceMesh;
use aeroforge_volume_core::BoundaryMarkerId;

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

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[test]
fn sharp_cube_corner_preserves_requested_incident_face_normal_spacing() {
    let surface = cube_surface();
    let body = audit_imported_surface_for_accurate_meshing(
        42,
        &surface,
        AccurateImportedSurfacePolicy::default(),
    )
    .expect("cube must pass accurate imported-surface audit");
    let requested = 0.05_f64;
    let generated = generate_tetrahedral_boundary_layer(
        &body,
        BoundaryMarkerId(7),
        BoundaryMarkerId(8),
        TetrahedralBoundaryLayerPolicy {
            first_layer_thickness: requested,
            growth_ratio: 1.0,
            layer_count: 1,
            maximum_total_thickness: requested,
            maximum_adjacent_face_normal_angle_radians: std::f64::consts::PI,
            minimum_tetrahedron_volume: 1.0e-12,
            max_generated_tetrahedra: 1_000,
            overlap_geometric_epsilon: 1.0e-10,
            max_overlap_pair_tests: 100_000,
        },
    )
    .expect("miter-scaled sharp cube layer must generate");

    let source = surface.positions[0];
    let first_layer = generated.mesh.points[surface.positions.len()];
    let displacement = [
        first_layer[0] - source[0],
        first_layer[1] - source[1],
        first_layer[2] - source[2],
    ];
    let outward_face_normals = [
        [-1.0, 0.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 0.0, -1.0],
    ];
    for normal in outward_face_normals {
        let projection = dot(displacement, normal);
        assert!((projection - requested).abs() < 1.0e-12);
    }
    assert!((generated.report.maximum_vertex_normal_amplification - 3.0_f64.sqrt()).abs() < 1.0e-12);

    println!(
        "AEROFORGE_BOUNDARY_LAYER_SHARP_FEATURE_SPACING=PASS requested={} minimum_projection={} amplification={}",
        requested,
        generated.report.minimum_vertex_face_normal_projection,
        generated.report.maximum_vertex_normal_amplification,
    );
}
