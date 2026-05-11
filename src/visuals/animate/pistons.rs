//! Pistons + connecting rods (per-frame slider-crank kinematics).

use bevy::prelude::*;

use crate::engine::{EngineCore, VIS_SCALE};
use crate::visuals::{ConRod, Piston, RodAttachmentPoints};

use super::tilt_vec;

pub fn animate_pistons(core: Res<EngineCore>, mut q: Query<(&Piston, &mut Transform)>) {
    for (p, mut t) in &mut q {
        if p.idx >= core.config.num_cylinders { continue; }
        let y_p = core.config.piston_y(core.angle, p.idx) * VIS_SCALE;
        let x = core.config.cyl_visual_x(p.idx);
        let tilt = p.bank_tilt;
        let pos = tilt_vec(x, y_p, 0.0, tilt);
        t.translation = pos;
        t.rotation = Quat::from_rotation_x(tilt);
    }
}

pub fn animate_rods(
    core: Res<EngineCore>,
    mut q: Query<(&ConRod, &mut Transform, Option<&RodAttachmentPoints>)>,
) {
    let r = core.config.crank_radius() * VIS_SCALE;
    for (rod, mut t, points) in &mut q {
        if rod.idx >= core.config.num_cylinders { continue; }
        let pin_phase = core.config.crank_phases[rod.idx];
        let world_theta = core.angle + pin_phase;
        // Crank pin (crank rotates in Y-Z plane)
        let pin = Vec3::new(rod.base_x, r * world_theta.cos(), r * world_theta.sin());
        // Piston wrist-pin along the tilted bank axis
        let y_p = core.config.piston_y(core.angle, rod.idx) * VIS_SCALE;
        let tilt = rod.bank_tilt;
        let small = tilt_vec(rod.base_x, y_p, 0.0, tilt);

        let dir = (small - pin).normalize_or_zero();
        let target_len = (small - pin).length();

        if let Some(pts) = points {
            // Robust alignment via the GLB's own attach_top/attach_bottom markers
            let model_vec = pts.top - pts.bottom;
            let model_dist = model_vec.length();
            if model_dist > 1e-4 {
                let scale = target_len / model_dist;
                t.scale = Vec3::splat(scale);

                let model_dir = model_vec.normalize();
                // Twist exactly around the rod's own longitudinal axis
                let twist = Quat::from_axis_angle(model_dir, std::f32::consts::PI / 2.0);
                t.rotation = Quat::from_rotation_arc(model_dir, dir) * twist;
                t.translation = pin - t.rotation.mul_vec3(pts.bottom * scale);
            }
        } else {
            // Cuboid placeholder fallback before the GLB markers are discovered
            let mid = (pin + small) * 0.5;
            t.translation = mid;
            t.rotation = Quat::from_rotation_arc(Vec3::Z, dir);
        }
    }
}
