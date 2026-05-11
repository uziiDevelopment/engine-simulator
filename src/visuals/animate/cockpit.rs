//! Clutch + throttle pedal hinge rotation and shift lever tilt.

use bevy::prelude::*;

use crate::engine::EngineCore;
use crate::visuals::{PedalControl, PedalKind, ShiftLever};

/// Map `clutch_engagement` (1 = engaged, 0 = fully depressed) onto a hinge angle.
pub fn animate_clutch_pedal(
    core: Res<EngineCore>,
    mut q: Query<(&PedalControl, &mut Transform)>,
) {
    for (p, mut t) in &mut q {
        if !matches!(p.kind, PedalKind::Clutch) { continue; }
        let depress = 1.0 - core.clutch_engagement.clamp(0.0, 1.0);
        let angle = -depress * p.max_angle;
        t.rotation = Quat::from_rotation_x(angle);
    }
}

/// Throttle pedal: rest at 0% throttle, pressed at 100%.
pub fn animate_throttle_pedal(
    core: Res<EngineCore>,
    mut q: Query<(&PedalControl, &mut Transform)>,
) {
    for (p, mut t) in &mut q {
        if !matches!(p.kind, PedalKind::Throttle) { continue; }
        let depress = core.throttle.clamp(0.0, 1.0);
        let angle = -depress * p.max_angle;
        t.rotation = Quat::from_rotation_x(angle);
    }
}

/// Tilt the shift lever toward `lever_pos` (x: side-to-side, y: forward-back).
pub fn animate_shift_lever(
    core: Res<EngineCore>,
    mut q: Query<&mut Transform, With<ShiftLever>>,
) {
    let max_tilt = 0.35_f32;
    let lp = core.gearbox.lever_pos;
    let tilt_z = -lp.x.clamp(-1.0, 1.0) * max_tilt;
    let tilt_x = lp.y.clamp(-1.0, 1.0) * max_tilt;
    let rot = Quat::from_rotation_z(tilt_z) * Quat::from_rotation_x(tilt_x);
    for mut t in &mut q {
        t.rotation = rot;
    }
}
