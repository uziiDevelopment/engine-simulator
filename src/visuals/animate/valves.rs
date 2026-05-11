//! Valve lift — slides the head along the bank axis by the current cam lift.

use bevy::prelude::*;

use crate::engine::{EngineCore, VIS_SCALE};
use crate::visuals::{Valve, ValveKind};

use super::tilt_vec;

pub fn animate_valves(core: Res<EngineCore>, mut q: Query<(&Valve, &mut Transform)>) {
    for (v, mut t) in &mut q {
        if v.cyl >= core.cylinders.len() { continue; }
        let lift_m = match v.kind {
            ValveKind::Intake  => core.cylinders[v.cyl].intake_lift,
            ValveKind::Exhaust => core.cylinders[v.cyl].exhaust_lift,
        };
        // Valve heads pull into the cylinder (along the bank axis) when opening.
        let delta = lift_m * VIS_SCALE * 1.5;
        let tilt = v.bank_tilt;
        let x = t.translation.x;
        t.translation = tilt_vec(x, v.seat_y - delta, v.z_local, tilt);
    }
}
