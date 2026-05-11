//! Turbo wheel spin + GLB-node discovery + alignment of `exhaust_mount` to
//! the manifold outlet marker.

use bevy::prelude::*;

use crate::engine::EngineCore;
use crate::visuals::{
    ExhaustOutlet, TurbineWheel, TurboGlbRoot, TurboMountAligned, TurboWheelsDiscovered,
};

/// Spin the discovered turbine wheels. Real turbo shafts hit ~190k RPM so we
/// map shaft speed to a saturated visual rate that reads as "fast" without
/// strobing.
pub fn animate_turbo(
    time: Res<Time>,
    core: Res<EngineCore>,
    mut q_turb: Query<(&mut Transform, &TurbineWheel)>,
) {
    if !core.config.turbo_enabled() { return; }
    let dt = time.delta_seconds();
    for (mut transform, wheel) in &mut q_turb {
        if let Some(turbo_state) = core.turbos.get(wheel.turbo_idx) {
            let omega_phys = turbo_state.shaft_omega;
            let visual_omega = (omega_phys * 0.0015).clamp(0.0, 25.0);
            transform.rotate_local_y(visual_omega * dt);
        }
    }
}

/// Scan each [`TurboGlbRoot`]'s GLB tree for the two spin nodes and tag them.
/// Retries every frame until both are found because the scene resolves async.
pub fn discover_turbo_wheels(
    mut commands: Commands,
    q_roots: Query<(Entity, &TurboGlbRoot), Without<TurboWheelsDiscovered>>,
    q_children: Query<&Children>,
    q_named: Query<&Name>,
) {
    for (root_entity, glb_root) in &q_roots {
        let mut found_count = 0u32;
        let mut stack: Vec<Entity> = q_children
            .get(root_entity)
            .map(|c| c.iter().copied().collect())
            .unwrap_or_default();

        while let Some(child) = stack.pop() {
            if let Ok(name) = q_named.get(child) {
                let nl = name.as_str().to_lowercase();
                let is_spin_node = nl.contains("intake_compressor") || nl.contains("exhaust_turbine");
                if is_spin_node {
                    bevy::log::info!("Turbo spin node found: '{}'", name.as_str());
                    commands.entity(child).insert(TurbineWheel { turbo_idx: glb_root.turbo_idx });
                    found_count += 1;
                }
            }
            if let Ok(children) = q_children.get(child) {
                stack.extend(children.iter().copied());
            }
        }

        if found_count >= 2 {
            commands.entity(root_entity).insert(TurboWheelsDiscovered);
        }
    }
}

/// Walk each [`TurboGlbRoot`]'s tree for `exhaust_mount` and translate the
/// root so that node's world position coincides with the matching
/// [`ExhaustOutlet`] marker.
pub fn align_turbo_to_outlet(
    mut commands: Commands,
    mut q_roots: Query<
        (Entity, &TurboGlbRoot, &GlobalTransform, &mut Transform),
        (With<TurboWheelsDiscovered>, Without<TurboMountAligned>),
    >,
    q_children: Query<&Children>,
    q_named: Query<&Name>,
    q_global: Query<&GlobalTransform>,
    q_outlets: Query<&ExhaustOutlet>,
) {
    for (root_entity, glb_root, root_gt, mut root_xf) in &mut q_roots {
        let Some(outlet) = q_outlets
            .iter()
            .find(|o| o.turbo_idx == Some(glb_root.turbo_idx))
        else { continue };

        // DFS for `exhaust_mount`
        let mut mount_world: Option<Vec3> = None;
        let mut stack: Vec<Entity> = q_children
            .get(root_entity)
            .map(|c| c.iter().copied().collect())
            .unwrap_or_default();
        while let Some(child) = stack.pop() {
            if let Ok(name) = q_named.get(child) {
                if name.as_str().to_lowercase().contains("exhaust_mount") {
                    if let Ok(gt) = q_global.get(child) {
                        mount_world = Some(gt.translation());
                    }
                    break;
                }
            }
            if let Ok(children) = q_children.get(child) {
                stack.extend(children.iter().copied());
            }
        }

        let Some(mount_pos) = mount_world else { continue };

        // Skip stale GlobalTransforms (newly spawned scene before propagation).
        let root_pos = root_gt.translation();
        if (mount_pos - root_pos).length_squared() < 1e-8 { continue; }

        // If we've already converged, lock in and stop running.
        if (mount_pos - outlet.world_pos).length_squared() < 1e-6 {
            commands.entity(root_entity).insert(TurboMountAligned);
            continue;
        }

        // Otherwise nudge the root by the world-space delta and try again next frame.
        let delta = outlet.world_pos - mount_pos;
        root_xf.translation += delta;
    }
}
