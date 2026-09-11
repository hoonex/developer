use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::thread;

use aeroforge_accurate_backend::{
    build_voxel_generated_su2_case_with_reference, scene_object_wall_tag, BoundaryRole,
    BoundarySource, DomainAxis, DomainSide, FlowModel, GeneratedSu2CaseBundle, InletBoundary,
    Su2Case, Su2CoefficientReference, Su2MarkerBinding, VoxelFluidDomainSpec,
};
use aeroforge_volume_core::{BlockBoundaryMarkers, BoundaryMarkerId};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::accurate_boundary_layer_prepare::{
    prepare_boundary_layer_tetgen_from_state, LIMITED_ALPHA_BOUNDARY_LAYER_PRESET_LABEL,
};
use crate::accurate_prepared_case::AccuratePreparedCase;
use crate::accurate_scene_geometry::voxelize_project_geometry_for_accurate;
use crate::accurate_tetgen_prepare::{prepare_tetgen_from_state, snapshot_project_state};
use crate::model::{ProjectState, SolverMode};

pub const ACCURATE_PREPARE_CELL_LIMIT: u64 = 200_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccurateMeshPath {
    Staircase,
    ValidatedTetgen,
    BoundaryLayerTetgen,
}

impl AccurateMeshPath {
    fn label(self) -> &'static str {
        match self {
            Self::Staircase => "Cartesian staircase",
            Self::ValidatedTetgen => "Validated external TetGen",
            Self::BoundaryLayerTetgen => "Boundary layer + external TetGen",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AccurateSettings {
    pub flow_model: FlowModel,
    pub inlet_speed_mps: f64,
    pub temperature_k: f64,
    pub turbulence_intensity: f64,
    pub turbulent_to_laminar_viscosity_ratio: f64,
    pub max_iterations: u32,
    pub convergence_log10: f64,
    /// Explicit SU2 force-coefficient normalization area. Never inferred from prepared geometry.
    pub reference_area_m2: f64,
    /// Explicit SU2 moment-coefficient normalization length. Never inferred from prepared geometry.
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
    Preparing,
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

#[derive(Debug)]
struct AccuratePrepareCompletion {
    revision: u64,
    settings: AccurateSettings,
    mesh_path: AccurateMeshPath,
    result: Result<(AccuratePreparedCase, PreparedCaseSummary), String>,
}

#[derive(Resource)]
pub struct AccurateRuntime {
    pub settings: AccurateSettings,
    pub selected_mesh_path: AccurateMeshPath,
    pub status: AccuratePrepareStatus,
    pub preparing_revision: Option<u64>,
    pub prepared_revision: Option<u64>,
    pub prepared_settings: Option<AccurateSettings>,
    pub prepared_mesh_path: Option<AccurateMeshPath>,
    pub summary: Option<PreparedCaseSummary>,
    pub last_error: Option<String>,
    pub prepared_case: Option<AccuratePreparedCase>,
    completion: Arc<Mutex<Option<AccuratePrepareCompletion>>>,
}

impl Default for AccurateRuntime {
    fn default() -> Self {
        Self {
            settings: AccurateSettings::default(),
            selected_mesh_path: AccurateMeshPath::Staircase,
            status: AccuratePrepareStatus::Idle,
            preparing_revision: None,
            prepared_revision: None,
            prepared_settings: None,
            prepared_mesh_path: None,
            summary: None,
            last_error: None,
            prepared_case: None,
            completion: Arc::new(Mutex::new(None)),
        }
    }
}

impl AccurateRuntime {
    pub fn is_fresh_for(&self, scene_revision: u64) -> bool {
        self.status == AccuratePrepareStatus::Prepared
            && self.prepared_case.is_some()
            && self.prepared_revision == Some(scene_revision)
            && self.prepared_settings.as_ref() == Some(&self.settings)
            && self.prepared_mesh_path == Some(self.selected_mesh_path)
    }
}

/// Polls background accurate preparation independently of which solve-workspace tab is visible.
/// A completed TetGen artifact is therefore promoted (or rejected) promptly even when the user
/// navigates away from the Prepare tab while the external mesher is running.
pub fn poll_accurate_prepare_completion(mut runtime: ResMut<AccurateRuntime>) {
    collect_prepare_completion(&mut runtime);
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
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("Prepare generated SU2 case");
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("accurate_prepare_scroll")
            .show(ui, |ui| {
                ui.label(
                    "Generate a closed wind-tunnel SU2 case (X inlet/outlet, Y/Z walls) with scene bodies as wall markers.",
                );
                ui.small(
                    "Local WindSource volumes/nozzles are not translated to SU2 boundary conditions yet; this case uses the dedicated inlet setting below.",
                );
                ui.separator();

                egui::ComboBox::from_label("Mesh path")
                    .selected_text(runtime.selected_mesh_path.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut runtime.selected_mesh_path,
                            AccurateMeshPath::Staircase,
                            AccurateMeshPath::Staircase.label(),
                        );
                        ui.selectable_value(
                            &mut runtime.selected_mesh_path,
                            AccurateMeshPath::ValidatedTetgen,
                            AccurateMeshPath::ValidatedTetgen.label(),
                        );
                        ui.selectable_value(
                            &mut runtime.selected_mesh_path,
                            AccurateMeshPath::BoundaryLayerTetgen,
                            AccurateMeshPath::BoundaryLayerTetgen.label(),
                        );
                    });

                match runtime.selected_mesh_path {
                    AccurateMeshPath::Staircase => {
                        ui.small(
                            "Cartesian voxel/staircase tetrahedra. Boundary/object provenance is retained, but the mesh is not body-fitted or engineering-quality.",
                        );
                        ui.small(
                            "Imported surfaces must pass the closed-surface accurate audit before rasterization into the ownership field.",
                        );
                    }
                    AccurateMeshPath::ValidatedTetgen => {
                        ui.small(
                            "Runs a user-installed TetGen executable in a worker thread after strict source intersection/containment admission, then validates local tetra quality and source correspondence.",
                        );
                        ui.small(
                            "Set TETGEN_EXECUTABLE or place tetgen(.exe) on PATH. AeroForge does not download or bundle TetGen. Current validation still records body_fitted_status=not_established and engineering_quality_status=not_established.",
                        );
                        ui.small(
                            "Source bodies must lie strictly inside the outer domain: touching the tunnel floor/walls is rejected by this exterior-mesher path.",
                        );
                    }
                    AccurateMeshPath::BoundaryLayerTetgen => {
                        ui.small(
                            "Experimental limited-alpha path: generates tetrahedral wall-normal layers, remeshes the expanded outer interfaces with user-installed TetGen, welds both regions, then validates the final solver mesh against the original physical walls.",
                        );
                        ui.monospace(format!(
                            "Validated preset: {LIMITED_ALPHA_BOUNDARY_LAYER_PRESET_LABEL}"
                        ));
                        ui.small(
                            "The preset is a proven geometric smoke configuration, not an inferred y+ target or engineering mesh prescription. body_fitted_status, engineering_quality_status and y_plus_status remain not_established.",
                        );
                        ui.small(
                            "Set TETGEN_EXECUTABLE or place tetgen(.exe) on PATH. Source bodies must remain strictly inside the outer domain after layer expansion.",
                        );
                    }
                }
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
                    "These values explicitly set SU2 REF_AREA / REF_LENGTH. AeroForge does not infer them from prepared geometry, and they do not make CD/CL engineering-valid.",
                );

                ui.separator();
                let preparing = runtime.status == AccuratePrepareStatus::Preparing;
                let cells = state.simulation.cell_count();
                let staircase_within_budget = cells <= ACCURATE_PREPARE_CELL_LIMIT;
                if runtime.selected_mesh_path == AccurateMeshPath::Staircase {
                    ui.monospace(format!("Voxel cells: {cells}"));
                    ui.monospace(format!(
                        "Worst-case tetrahedra: {}",
                        cells.saturating_mul(6)
                    ));
                    if !staircase_within_budget {
                        ui.colored_label(
                            egui::Color32::YELLOW,
                            format!(
                                "Staircase preparation blocked above {ACCURATE_PREPARE_CELL_LIMIT} cells. Grid is never silently reduced."
                            ),
                        );
                    }
                }

                let can_prepare = !preparing
                    && (runtime.selected_mesh_path != AccurateMeshPath::Staircase
                        || staircase_within_budget);
                let prepare_label = match runtime.selected_mesh_path {
                    AccurateMeshPath::Staircase => "Prepare staircase SU2 case",
                    AccurateMeshPath::ValidatedTetgen => "Prepare validated TetGen SU2 case",
                    AccurateMeshPath::BoundaryLayerTetgen => {
                        "Prepare boundary-layer + TetGen SU2 case"
                    }
                };
                let prepare = ui
                    .add_enabled(can_prepare, egui::Button::new(prepare_label))
                    .clicked();

                if prepare {
                    let settings_snapshot = runtime.settings.clone();
                    match runtime.selected_mesh_path {
                        AccurateMeshPath::Staircase => {
                            match prepare_staircase_from_state(&state, &settings_snapshot) {
                                Ok((bundle, summary)) => {
                                    runtime.prepared_case =
                                        Some(AccuratePreparedCase::staircase(bundle));
                                    runtime.summary = Some(summary);
                                    runtime.prepared_revision = Some(state.revision);
                                    runtime.prepared_settings = Some(settings_snapshot);
                                    runtime.prepared_mesh_path = Some(AccurateMeshPath::Staircase);
                                    runtime.preparing_revision = None;
                                    runtime.last_error = None;
                                    runtime.status = AccuratePrepareStatus::Prepared;
                                }
                                Err(error) => {
                                    runtime.preparing_revision = None;
                                    runtime.last_error = Some(error);
                                    runtime.status = AccuratePrepareStatus::Failed;
                                }
                            }
                        }
                        AccurateMeshPath::ValidatedTetgen => {
                            launch_external_prepare(
                                &mut runtime,
                                &state,
                                settings_snapshot,
                                AccurateMeshPath::ValidatedTetgen,
                            );
                        }
                        AccurateMeshPath::BoundaryLayerTetgen => {
                            launch_external_prepare(
                                &mut runtime,
                                &state,
                                settings_snapshot,
                                AccurateMeshPath::BoundaryLayerTetgen,
                            );
                        }
                    }
                }

                if runtime.status == AccuratePrepareStatus::Preparing {
                    ui.separator();
                    ui.label(format!(
                        "External mesh preparation: running scene revision {}",
                        runtime.preparing_revision.unwrap_or_default()
                    ));
                    ui.spinner();
                    ui.small(
                        "The editor remains responsive; completion is revision/settings/path-bound and may already be stale if the scene changes meanwhile.",
                    );
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
                if runtime.prepared_mesh_path.is_some()
                    && runtime.prepared_mesh_path != Some(runtime.selected_mesh_path)
                {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        "Prepared case is stale: mesh path selection changed after preparation.",
                    );
                }

                if let Some(summary) = &runtime.summary {
                    ui.separator();
                    if let Some(prepared_case) = &runtime.prepared_case {
                        ui.monospace(format!("Mesh path: {}", prepared_case.mesh_kind_label()));
                        if runtime.prepared_mesh_path == Some(AccurateMeshPath::Staircase) {
                            ui.monospace(format!("Solid cells: {}", summary.solid_cells));
                        }
                    }
                    ui.monospace(format!("Active body markers: {}", summary.active_body_markers));
                    ui.monospace(format!("Points: {}", summary.points));
                    ui.monospace(format!("Tetrahedra: {}", summary.tetrahedra));
                    ui.monospace(format!("Boundary triangles: {}", summary.boundary_triangles));
                    ui.monospace(format!("SU2 markers: {}", summary.marker_count));
                    ui.monospace(format!(
                        "Mesh text: {:.2} MiB",
                        summary.mesh_bytes as f64 / 1_048_576.0
                    ));
                    ui.monospace(format!("Config text: {} bytes", summary.config_bytes));
                    ui.small(
                        "Prepared in memory. Persisting and launching SU2 remains a separate explicit action in Run / Results.",
                    );
                }
                if let Some(error) = &runtime.last_error {
                    ui.colored_label(egui::Color32::RED, format!("Preparation failed: {error}"));
                    if runtime.prepared_case.is_some() {
                        ui.small(
                            "The previous prepared artifact is retained for inspection, but execution remains disabled until a preparation succeeds again.",
                        );
                    }
                }
            });
    });

    Ok(())
}

fn collect_prepare_completion(runtime: &mut AccurateRuntime) {
    let completed = {
        let mut slot = runtime
            .completion
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        slot.take()
    };
    let Some(completed) = completed else {
        return;
    };

    runtime.preparing_revision = None;
    match completed.result {
        Ok((prepared_case, summary)) => {
            runtime.prepared_case = Some(prepared_case);
            runtime.summary = Some(summary);
            runtime.prepared_revision = Some(completed.revision);
            runtime.prepared_settings = Some(completed.settings);
            runtime.prepared_mesh_path = Some(completed.mesh_path);
            runtime.last_error = None;
            runtime.status = AccuratePrepareStatus::Prepared;
        }
        Err(error) => {
            runtime.last_error = Some(error);
            runtime.status = AccuratePrepareStatus::Failed;
        }
    }
}

fn launch_external_prepare(
    runtime: &mut AccurateRuntime,
    state: &ProjectState,
    settings: AccurateSettings,
    mesh_path: AccurateMeshPath,
) {
    debug_assert!(matches!(
        mesh_path,
        AccurateMeshPath::ValidatedTetgen | AccurateMeshPath::BoundaryLayerTetgen
    ));
    let snapshot = snapshot_project_state(state);
    let revision = state.revision;
    let completion_slot = Arc::clone(&runtime.completion);

    runtime.status = AccuratePrepareStatus::Preparing;
    runtime.preparing_revision = Some(revision);
    runtime.last_error = None;

    thread::spawn(move || {
        let result = match mesh_path {
            AccurateMeshPath::ValidatedTetgen => prepare_tetgen_from_state(&snapshot, &settings),
            AccurateMeshPath::BoundaryLayerTetgen => {
                prepare_boundary_layer_tetgen_from_state(&snapshot, &settings)
            }
            AccurateMeshPath::Staircase => Err(
                "staircase preparation cannot be launched through the external-mesh worker"
                    .to_owned(),
            ),
        };
        let completed = AccuratePrepareCompletion {
            revision,
            settings,
            mesh_path,
            result,
        };
        let mut slot = completion_slot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *slot = Some(completed);
    });
}

fn prepare_staircase_from_state(
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
    let (case, coefficient_reference) =
        solver_case_for_scene_ids(state, settings, &active_scene_ids);

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

pub(crate) fn solver_case_for_scene_ids(
    state: &ProjectState,
    settings: &AccurateSettings,
    active_scene_ids: &[u64],
) -> (Su2Case, Su2CoefficientReference) {
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
    (case, coefficient_reference)
}

fn closed_wind_tunnel_bindings() -> Vec<Su2MarkerBinding> {
    let binding = |marker, tag: &str, role, axis, side| Su2MarkerBinding {
        marker: BoundaryMarkerId(marker),
        tag: tag.into(),
        role,
        source: BoundarySource::DomainFace { axis, side },
    };
    vec![
        binding(1, "inlet", BoundaryRole::Inlet, DomainAxis::X, DomainSide::Min),
        binding(2, "outlet", BoundaryRole::Outlet, DomainAxis::X, DomainSide::Max),
        binding(3, "y_min", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Min),
        binding(4, "y_max", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Max),
        binding(5, "z_min", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Min),
        binding(6, "z_max", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Max),
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
        let (bundle, summary) =
            prepare_staircase_from_state(&state, &AccurateSettings::default()).unwrap();
        assert!(summary.solid_cells > 0);
        assert_eq!(summary.active_body_markers, 1);
        assert!(bundle.config_text.contains("MARKER_INLET= ( inlet"));
        assert!(bundle.config_text.contains("MARKER_OUTLET= ( outlet, 0.0 )"));
        assert!(bundle.config_text.contains("MARKER_MONITORING= ( body_1 )"));
        assert!(bundle.config_text.contains("REF_AREA= 1.000000000000e0"));
        assert!(bundle.config_text.contains("REF_LENGTH= 1.000000000000e0"));
        assert!(bundle.mesh_text.contains("MARKER_TAG= body_1"));
        assert!(bundle.marker_bindings.iter().any(|binding| {
            binding.tag == "body_1"
                && binding.source == BoundarySource::SceneObject { scene_object_id: 1 }
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
        let (bundle, summary) =
            prepare_staircase_from_state(&state, &AccurateSettings::default()).unwrap();
        assert!(summary.solid_cells > 0);
        assert_eq!(summary.active_body_markers, 1);
        assert!(bundle.config_text.contains(&format!("MARKER_MONITORING= ( body_{imported_id} )")));
        assert!(bundle.mesh_text.contains(&format!("MARKER_TAG= body_{imported_id}")));
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
        let error = prepare_staircase_from_state(&state, &AccurateSettings::default()).unwrap_err();
        assert!(error.contains("failed accurate audit"));
    }

    #[test]
    fn preparation_budget_fails_without_silent_grid_reduction() {
        let mut state = ProjectState::default();
        state.simulation.grid = [100, 100, 100];
        let error = prepare_staircase_from_state(&state, &AccurateSettings::default()).unwrap_err();
        assert!(error.contains("preparation limit"));
        assert_eq!(state.simulation.grid, [100, 100, 100]);
    }

    #[test]
    fn solver_case_contract_is_shared_by_scene_id_list() {
        let state = ProjectState::default();
        let settings = AccurateSettings::default();
        let (case, reference) = solver_case_for_scene_ids(&state, &settings, &[3, 9]);
        assert_eq!(case.wall_markers, vec!["y_min", "y_max", "z_min", "z_max", "body_3", "body_9"]);
        assert_eq!(case.inlets[0].marker, "inlet");
        assert_eq!(case.outlet_marker, "outlet");
        assert_eq!(reference.area_m2, settings.reference_area_m2);
        assert_eq!(reference.length_m, settings.reference_length_m);
    }

    #[test]
    fn mesh_path_change_invalidates_prepared_case_freshness() {
        let mut state = ProjectState::default();
        state.simulation.grid = [8, 6, 8];
        let settings = AccurateSettings::default();
        let (bundle, summary) = prepare_staircase_from_state(&state, &settings).unwrap();
        let mut runtime = AccurateRuntime::default();
        runtime.settings = settings.clone();
        runtime.status = AccuratePrepareStatus::Prepared;
        runtime.prepared_revision = Some(state.revision);
        runtime.prepared_settings = Some(settings);
        runtime.prepared_mesh_path = Some(AccurateMeshPath::Staircase);
        runtime.summary = Some(summary);
        runtime.prepared_case = Some(AccuratePreparedCase::staircase(bundle));
        assert!(runtime.is_fresh_for(state.revision));
        runtime.selected_mesh_path = AccurateMeshPath::BoundaryLayerTetgen;
        assert!(!runtime.is_fresh_for(state.revision));
    }
}
