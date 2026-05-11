//! Crankshaft rotation + clutch plate position (drivetrain readout).

use bevy::prelude::*;

use crate::engine::{EngineCore, VIS_SCALE};
use crate::visuals::{Clutch, Crankshaft};

pub fn animate_crank(core: Res<EngineCore>, mut q: Query<&mut Transform, With<Crankshaft>>) {
    for mut t in &mut q {
        t.rotation = Quat::from_rotation_x(core.angle);
    }
}

pub fn animate_drivetrain(core: Res<EngineCore>, mut q: Query<&mut Transform, With<Clutch>>) {
    let base_rot = Quat::from_rotation_y(-std::f32::consts::PI / 2.0);

    let positions = core.config.crank_positions();
    let spacing = core.config.cylinder_spacing * VIS_SCALE;
    let center_x = (positions as f32 - 1.0) * 0.5 * spacing;
    let rear_x = center_x + spacing * 0.5;

    // Clutch throw — moves 50 mm away when fully disengaged.
    let throw = (1.0 - core.clutch_engagement) * 0.050;

    for mut t in &mut q {
        let relative_angle = core.drivetrain_angle - core.angle;
        t.rotation = Quat::from_rotation_x(relative_angle) * base_rot;
        t.translation = Vec3::new(rear_x + 0.06 * VIS_SCALE + throw, 0.0, 0.0);
    }
}
