//! Dynamic scene assembly: spawns engine visual entities based on current config.
//!
//! This is a conductor module: each engine part lives in its own submodule, and
//! [`spawn_engine_visuals`] just orchestrates them. The shared geometry that
//! several parts need (head height, rear of crank, etc.) is computed here once
//! and passed into each submodule via [`BuildCtx`].

mod cockpit;
mod cylinders;
mod crank;
mod gearbox;
mod heads;
mod manifolds;
mod static_scene;
mod turbo;
mod valves;

use bevy::prelude::*;

use crate::engine::{EngineCore, EngineLayout, VIS_SCALE};

use super::EngineVisual;

pub use static_scene::setup_static_scene;

/// Geometry + shared handles computed once per build. Submodules consume this
/// instead of recomputing.
pub(super) struct BuildCtx<'a> {
    pub core: &'a EngineCore,
    pub s: f32,
    pub head_y: f32,
    pub head_width: f32,
    pub journal_count: usize,
    pub rear_x: f32,
    pub front_x: f32,
    pub length_scale: f32,
}

/// Rebuilds the entire engine scene from the current config.
pub fn spawn_engine_visuals(
    commands: &mut Commands,
    core: &EngineCore,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &AssetServer,
) {
    let cfg = &core.config;
    let s = VIS_SCALE;
    let num_cyl = cfg.num_cylinders;

    // ── Compute shared geometry (used by multiple submodules) ────────────────
    let rod_length = cfg.rod_length;
    let cyl_spacing = cfg.cylinder_spacing;
    let head_y = rod_length * s + 0.18 * s;

    let journal_count = match cfg.layout {
        EngineLayout::Inline | EngineLayout::Flat => num_cyl + 1,
        EngineLayout::V => (num_cyl / 2) + 1,
        EngineLayout::W { .. } => (num_cyl / 2) + 1,
    };
    let head_width = (journal_count as f32 * cyl_spacing + 0.08) * s;

    // Modular crank dimensions: must match the values used by crank.rs so the
    // pulley / flywheel snap to the right ends.
    const MODEL_LENGTH: f32 = 10.49;
    let length_scale = (cyl_spacing * s) / MODEL_LENGTH;
    let front_x = cfg.cyl_visual_x(0) - 6.54 * length_scale;
    let rear_x = cfg.cyl_visual_x(num_cyl - 1) + 3.95 * length_scale;

    let ctx = BuildCtx {
        core,
        s,
        head_y,
        head_width,
        journal_count,
        rear_x,
        front_x,
        length_scale,
    };

    // ── Submodule conductor ─────────────────────────────────────────────────
    let crank_entity = crank::spawn(commands, meshes, materials, asset_server, &ctx);
    let groups = spawn_groups(commands);

    cylinders::spawn(commands, meshes, materials, &ctx, &groups);
    valves::spawn(commands, meshes, materials, &ctx, &groups);
    heads::spawn(commands, meshes, materials, &ctx, &groups);
    manifolds::spawn(commands, meshes, materials, &ctx, &groups);
    turbo::spawn(commands, asset_server, &ctx);
    gearbox::spawn(commands, meshes, materials, &ctx);
    cockpit::spawn(commands, meshes, materials, &ctx);

    let _ = crank_entity; // silence unused if no future child needs it
}

/// Top-level group entities that submodules parent their parts under.
pub(super) struct Groups {
    pub pistons: Entity,
    pub rings: Entity,
    pub rods: Entity,
    pub bores: Entity,
    pub block: Entity,
    pub valves: Entity,
    pub heads: Entity,
    pub manifolds: Entity,
}

fn spawn_groups(commands: &mut Commands) -> Groups {
    Groups {
        pistons: commands.spawn((EngineVisual, Name::new("Pistons"), SpatialBundle::default())).id(),
        rings:   commands.spawn((EngineVisual, Name::new("Piston Rings"), SpatialBundle::default())).id(),
        rods:    commands.spawn((EngineVisual, Name::new("Connecting Rods"), SpatialBundle::default())).id(),
        bores:   commands.spawn((EngineVisual, Name::new("Cylinder Bores"), SpatialBundle::default())).id(),
        block:   commands.spawn((EngineVisual, Name::new("Block Slices"), SpatialBundle::default())).id(),
        valves:  commands.spawn((EngineVisual, Name::new("Valves"), SpatialBundle::default())).id(),
        heads:   commands.spawn((EngineVisual, Name::new("Cylinder Heads"), SpatialBundle::default())).id(),
        manifolds: commands.spawn((EngineVisual, Name::new("Manifolds"), SpatialBundle::default())).id(),
    }
}

/// Compute world position from local (x along crank, y along bank axis, z lateral)
/// with the bank tilt applied. Shared by every per-cylinder submodule.
#[inline]
pub(super) fn tilt_position(x: f32, y_local: f32, z_local: f32, tilt: f32) -> Vec3 {
    let cos_t = tilt.cos();
    let sin_t = tilt.sin();
    Vec3::new(
        x,
        y_local * cos_t - z_local * sin_t,
        y_local * sin_t + z_local * cos_t,
    )
}
