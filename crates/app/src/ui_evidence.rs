use std::env;
use std::path::PathBuf;

use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};

const EVIDENCE_PATH_ENV: &str = "AEROFORGE_UI_EVIDENCE_PATH";
const CAPTURE_AFTER_SECONDS: f32 = 5.0;

#[derive(Default)]
pub struct UiEvidenceCaptureState {
    elapsed_seconds: f32,
    requested: bool,
}

/// Evidence-only primary-window render-target capture.
///
/// In ordinary AeroForge launches `AEROFORGE_UI_EVIDENCE_PATH` is absent and this system is a
/// no-op. The temporary GitHub evidence workflow sets one absolute path; after five seconds of
/// normal app/render updates this spawns Bevy's native primary-window screenshot request and saves
/// exactly that render target. The hook is intentionally temporary and must be removed together
/// with the evidence workflow once the screenshot artifact has been verified.
pub fn capture_primary_window_when_requested(
    mut commands: Commands,
    time: Res<Time>,
    mut state: Local<UiEvidenceCaptureState>,
) {
    if state.requested {
        return;
    }
    let Some(path) = env::var_os(EVIDENCE_PATH_ENV).map(PathBuf::from) else {
        return;
    };

    state.elapsed_seconds += time.delta_secs();
    if state.elapsed_seconds < CAPTURE_AFTER_SECONDS {
        return;
    }

    info!(
        path = %path.display(),
        elapsed_seconds = state.elapsed_seconds,
        "requesting evidence-only Bevy primary-window screenshot"
    );
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));
    state.requested = true;
}
