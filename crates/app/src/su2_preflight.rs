use std::path::PathBuf;

use aeroforge_accurate_backend::{discover_su2, probe_su2_banner};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

const SUPPORTED_SU2_BANNER_FRAGMENT: &str = "SU2 v8.5.0";

#[derive(Clone, Debug, PartialEq, Eq)]
enum Su2PreflightState {
    Unchecked,
    Missing,
    ProbeFailed {
        executable: PathBuf,
        error: String,
    },
    MissingBanner {
        executable: PathBuf,
    },
    Unsupported {
        executable: PathBuf,
        banner: String,
    },
    Ready {
        executable: PathBuf,
        banner: String,
    },
}

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct Su2Preflight {
    state: Su2PreflightState,
}

impl Default for Su2Preflight {
    fn default() -> Self {
        Self {
            state: Su2PreflightState::Unchecked,
        }
    }
}

impl Su2Preflight {
    fn refresh(&mut self) {
        self.state = match discover_su2() {
            Some(executable) => classify_su2_probe(executable, probe_su2_banner),
            None => Su2PreflightState::Missing,
        };
    }

    fn is_ready(&self) -> bool {
        matches!(self.state, Su2PreflightState::Ready { .. })
    }
}

fn classify_su2_probe<F>(executable: PathBuf, probe: F) -> Su2PreflightState
where
    F: FnOnce(&std::path::Path) -> std::io::Result<Option<String>>,
{
    match probe(&executable) {
        Ok(Some(banner)) if banner.contains(SUPPORTED_SU2_BANNER_FRAGMENT) => {
            Su2PreflightState::Ready { executable, banner }
        }
        Ok(Some(banner)) => Su2PreflightState::Unsupported { executable, banner },
        Ok(None) => Su2PreflightState::MissingBanner { executable },
        Err(error) => Su2PreflightState::ProbeFailed {
            executable,
            error: error.to_string(),
        },
    }
}

pub fn initialize_su2_preflight(mut preflight: ResMut<Su2Preflight>) {
    preflight.refresh();
}

pub fn su2_execution_ready(preflight: Res<Su2Preflight>) -> bool {
    preflight.is_ready()
}

pub fn draw_su2_preflight_gate(
    mut contexts: EguiContexts,
    mut preflight: ResMut<Su2Preflight>,
) -> Result {
    let state = preflight.state.clone();
    let ctx = contexts.ctx_mut()?;

    if let Su2PreflightState::Ready { executable, banner } = state {
        egui::TopBottomPanel::top("accurate_su2_preflight_ready").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(egui::Color32::LIGHT_GREEN, "SU2 8.5.0 ready");
                ui.separator();
                ui.monospace(banner);
                ui.separator();
                ui.monospace(executable.display().to_string())
                    .on_hover_text("Resolved SU2_CFD executable");
                if ui.button("Re-check SU2").clicked() {
                    preflight.refresh();
                }
            });
        });
        return Ok(());
    }

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("SU2 preflight");
        ui.separator();
        ui.label(
            "Accurate execution is fail-closed until a supported external SU2 8.5.0 runtime is discovered and its banner is verified.",
        );
        ui.add_space(8.0);

        match state {
            Su2PreflightState::Unchecked => {
                ui.colored_label(egui::Color32::YELLOW, "SU2 has not been checked yet.");
            }
            Su2PreflightState::Missing => {
                ui.colored_label(egui::Color32::RED, "SU2_CFD was not found.");
                ui.label("Install SU2 8.5.0, then configure SU2_RUN or place SU2_CFD on PATH.");
            }
            Su2PreflightState::ProbeFailed { executable, error } => {
                ui.colored_label(egui::Color32::RED, "SU2_CFD was found but could not be probed.");
                ui.monospace(executable.display().to_string());
                ui.monospace(error);
            }
            Su2PreflightState::MissingBanner { executable } => {
                ui.colored_label(
                    egui::Color32::RED,
                    "SU2_CFD did not report a recognizable SU2 version banner.",
                );
                ui.monospace(executable.display().to_string());
            }
            Su2PreflightState::Unsupported { executable, banner } => {
                ui.colored_label(
                    egui::Color32::RED,
                    format!("Unsupported SU2 runtime. Expected {SUPPORTED_SU2_BANNER_FRAGMENT}."),
                );
                ui.monospace(banner);
                ui.monospace(executable.display().to_string());
            }
            Su2PreflightState::Ready { .. } => unreachable!(),
        }

        ui.add_space(8.0);
        if ui.button("Re-check SU2").clicked() {
            preflight.refresh();
        }
        ui.small(
            "This preflight is an operational compatibility check only. It does not establish solver installation correctness for every case or engineering CFD accuracy.",
        );
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn supported_banner_is_ready() {
        let path = PathBuf::from("SU2_CFD");
        let state = classify_su2_probe(path.clone(), |_| {
            Ok(Some("SU2 v8.5.0 Blackbird".to_owned()))
        });
        assert_eq!(
            state,
            Su2PreflightState::Ready {
                executable: path,
                banner: "SU2 v8.5.0 Blackbird".to_owned(),
            }
        );
    }

    #[test]
    fn unsupported_banner_is_fail_closed_and_preserved() {
        let path = PathBuf::from("SU2_CFD");
        let state = classify_su2_probe(path.clone(), |_| {
            Ok(Some("SU2 v9.0.0 future".to_owned()))
        });
        assert_eq!(
            state,
            Su2PreflightState::Unsupported {
                executable: path,
                banner: "SU2 v9.0.0 future".to_owned(),
            }
        );
    }

    #[test]
    fn missing_banner_is_fail_closed() {
        let path = PathBuf::from("SU2_CFD");
        assert_eq!(
            classify_su2_probe(path.clone(), |_| Ok(None)),
            Su2PreflightState::MissingBanner { executable: path }
        );
    }

    #[test]
    fn probe_error_is_fail_closed_and_visible() {
        let path = PathBuf::from("SU2_CFD");
        let state = classify_su2_probe(path.clone(), |_| {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"))
        });
        assert_eq!(
            state,
            Su2PreflightState::ProbeFailed {
                executable: path,
                error: "denied".to_owned(),
            }
        );
    }
}
