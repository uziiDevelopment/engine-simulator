//! Rendering: spawns the engine geometry dynamically, animates it from [`EngineCore`].
//!
//! The visuals are read-only consumers of the simulation: every system in
//! `animate` queries components + the resource and writes to `Transform`s and
//! material handles.  No physics happens here.
//!
//! The scene is **fully dynamic**: when the engine config changes (detected via
//! `config_generation`), all engine entities are despawned and re-spawned to
//! match the new cylinder count / layout.

mod animate;
mod parts;

use bevy::prelude::*;

pub struct VisualsPlugin;

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(VisualGeneration(u64::MAX)) // force initial spawn
            .add_systems(Startup, parts::setup_static_scene)
            .add_systems(
                Update,
                (
                    rebuild_engine_visuals,
                    animate::animate_crank,
                    animate::animate_pistons,
                    animate::animate_rods,
                    animate::animate_valves,
                    animate::animate_cylinder_gas,
                    animate::animate_combustion_flash,
                    animate::animate_manifolds,
                )
                    .chain()
                    .after(crate::engine::engine_step),
            );
    }
}

// ── Resource tracking which generation we last spawned ───────────────────────
#[derive(Resource)]
struct VisualGeneration(u64);

/// Detects config changes and respawns the engine visual entities.
fn rebuild_engine_visuals(
    mut commands: Commands,
    core: Res<crate::engine::EngineCore>,
    mut vis_gen: ResMut<VisualGeneration>,
    engine_q: Query<Entity, With<EngineVisual>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if vis_gen.0 == core.config_generation {
        return;
    }
    // Despawn all existing engine visual entities
    for entity in &engine_q {
        commands.entity(entity).despawn_recursive();
    }
    // Spawn new visuals based on current config
    parts::spawn_engine_visuals(&mut commands, &core, &mut meshes, &mut materials);
    vis_gen.0 = core.config_generation;
}

// ── Marker / data components shared across the submodules ───────────────────

/// Marker on every entity that belongs to the engine visual (for despawn).
#[derive(Component)]
pub struct EngineVisual;

#[derive(Component)] pub struct Crankshaft;

#[derive(Component)]
pub struct Piston {
    pub idx: usize,
    /// Bank tilt angle (rad) — 0 for inline.
    pub bank_tilt: f32,
}

#[derive(Component)]
pub struct ConRod {
    pub idx: usize,
    pub base_x: f32,
    /// Bank tilt angle (rad) — 0 for inline.
    pub bank_tilt: f32,
}

#[derive(Component, Clone, Copy)]
pub enum ValveKind { Intake, Exhaust }

#[derive(Component)]
pub struct Valve {
    pub cyl: usize,
    pub kind: ValveKind,
    pub seat_y: f32,
    /// Bank tilt angle (rad) — 0 for inline.
    pub bank_tilt: f32,
}

#[derive(Component)]
pub struct CylinderGasViz {
    pub idx: usize,
    pub bore_material: Handle<StandardMaterial>,
    pub bank_tilt: f32,
}

#[derive(Component)]
pub struct CombustionFlash {
    pub cyl: usize,
    pub material: Handle<StandardMaterial>,
}

#[derive(Component, Clone, Copy)]
pub enum ManifoldKind { Intake, Exhaust }

#[derive(Component)]
pub struct ManifoldViz {
    pub kind: ManifoldKind,
    pub material: Handle<StandardMaterial>,
}
