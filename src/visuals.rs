//! Rendering: spawns the engine geometry once, animates it from [`EngineCore`].
//!
//! The visuals are read-only consumers of the simulation: every system in
//! `animate` queries components + the resource and writes to `Transform`s and
//! material handles.  No physics happens here.

mod animate;
mod parts;

use bevy::prelude::*;

pub struct VisualsPlugin;

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, parts::setup_scene)
            .add_systems(
                Update,
                (
                    animate::animate_crank,
                    animate::animate_pistons,
                    animate::animate_rods,
                    animate::animate_valves,
                    animate::animate_cylinder_gas,
                    animate::animate_combustion_flash,
                    animate::animate_manifolds,
                )
                    .after(crate::engine::engine_step),
            );
    }
}

// ── Marker / data components shared across the submodules ───────────────────
#[derive(Component)] pub struct Crankshaft;
#[derive(Component)] pub struct Piston { pub idx: usize }
#[derive(Component)] pub struct ConRod { pub idx: usize, pub base_x: f32 }

#[derive(Component, Clone, Copy)]
pub enum ValveKind { Intake, Exhaust }

#[derive(Component)]
pub struct Valve {
    pub cyl: usize,
    pub kind: ValveKind,
    pub seat_y: f32,
}

#[derive(Component)]
pub struct CylinderGasViz {
    pub idx: usize,
    pub bore_material: Handle<StandardMaterial>,
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
