use std::collections::BTreeSet;

use aeroforge_accurate_backend::{
    build_voxel_generated_su2_case_with_reference, scene_object_wall_tag, BoundaryRole,
    BoundarySource, DomainAxis, DomainSide, FlowModel, GeneratedSu2CaseBundle, InletBoundary,
    Su2Case, Su2CoefficientReference, Su2MarkerBinding, VoxelFluidDomainSpec,
};
use aeroforge_volume_core::{BlockBoundaryMarkers, BoundaryMarkerId};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::accurate_scene_geometry::voxelize_project_geometry_for_accurate;
use crate::model::{ProjectState, SolverMode};

pub const ACCURATE_PREPARE_CELL_LIMIT: u64 = 200_000;

#[derive(Clone, Debug, PartialEq)]
pub struct AccurateSettings {
    pub flow_model: FlowModel,
    pub inlet_speed_mps: f64,
    pub temperature_k: f64,
    pub turbulence_intensity: f64,
    pub turbulent_to_laminar_viscosity_ratio: f64,
    pub max_iterations: u32,
    pub convergence_log10: f64,
    /// Explicit SU2 force-coefficient normalization area. Never inferred from staircase geometry.
    pub reference_area_m2: f64,
    /// Explicit SU2 moment-coefficient normalization length. Never inferred from staircase geometry.
    pub reference_length_m: f64,
}

impl Default for AccurateSettings {
    fn default() -> Self {
        Self {
            flow_model: FlowModel::RansSst,
            inlet_speed_mps: 12.0,
            temperature_k: 288.15,
            turbulence_intensity: 0.02,
            turbulent_to_laminar_viscosity_ratio: 10.0,
            max_iterations: 1_000,
            convergence_log10: -6.0,
            reference_area_m2: 1.0,
            reference_length_m: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccuratePrepareStatus {
    Idle,
    Prepared,
    Failed,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreparedCaseSummary {
    pub solid_cells: usize,
    pub active_body_markers: usize,
    pub points: usize,
    pub tetrahedra: usize,
    pub boundary_triangles: usize,
    pub marker_count: usize,
    pub mesh_bytes: usize,
    pub config_bytes: usize,
}

#[derive(Resource)]
pub struct AccurateRuntime {
    pub settings: AccurateSettings,
    pub status: AccuratePrepareStatus,
    pub prepared_revision: Option<u64>,
    pub prepared_settings: Option<AccurateSettings>,
    pub summary: Option<PreparedCaseSummary>,
    pub last_error: Option<String>,
    /// Prepared in-memory mesh/config/provenance bundle. It is not executed automatically.
    pub bundle: Option<GeneratedSu2CaseBundle>,
}

impl Default for AccurateRuntime {
    fn default() -> Self {
        Self {
            settings: AccurateSettings::default(),
            status: AccuratePrepareStatus::Idle,
            prepared_revision: None,
            prepared_settings: None,
            summary: None,
            last_error: None,
            bundle: None,
        }
    }
}

impl AccurateRuntime {
    pub fn is_fresh_for(&self, scene_revision: u64) -> bool {
        self.status == AccuratePrepareStatus::Prepared
            && self.bundle.is_some()
            && self.prepared_revision == Some(scene_revision)
            && self.prepared_settings.as_ref() == Some(&self.settings)
    }
}

pub fn draw_accurate_prepare_ui(
    mut contexts: EguiContexts,
    state: Res<ProjectState>,
    mut runtime: ResMut<AccurateRuntime>,
) -> Result {
    if state.simulation.mode != SolverMode::Accurate {
        return Ok(());
    }

    let ctx = contexts.ctx_mut()?;
    egui::Window::new("Accurate solve — generated SU2 case")
        .default_width(430.0)
        .resizable(true)
        .show(ctx, |ui| {
            ui.label(
                "Generate a closed wind-tunnel SU2 case (X inlet/outlet, Y/Z walls) with scene bodies as wall markers.",
            );
            ui.small(
                "The current generated mesh is a Cartesian staircase tetra mesh. It preserves boundary/object provenance but is not yet a body-fitted engineering-quality mesh.",
            );
            ui.small(
                "Imported surfaces must pass the closed-surface accurate audit before they are rasterized into the same staircase ownership field as analytic primitives.",
            );
            ui.small(
                "Local WindSource volumes/nozzles are not translated to SU2 boundary conditions yet; this case uses the dedicated inlet setting below.",
            );
            ui.separator();

            egui::ComboBox::from_label("Flow model")
                .selected_text(match runtime.settings.flow_model {
                    FlowModel::Laminar => "Laminar",
                    FlowModel::RansSst => "RANS SST",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut runtime.settings.flow_model,
                        FlowModel::Laminar,
                        "Laminar",
                    );
                    ui.selectable_value(
                        &mut runtime.settings.flow_model,
                        FlowModel::RansSst,
                        "RANS SST",
                    );
                });
            ui.horizontal(|ui| {
                ui.label("Inlet speed (m/s)");
                ui.add(
                    egui::DragValue::new(&mut runtime.settings.inlet_speed_mps)
                        .range(0.1..=300.0)
                        .speed(0.1),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Temperature (K)");
                ui.add(
                    egui::DragValue::new(&mut runtime.settings.temperature_k)
                        .range(100.0..=1000.0)
                        .speed(0.5),
                );
            });
            if runtime.settings.flow_model == FlowModel::RansSst {
                ui.horizontal(|ui| {
                    ui.label("Turbulence intensity");
                    ui.add(
                        egui::DragValue::new(&mut runtime.settings.turbulence_intensity)
                            .range(0.0001..=0.5)
                            .speed(0.001),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Turbulent / laminar μ ratio");
                    ui.add(
                        egui::DragValue::new(
                            &mut runtime.settings.turbulent_to_laminar_viscosity_ratio,
                        )
                        .range(1.0..=1000.0)
                        .speed(0.5),
                    );
                });
            }
            ui.horizontal(|ui| {
                ui.label("Max iterations");
                ui.add(
                    egui::DragValue::new(&mut runtime.settings.max_iterations)
                        .range(1..=1_000_000)
                        .speed(10.0),
                );
            });
            ui.horizontal(|ui| {
                ui.label("log10 residual target");
                ui.add(
                    egui::DragValue::new(&mut runtime.settings.convergence_log10)
                        .range(-14.0..=-2.0)
                        .speed(0.1),
                );
            });

            ui.separator();
            ui.label("Coefficient normalization reference (explicit SI)");
            ui.horizontal(|ui| {
                ui.label("Reference area (m²)");
                ui.add(
                    egui::DragValue::new(&mut runtime.settings.reference_area_m2)
                        .range(1.0e-9..=1.0e9)
                        .speed(0.01),
                );
            });
            ui.horizontal(|ui| {
                ui.label("Reference length (m)");
                ui.add(
                    egui::DragValue::new(&mut runtime.settings.reference_length_m)
                        .range(1.0e-9..=1.0e9)
                        .speed(0.01),
                );
            });
            ui.small(
                "These values explicitly set SU2 REF_AREA / REF_LENGTH. AeroForge does not infer them from the voxel mesh, and they do not make CD/CL engineering-valid.",
            );

            ui.separator();
            let cells = state.simulation.cell_count();
            ui.monospace(format!("Voxel cells: {cells}"));
            ui.monospace(format!("Worst-case tetrahedra: {}", cells.saturating_mul(6)));
            let within_budget = cells <= ACCURATE_PREPARE_CELL_LIMIT;
            if !within_budget {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    format!(
                        "Preparation blocked above {ACCURATE_PREPARE_CELL_LIMIT} cells. Grid is never silently reduced."
                    ),
                );
            }

            let prepare = ui
                .add_enabled(within_budget, egui::Button::new("Prepare generated SU2 case"))
                .clicked();
            if prepare {
                let settings_snapshot = runtime.settings.clone();
                match prepare_from_state(&state, &settings_snapshot) {
                    Ok((bundle, summary)) => {
                        runtime.bundle = Some(bundle);
                        runtime.summary = Some(summary);
                        runtime.prepared_revision = Some(state.revision);
                        runtime.prepared_settings = Some(settings_snapshot);
                        runtime.last_error = None;
                        runtime.status = AccuratePrepareStatus::Prepared;
                    }
                    Err(error) => {
                        runtime.bundle = None;
                        runtime.summary = None;
                        runtime.prepared_revision = None;
                        runtime.prepared_settings = None;
                        runtime.last_error = Some(error);
                        runtime.status = AccuratePrepareStatus::Failed;
                    }
                }
            }

            if let Some(prepared_revision) = runtime.prepared_revision {
                if prepared_revision != state.revision {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!(
                            "Prepared case is stale: scene revision {prepared_revision}, current revision {}.",
                            state.revision
                        ),
                    );
                }
            }
            if runtime.prepared_settings.is_some()
                && runtime.prepared_settings.as_ref() != Some(&runtime.settings)
            {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "Prepared case is stale: accurate solver settings changed after preparation.",
                );
            }

            if let Some(summary) = &runtime.summary {
                ui.separator();
                ui.monospace(format!("Solid cells: {}", summary.solid_cells));
                ui.monospace(format!("Active body markers: {}", summary.active_body_markers));
                ui.monospace(format!("Points: {}", summary.points));
                ui.monospace(format!("Tetrahedra: {}", summary.tetrahedra));
                ui.monospace(format!("Boundary triangles: {}", summary.boundary_triangles));
                ui.monospace(format!("SU2 markers: {}", summary.marker_count));
                ui.monospace(format!("Mesh text: {:.2} MiB", summary.mesh_bytes as f64 / 1_048_576.0));
                ui.monospace(format!("Config text: {} bytes", summary.config_bytes));
                ui.small(
                    "Prepared in memory. Persisting and launching SU2 remains a separate explicit action.",
                );
            }
            if let Some(error) = &runtime.last_error {
                ui.colored_label(egui::Color32::RED, format!("Preparation failed: {error}"));
            }
        });

    Ok(())
}

fn prepare_from_state(
    state: &ProjectState,
    settings: &AccurateSettings,
) -> Result<(GeneratedSu2CaseBundle, PreparedCaseSummary), String> {
    let cell_count = state.simulation.cell_count();
    if cell_count > ACCURATE_PREPARE_CELL_LIMIT {
        return Err(format!(
            "requested grid has {cell_count} cells; preparation limit is {ACCURATE_PREPARE_CELL_LIMIT}"
        ));
    }

    let domain_size = state.simulation.domain_size_m;
    let outer_markers = BlockBoundaryMarkers {
        x_min: BoundaryMarkerId(1),
        x_max: BoundaryMarkerId(2),
        y_min: BoundaryMarkerId(3),
        y_max: BoundaryMarkerId(4),
        z_min: BoundaryMarkerId(5),
        z_max: BoundaryMarkerId(6),
    };
    let domain = VoxelFluidDomainSpec {
        min: [
            -0.5 * domain_size.x as f64,
            0.0,
            -0.5 * domain_size.z as f64,
        ],
        max: [
            0.5 * domain_size.x as f64,
            domain_size.y as f64,
            0.5 * domain_size.z as f64,
        ],
        cells: state.simulation.grid.map(|value| value as usize),
        outer_markers,
    };

    let voxelized = voxelize_project_geometry_for_accurate(state, domain)?;

    let active_owner_labels = voxelized
        .solid_owner
        .iter()
        .copied()
        .filter(|&owner| owner != 0)
        .collect::<BTreeSet<_>>();
    let active_scene_ids = active_owner_labels
        .iter()
        .map(|&owner| voxelized.owner_object_ids[owner as usize - 1])
        .collect::<Vec<_>>();

    let mut wall_markers = vec![
        "y_min".to_owned(),
        "y_max".to_owned(),
        "z_min".to_owned(),
        "z_max".to_owned(),
    ];
    wall_markers.extend(active_scene_ids.iter().copied().map(scene_object_wall_tag));

    let case = Su2Case {
        mesh_filename: "aeroforge_generated.su2".into(),
        density_kg_m3: state.simulation.air_density as f64,
        kinematic_viscosity_m2_s: state.simulation.kinematic_viscosity as f64,
        flow_model: settings.flow_model,
        inlets: vec![InletBoundary {
            marker: "inlet".into(),
            temperature_k: settings.temperature_k,
            speed_mps: settings.inlet_speed_mps,
            direction: [1.0, 0.0, 0.0],
            turbulence_intensity: (settings.flow_model == FlowModel::RansSst)
                .then_some(settings.turbulence_intensity),
            turbulent_to_laminar_viscosity_ratio: settings
                .turbulent_to_laminar_viscosity_ratio,
        }],
        outlet_marker: "outlet".into(),
        wall_markers,
        max_iterations: settings.max_iterations,
        convergence_log10: settings.convergence_log10,
        output_basename: "aeroforge_generated".into(),
    };
    let coefficient_reference = Su2CoefficientReference {
        area_m2: settings.reference_area_m2,
        length_m: settings.reference_length_m,
    };

    let generated = build_voxel_generated_su2_case_with_reference(
        &case,
        domain,
        &voxelized.solid_owner,
        &voxelized.owner_object_ids,
        closed_wind_tunnel_bindings(),
        Some(&coefficient_reference),
    )
    .map_err(|error| error.to_string())?;

    let summary = PreparedCaseSummary {
        solid_cells: voxelized.solid_cells,
        active_body_markers: active_scene_ids.len(),
        points: generated.volume_mesh.points.len(),
        tetrahedra: generated.volume_mesh.cells.len(),
        boundary_triangles: generated.volume_mesh.boundary.len(),
        marker_count: generated.bundle.marker_bindings.len(),
        mesh_bytes: generated.bundle.mesh_text.len(),
        config_bytes: generated.bundle.config_text.len(),
    };
    Ok((generated.bundle, summary))
}

fn closed_wind_tunnel_bindings() -> Vec<Su2MarkerBinding> {
    let binding = |marker, tag: &str, role, axis, side| Su2MarkerBinding {
        marker: BoundaryMarkerId(marker),
        tag: tag.into(),
        role,
        source: BoundarySource::DomainFace { axis, side },
    };
    vec![
        binding(
            1,
            "inlet",
            BoundaryRole::Inlet,
            DomainAxis::X,
            DomainSide::Min,
        ),
        binding(
            2,
            "outlet",
            BoundaryRole::Outlet,
            DomainAxis::X,
            DomainSide::Max,
        ),
        binding(
            3,
            "y_min",
            BoundaryRole::Wall,
            DomainAxis::Y,
            DomainSide::Min,
        ),
        binding(
            4,
            "y_max",
            BoundaryRole::Wall,
            DomainAxis::Y,
            DomainSide::Max,
        ),
        binding(
            5,
            "z_min",
            BoundaryRole::Wall,
            DomainAxis::Z,
            DomainSide::Min,
        ),
        binding(
            6,
            "z_max",
            BoundaryRole::Wall,
            DomainAxis::Z,
            DomainSide::Max,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeroforge_geometry_core::SurfaceMesh;

    fn imported_tetra_surface() -> SurfaceMesh {
        SurfaceMesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [2.0, 0.0, 0.0],
                [0.0, 2.0, 0.0],
                [0.0, 0.0, 2.0],
            ],
            triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        }
    }

    #[test]
    fn small_scene_prepares_closed_tunnel_with_object_provenance_and_reference() {
        let mut state = ProjectState::default();
        state.simulation.mode = SolverMode::Accurate;
        state.simulation.grid = [8, 6, 8];
        let (bundle, summary) = prepare_from_state(&state, &AccurateSettings::default()).unwrap();

        assert!(summary.solid_cells > 0);
        assert_eq!(summary.active_body_markers, 1);
        assert!(bundle.config_text.contains("MARKER_INLET= ( inlet"));
        assert!(bundle.config_text.contains("MARKER_OUTLET= ( outlet, 0.0 )"));
        assert!(bundle.config_text.contains("MARKER_MONITORING= ( body_1 )"));
        assert!(bundle.config_text.contains("REF_AREA= 1.000000000000e0"));
        assert!(bundle.config_text.contains("REF_LENGTH= 1.000000000000e0"));
        assert!(bundle.config_text.contains("body_1, 0.0"));
        assert!(bundle.mesh_text.contains("MARKER_TAG= body_1"));
        assert!(bundle.mesh_text.contains("MARKER_TAG= y_min"));
        assert!(bundle.marker_bindings.iter().any(|binding| {
            binding.tag == "body_1"
                && binding.source
                    == BoundarySource::SceneObject {
                        scene_object_id: 1,
                    }
        }));
    }

    #[test]
    fn imported_surface_prepares_with_stable_scene_provenance() {
        let mut state = ProjectState::default();
        state.objects.clear();
        state.simulation.mode = SolverMode::Accurate;
        state.simulation.domain_size_m = Vec3::new(4.0, 4.0, 4.0);
        state.simulation.grid = [4, 4, 4];
        let imported_id = state.add_imported_surface("tetra.obj", imported_tetra_surface());
        state.imported_surfaces[0].position = Vec3::new(-1.0, 1.0, -1.0);
        state.touch();

        let (bundle, summary) = prepare_from_state(&state, &AccurateSettings::default()).unwrap();

        assert!(summary.solid_cells > 0);
        assert_eq!(summary.active_body_markers, 1);
        assert!(bundle
            .config_text
            .contains(&format!("MARKER_MONITORING= ( body_{imported_id} )")));
        assert!(bundle
            .mesh_text
            .contains(&format!("MARKER_TAG= body_{imported_id}")));
        assert!(bundle.marker_bindings.iter().any(|binding| {
            binding.tag == format!("body_{imported_id}")
                && binding.source
                    == BoundarySource::SceneObject {
                        scene_object_id: imported_id,
                    }
        }));
    }

    #[test]
    fn invalid_imported_surface_fails_preparation_closed() {
        let mut state = ProjectState::default();
        state.objects.clear();
        state.simulation.grid = [4, 4, 4];
        state.add_imported_surface(
            "open.obj",
            SurfaceMesh {
                positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                triangles: vec![[0, 1, 2]],
            },
        );

        let error = prepare_from_state(&state, &AccurateSettings::default()).unwrap_err();
        assert!(error.contains("failed accurate audit"));
    }

    #[test]
    fn object_outside_domain_does_not_create_unused_wall_marker() {
        let mut state = ProjectState::default();
        state.simulation.mode = SolverMode::Accurate;
        state.simulation.grid = [8, 6, 8];
        state.objects[0].position = Vec3::new(100.0, 100.0, 100.0);
        let (bundle, summary) = prepare_from_state(&state, &AccurateSettings::default()).unwrap();

        assert_eq!(summary.solid_cells, 0);
        assert_eq!(summary.active_body_markers, 0);
        assert!(!bundle.config_text.contains("body_1"));
        assert!(!bundle.mesh_text.contains("MARKER_TAG= body_1"));
        assert!(bundle.config_text.contains("REF_AREA= 1.000000000000e0"));
        assert!(bundle.config_text.contains("REF_LENGTH= 1.000000000000e0"));
    }

    #[test]
    fn preparation_budget_fails_without_silent_grid_reduction() {
        let mut state = ProjectState::default();
        state.simulation.grid = [100, 100, 100];
        let error = prepare_from_state(&state, &AccurateSettings::default()).unwrap_err();
        assert!(error.contains("preparation limit"));
        assert_eq!(state.simulation.grid, [100, 100, 100]);
    }

    #[test]
    fn invalid_reference_fails_preparation_closed() {
        let mut state = ProjectState::default();
        state.simulation.grid = [8, 6, 8];
        let mut settings = AccurateSettings::default();
        settings.reference_area_m2 = 0.0;
        let error = prepare_from_state(&state, &settings).unwrap_err();
        assert!(error.contains("reference area"));
    }

    #[test]
    fn accurate_setting_change_invalidates_prepared_bundle_freshness() {
        let mut state = ProjectState::default();
        state.simulation.grid = [8, 6, 8];
        let settings = AccurateSettings::default();
        let (bundle, summary) = prepare_from_state(&state, &settings).unwrap();
        let mut runtime = AccurateRuntime {
            settings: settings.clone(),
            status: AccuratePrepareStatus::Prepared,
            prepared_revision: Some(state.revision),
            prepared_settings: Some(settings),
            summary: Some(summary),
            last_error: None,
            bundle: Some(bundle),
        };

        assert!(runtime.is_fresh_for(state.revision));
        runtime.settings.reference_area_m2 += 0.5;
        assert!(!runtime.is_fresh_for(state.revision));
    }
}
