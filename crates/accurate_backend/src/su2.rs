use std::env;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowModel {
    Laminar,
    RansSst,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InletBoundary {
    pub marker: String,
    pub temperature_k: f64,
    pub speed_mps: f64,
    pub direction: [f64; 3],
    /// Fraction, e.g. 0.05 = 5%. Used by RANS only.
    pub turbulence_intensity: Option<f64>,
    pub turbulent_to_laminar_viscosity_ratio: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Su2CoefficientReference {
    /// Reference area used by SU2 to non-dimensionalize integrated force coefficients.
    pub area_m2: f64,
    /// Reference length used by SU2 to non-dimensionalize integrated moment coefficients.
    pub length_m: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Su2Case {
    pub mesh_filename: String,
    pub density_kg_m3: f64,
    pub kinematic_viscosity_m2_s: f64,
    pub flow_model: FlowModel,
    pub inlets: Vec<InletBoundary>,
    pub outlet_marker: String,
    /// All physical no-slip/heat-flux wall markers in the generated case.
    pub wall_markers: Vec<String>,
    pub max_iterations: u32,
    /// SU2 residual target is expressed as log10 residual, commonly around -6.
    pub convergence_log10: f64,
    pub output_basename: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Su2CaseError {
    InvalidMeshFilename,
    InvalidOutputBasename,
    InvalidMarker(String),
    MonitoringMarkerNotWall(String),
    MissingInlet,
    NonPositiveDensity,
    NonPositiveViscosity,
    NonPositiveSpeed(String),
    InvalidTemperature(String),
    InvalidDirection(String),
    InvalidTurbulence(String),
    InvalidReferenceArea,
    InvalidReferenceLength,
    ZeroIterations,
}

impl fmt::Display for Su2CaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMeshFilename => {
                write!(f, "mesh filename must be a safe non-empty relative filename")
            }
            Self::InvalidOutputBasename => {
                write!(f, "output basename contains unsupported characters")
            }
            Self::InvalidMarker(marker) => write!(f, "invalid SU2 marker: {marker}"),
            Self::MonitoringMarkerNotWall(marker) => write!(
                f,
                "monitoring marker `{marker}` must also be declared as a wall marker"
            ),
            Self::MissingInlet => write!(f, "at least one inlet is required"),
            Self::NonPositiveDensity => write!(f, "density must be positive"),
            Self::NonPositiveViscosity => write!(f, "kinematic viscosity must be positive"),
            Self::NonPositiveSpeed(marker) => {
                write!(f, "inlet {marker} must have positive speed")
            }
            Self::InvalidTemperature(marker) => write!(
                f,
                "inlet {marker} must have a positive finite temperature"
            ),
            Self::InvalidDirection(marker) => {
                write!(f, "inlet {marker} has a zero/invalid direction")
            }
            Self::InvalidTurbulence(marker) => {
                write!(f, "inlet {marker} has invalid turbulence settings")
            }
            Self::InvalidReferenceArea => {
                write!(f, "coefficient reference area must be positive and finite")
            }
            Self::InvalidReferenceLength => {
                write!(f, "coefficient reference length must be positive and finite")
            }
            Self::ZeroIterations => write!(f, "max_iterations must be > 0"),
        }
    }
}

impl std::error::Error for Su2CaseError {}

impl Su2CoefficientReference {
    pub fn validate(&self) -> Result<(), Su2CaseError> {
        if self.area_m2 <= 0.0 || !self.area_m2.is_finite() {
            return Err(Su2CaseError::InvalidReferenceArea);
        }
        if self.length_m <= 0.0 || !self.length_m.is_finite() {
            return Err(Su2CaseError::InvalidReferenceLength);
        }
        Ok(())
    }
}

impl Su2Case {
    pub fn validate(&self) -> Result<(), Su2CaseError> {
        if !safe_filename(&self.mesh_filename)
            || !self.mesh_filename.to_ascii_lowercase().ends_with(".su2")
        {
            return Err(Su2CaseError::InvalidMeshFilename);
        }
        if !safe_token(&self.output_basename) {
            return Err(Su2CaseError::InvalidOutputBasename);
        }
        if self.density_kg_m3 <= 0.0 || !self.density_kg_m3.is_finite() {
            return Err(Su2CaseError::NonPositiveDensity);
        }
        if self.kinematic_viscosity_m2_s <= 0.0
            || !self.kinematic_viscosity_m2_s.is_finite()
        {
            return Err(Su2CaseError::NonPositiveViscosity);
        }
        if self.max_iterations == 0 {
            return Err(Su2CaseError::ZeroIterations);
        }
        if self.inlets.is_empty() {
            return Err(Su2CaseError::MissingInlet);
        }
        validate_marker(&self.outlet_marker)?;
        for marker in &self.wall_markers {
            validate_marker(marker)?;
        }
        for inlet in &self.inlets {
            validate_marker(&inlet.marker)?;
            if inlet.speed_mps <= 0.0 || !inlet.speed_mps.is_finite() {
                return Err(Su2CaseError::NonPositiveSpeed(inlet.marker.clone()));
            }
            if inlet.temperature_k <= 0.0 || !inlet.temperature_k.is_finite() {
                return Err(Su2CaseError::InvalidTemperature(inlet.marker.clone()));
            }
            let norm_sq = inlet.direction.iter().map(|v| v * v).sum::<f64>();
            if !norm_sq.is_finite() || norm_sq <= 1.0e-16 {
                return Err(Su2CaseError::InvalidDirection(inlet.marker.clone()));
            }
            if let Some(intensity) = inlet.turbulence_intensity {
                if !(0.0..1.0).contains(&intensity)
                    || inlet.turbulent_to_laminar_viscosity_ratio <= 0.0
                    || !inlet.turbulent_to_laminar_viscosity_ratio.is_finite()
                {
                    return Err(Su2CaseError::InvalidTurbulence(inlet.marker.clone()));
                }
            }
        }
        Ok(())
    }

    pub fn render_config(&self) -> Result<String, Su2CaseError> {
        self.render_config_with_monitoring(&[])
    }

    /// Renders the same physical case while explicitly selecting the wall markers whose integrated
    /// loads SU2 should monitor. Monitoring markers are required to be a subset of `wall_markers`
    /// so post-processing cannot silently reference an undeclared or non-wall boundary.
    pub fn render_config_with_monitoring(
        &self,
        monitoring_markers: &[String],
    ) -> Result<String, Su2CaseError> {
        self.render_config_with_monitoring_and_reference(monitoring_markers, None)
    }

    /// Extends the monitoring contract with explicit SI reference dimensions and a fixed Cartesian
    /// coefficient frame for the current generated +X-flow path. When a reference is supplied,
    /// AeroForge pins SU2 to AOA=0 deg, sideslip=0 deg and moment origin [0, 0, 0] m.
    ///
    /// SU2 8.5.0 projects 3D coefficients at that frame as CD=CFx, CL=CFz and CSF=CFy. AeroForge's
    /// scene convention is Y-up, so callers must not relabel raw SU2 CL as vertical lift. The fixed
    /// frame makes the raw history semantics reproducible without silently remapping world axes.
    pub fn render_config_with_monitoring_and_reference(
        &self,
        monitoring_markers: &[String],
        coefficient_reference: Option<&Su2CoefficientReference>,
    ) -> Result<String, Su2CaseError> {
        self.validate()?;
        for marker in monitoring_markers {
            validate_marker(marker)?;
            if !self.wall_markers.iter().any(|wall| wall == marker) {
                return Err(Su2CaseError::MonitoringMarkerNotWall(marker.clone()));
            }
        }
        if let Some(reference) = coefficient_reference {
            reference.validate()?;
        }

        let solver = match self.flow_model {
            FlowModel::Laminar => "INC_NAVIER_STOKES",
            FlowModel::RansSst => "INC_RANS",
        };
        let dynamic_viscosity = self.density_kg_m3 * self.kinematic_viscosity_m2_s;
        let mut cfg = String::new();

        push_kv(&mut cfg, "SOLVER", solver);
        push_kv(&mut cfg, "SYSTEM_MEASUREMENTS", "SI");
        push_kv(&mut cfg, "INC_NONDIM", "DIMENSIONAL");
        push_kv(&mut cfg, "INC_DENSITY_MODEL", "CONSTANT");
        push_kv(&mut cfg, "FLUID_MODEL", "CONSTANT_DENSITY");
        push_kv(
            &mut cfg,
            "INC_DENSITY_INIT",
            &fmt_float(self.density_kg_m3),
        );
        push_kv(&mut cfg, "VISCOSITY_MODEL", "CONSTANT_VISCOSITY");
        push_kv(&mut cfg, "MU_CONSTANT", &fmt_float(dynamic_viscosity));
        if let Some(reference) = coefficient_reference {
            push_kv(&mut cfg, "REF_AREA", &fmt_float(reference.area_m2));
            push_kv(&mut cfg, "REF_LENGTH", &fmt_float(reference.length_m));
            push_kv(&mut cfg, "AOA", &fmt_float(0.0));
            push_kv(&mut cfg, "SIDESLIP_ANGLE", &fmt_float(0.0));
            push_kv(&mut cfg, "REF_ORIGIN_MOMENT_X", &fmt_float(0.0));
            push_kv(&mut cfg, "REF_ORIGIN_MOMENT_Y", &fmt_float(0.0));
            push_kv(&mut cfg, "REF_ORIGIN_MOMENT_Z", &fmt_float(0.0));
        }
        if self.flow_model == FlowModel::RansSst {
            push_kv(&mut cfg, "KIND_TURB_MODEL", "SST");
            push_kv(&mut cfg, "SST_OPTIONS", "V2003m");
        }

        push_kv(&mut cfg, "MESH_FILENAME", &self.mesh_filename);
        push_kv(&mut cfg, "MESH_FORMAT", "SU2");
        push_kv(&mut cfg, "INC_INLET_TYPE", "VELOCITY_INLET");
        push_kv(&mut cfg, "INC_OUTLET_TYPE", "PRESSURE_OUTLET");

        let inlet_values = self
            .inlets
            .iter()
            .map(|inlet| {
                let d = normalized(inlet.direction);
                format!(
                    "{}, {}, {}, {}, {}, {}",
                    inlet.marker,
                    fmt_float(inlet.temperature_k),
                    fmt_float(inlet.speed_mps),
                    fmt_float(d[0]),
                    fmt_float(d[1]),
                    fmt_float(d[2])
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        push_kv(
            &mut cfg,
            "MARKER_INLET",
            &format!("( {inlet_values} )"),
        );
        push_kv(
            &mut cfg,
            "MARKER_OUTLET",
            &format!("( {}, 0.0 )", self.outlet_marker),
        );

        if !self.wall_markers.is_empty() {
            let heatflux = self
                .wall_markers
                .iter()
                .map(|marker| format!("{marker}, 0.0"))
                .collect::<Vec<_>>()
                .join(", ");
            push_kv(
                &mut cfg,
                "MARKER_HEATFLUX",
                &format!("( {heatflux} )"),
            );
            push_kv(
                &mut cfg,
                "MARKER_PLOTTING",
                &format!("( {} )", self.wall_markers.join(", ")),
            );
        }
        if !monitoring_markers.is_empty() {
            push_kv(
                &mut cfg,
                "MARKER_MONITORING",
                &format!("( {} )", monitoring_markers.join(", ")),
            );
        }

        if self.flow_model == FlowModel::RansSst {
            let turbulent = self
                .inlets
                .iter()
                .map(|inlet| {
                    format!(
                        "{}, {}, {}",
                        inlet.marker,
                        fmt_float(inlet.turbulence_intensity.unwrap_or(0.05)),
                        fmt_float(inlet.turbulent_to_laminar_viscosity_ratio.max(1.0))
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            push_kv(
                &mut cfg,
                "MARKER_INLET_TURBULENT",
                &format!("( {turbulent} )"),
            );
        }

        // Keep the persisted history contract explicit. SU2 only writes groups requested through
        // HISTORY_OUTPUT; request AERO_COEFF when a monitored body also has an explicit coefficient
        // reference so the aggregate CFx/CFy/CFz and CMx/CMy/CMz diagnostics are actually present.
        let history_output = if coefficient_reference.is_some() && !monitoring_markers.is_empty() {
            "ITER, RMS_RES, AERO_COEFF"
        } else {
            "ITER, RMS_RES"
        };
        push_kv(&mut cfg, "HISTORY_OUTPUT", history_output);

        // SU2 8.5.0 does not provide a usable default for the flow convective scheme.
        // Keep these numerical choices explicit so generated configs do not depend on version
        // defaults and can be exercised by the pinned external-runtime evidence test.
        push_kv(&mut cfg, "CONV_NUM_METHOD_FLOW", "FDS");
        push_kv(&mut cfg, "MUSCL_FLOW", "YES");
        push_kv(&mut cfg, "SLOPE_LIMITER_FLOW", "NONE");
        push_kv(&mut cfg, "TIME_DISCRE_FLOW", "EULER_IMPLICIT");

        push_kv(&mut cfg, "ITER", &self.max_iterations.to_string());
        push_kv(
            &mut cfg,
            "CONV_RESIDUAL_MINVAL",
            &fmt_float(self.convergence_log10),
        );
        push_kv(
            &mut cfg,
            "RESTART_FILENAME",
            &format!("{}_restart", self.output_basename),
        );
        push_kv(
            &mut cfg,
            "VOLUME_FILENAME",
            &format!("{}_volume", self.output_basename),
        );
        push_kv(
            &mut cfg,
            "SURFACE_FILENAME",
            &format!("{}_surface", self.output_basename),
        );
        push_kv(
            &mut cfg,
            "OUTPUT_FILES",
            "( RESTART_ASCII, PARAVIEW, SURFACE_CSV )",
        );
        Ok(cfg)
    }
}

#[derive(Debug)]
pub struct Su2RunResult {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub fn discover_su2() -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) {
        &["SU2_CFD.exe", "SU2_CFD"]
    } else {
        &["SU2_CFD", "SU2_CFD.exe"]
    };

    if let Some(root) = env::var_os("SU2_RUN") {
        for name in names {
            let candidate = PathBuf::from(&root).join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    let path = env::var_os("PATH")?;
    for dir in env::split_paths(&path) {
        for name in names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

pub fn probe_su2_banner(executable: &Path) -> std::io::Result<Option<String>> {
    let output = Command::new(executable).arg("--help").output()?;
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(combined
        .lines()
        .map(str::trim)
        .find(|line| line.contains("SU2 v"))
        .map(ToOwned::to_owned))
}

pub fn run_su2_case(
    executable: &Path,
    working_directory: &Path,
    config_filename: &str,
) -> std::io::Result<Su2RunResult> {
    if !safe_filename(config_filename) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "config filename must be a safe relative filename",
        ));
    }
    let output = Command::new(executable)
        .current_dir(working_directory)
        .arg(config_filename)
        .output()?;
    Ok(Su2RunResult {
        success: output.status.success(),
        exit_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn validate_marker(marker: &str) -> Result<(), Su2CaseError> {
    if safe_token(marker) {
        Ok(())
    } else {
        Err(Su2CaseError::InvalidMarker(marker.to_owned()))
    }
}

fn safe_filename(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('/')
        && !value.contains('\\')
        && value != "."
        && value != ".."
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

fn safe_token(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

fn normalized(v: [f64; 3]) -> [f64; 3] {
    let norm = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / norm, v[1] / norm, v[2] / norm]
}

fn push_kv(out: &mut String, key: &str, value: &str) {
    out.push_str(key);
    out.push_str("= ");
    out.push_str(value);
    out.push('\n');
}

fn fmt_float(value: f64) -> String {
    format!("{value:.12e}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_case() -> Su2Case {
        Su2Case {
            mesh_filename: "case.su2".into(),
            density_kg_m3: 1.225,
            kinematic_viscosity_m2_s: 1.48e-5,
            flow_model: FlowModel::RansSst,
            inlets: vec![InletBoundary {
                marker: "inlet_main".into(),
                temperature_k: 288.15,
                speed_mps: 12.0,
                direction: [2.0, 0.0, 0.0],
                turbulence_intensity: Some(0.02),
                turbulent_to_laminar_viscosity_ratio: 10.0,
            }],
            outlet_marker: "outlet".into(),
            wall_markers: vec!["tunnel_wall".into(), "body".into()],
            max_iterations: 1_000,
            convergence_log10: -6.0,
            output_basename: "aeroforge".into(),
        }
    }

    #[test]
    fn rans_config_contains_dimensional_air_and_sst() {
        let monitoring = vec!["body".to_owned()];
        let cfg = sample_case()
            .render_config_with_monitoring(&monitoring)
            .unwrap();
        assert!(cfg.contains("SOLVER= INC_RANS"));
        assert!(cfg.contains("SYSTEM_MEASUREMENTS= SI"));
        assert!(cfg.contains("INC_NONDIM= DIMENSIONAL"));
        assert!(cfg.contains("KIND_TURB_MODEL= SST"));
        assert!(cfg.contains("SST_OPTIONS= V2003m"));
        assert!(cfg.contains("MARKER_INLET= ( inlet_main"));
        assert!(cfg.contains("MARKER_INLET_TURBULENT= ( inlet_main"));
        assert!(cfg.contains("MARKER_OUTLET= ( outlet, 0.0 )"));
        assert!(cfg.contains("MARKER_HEATFLUX= ( tunnel_wall, 0.0, body, 0.0 )"));
        assert!(cfg.contains("MARKER_MONITORING= ( body )"));
        assert!(cfg.contains("MARKER_PLOTTING= ( tunnel_wall, body )"));
        assert!(cfg.contains("HISTORY_OUTPUT= ITER, RMS_RES"));
        assert!(!cfg.contains("AERO_COEFF"));
        assert!(cfg.contains("CONV_NUM_METHOD_FLOW= FDS"));
        assert!(cfg.contains("MUSCL_FLOW= YES"));
        assert!(cfg.contains("SLOPE_LIMITER_FLOW= NONE"));
        assert!(cfg.contains("TIME_DISCRE_FLOW= EULER_IMPLICIT"));
    }

    #[test]
    fn monitoring_is_separate_from_physical_wall_boundary_set() {
        let monitoring = vec!["body".to_owned()];
        let cfg = sample_case()
            .render_config_with_monitoring(&monitoring)
            .unwrap();
        let line = cfg
            .lines()
            .find(|line| line.starts_with("MARKER_MONITORING="))
            .unwrap();
        assert_eq!(line, "MARKER_MONITORING= ( body )");
        assert!(!line.contains("tunnel_wall"));
    }

    #[test]
    fn default_render_omits_marker_monitoring_and_coefficient_reference() {
        let cfg = sample_case().render_config().unwrap();
        assert!(!cfg
            .lines()
            .any(|line| line.starts_with("MARKER_MONITORING=")));
        assert!(!cfg.lines().any(|line| line.starts_with("REF_AREA=")));
        assert!(!cfg.lines().any(|line| line.starts_with("REF_LENGTH=")));
        assert!(!cfg.lines().any(|line| line.starts_with("AOA=")));
        assert!(!cfg
            .lines()
            .any(|line| line.starts_with("SIDESLIP_ANGLE=")));
        assert!(!cfg
            .lines()
            .any(|line| line.starts_with("REF_ORIGIN_MOMENT_")));
        assert!(cfg.contains("HISTORY_OUTPUT= ITER, RMS_RES"));
        assert!(!cfg.contains("AERO_COEFF"));
        assert!(cfg.contains("MARKER_HEATFLUX= ( tunnel_wall, 0.0, body, 0.0 )"));
    }

    #[test]
    fn explicit_coefficient_reference_is_rendered_in_fixed_zero_angle_frame() {
        let monitoring = vec!["body".to_owned()];
        let reference = Su2CoefficientReference {
            area_m2: 2.5,
            length_m: 1.25,
        };
        let cfg = sample_case()
            .render_config_with_monitoring_and_reference(&monitoring, Some(&reference))
            .unwrap();
        assert!(cfg.contains("SYSTEM_MEASUREMENTS= SI"));
        assert!(cfg.contains("REF_AREA= 2.500000000000e0"));
        assert!(cfg.contains("REF_LENGTH= 1.250000000000e0"));
        assert!(cfg.contains("AOA= 0.000000000000e0"));
        assert!(cfg.contains("SIDESLIP_ANGLE= 0.000000000000e0"));
        assert!(cfg.contains("REF_ORIGIN_MOMENT_X= 0.000000000000e0"));
        assert!(cfg.contains("REF_ORIGIN_MOMENT_Y= 0.000000000000e0"));
        assert!(cfg.contains("REF_ORIGIN_MOMENT_Z= 0.000000000000e0"));
        assert!(cfg.contains("HISTORY_OUTPUT= ITER, RMS_RES, AERO_COEFF"));
    }

    #[test]
    fn invalid_coefficient_reference_fails_closed() {
        let monitoring = vec!["body".to_owned()];
        let mut reference = Su2CoefficientReference {
            area_m2: 0.0,
            length_m: 1.0,
        };
        assert_eq!(
            sample_case().render_config_with_monitoring_and_reference(
                &monitoring,
                Some(&reference)
            ),
            Err(Su2CaseError::InvalidReferenceArea)
        );
        reference.area_m2 = 1.0;
        reference.length_m = f64::NAN;
        assert_eq!(
            sample_case().render_config_with_monitoring_and_reference(
                &monitoring,
                Some(&reference)
            ),
            Err(Su2CaseError::InvalidReferenceLength)
        );
    }

    #[test]
    fn monitoring_marker_must_be_a_declared_wall() {
        let monitoring = vec!["ghost_body".to_owned()];
        assert_eq!(
            sample_case().render_config_with_monitoring(&monitoring),
            Err(Su2CaseError::MonitoringMarkerNotWall("ghost_body".into()))
        );
    }

    #[test]
    fn direction_is_normalized_in_config() {
        let cfg = sample_case().render_config().unwrap();
        assert!(cfg.contains(
            "1.000000000000e0, 0.000000000000e0, 0.000000000000e0"
        ));
    }

    #[test]
    fn unsafe_marker_is_rejected() {
        let mut case = sample_case();
        case.outlet_marker = "outlet, injected".into();
        assert!(matches!(
            case.validate(),
            Err(Su2CaseError::InvalidMarker(_))
        ));
    }

    #[test]
    fn invalid_temperature_is_reported_separately() {
        let mut case = sample_case();
        case.inlets[0].temperature_k = -1.0;
        assert!(matches!(
            case.validate(),
            Err(Su2CaseError::InvalidTemperature(_))
        ));
    }

    #[test]
    fn missing_inlet_is_rejected() {
        let mut case = sample_case();
        case.inlets.clear();
        assert_eq!(case.validate(), Err(Su2CaseError::MissingInlet));
    }
}
