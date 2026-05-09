# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build           # debug build
cargo build --release # optimized build
cargo run             # build and launch simulator
```

No test suite exists — validation is done by running the simulator.

## Architecture

A real-time combustion engine simulator built on **Bevy 0.14** (ECS), with procedural audio via **rodio** and an immediate-mode UI via **bevy_egui**.

`main.rs` registers five plugins in dependency order:
1. `EnginePlugin` — physics simulation
2. `VisualsPlugin` — 3D rendering and animation
3. `CameraPlugin` — orbit camera
4. `UiPlugin` — egui control panel
5. `EngineAudioPlugin` — procedural audio from exhaust pressure

### Physics loop (`src/engine.rs`)

Each Bevy frame runs ~80 substeps for numerical stability at high RPM. The single degree of freedom is the crankshaft (`omega`, `angle`, `fourstroke_angle`). Everything else is derived:

- Piston kinematics from slider-crank geometry (`engine/geometry.rs`)
- Per-cylinder gas state (mass, T, P, composition) stepped via ideal-gas law + compressible orifice flow through cam-driven valves (`engine/cylinder.rs`, `engine/thermo.rs`, `engine/valve.rs`)
- Wiebe burn function integrates combustion energy release over crank angle
- Net torque from all pistons → angular acceleration → updated `omega`
- Manifolds are fixed-volume plenums that exchange mass with cylinders and the atmosphere (`engine/manifold.rs`)

Run-state machine lives in `engine/state.rs`: Off → Cranking (E key engages starter) → Running (self-sustains above ~220 RPM stall threshold).

### Engine configurations (`src/engine/config.rs`)

Three presets, all dynamically switchable at runtime:
- **Inline-4** — 2.0L, 86×86 mm
- **V8 (cross-plane)** — 5.0L, 90° bank
- **Flat-6** — 3.8L, 180° boxer

On config change, the entire 3D engine is despawned and rebuilt by `VisualsPlugin`.

### Fuel system (`src/engine/fuel.rs`)

Six presets (Gasoline, E85, Methanol, Diesel, Hydrogen, Nitromethane) with differing LHV, stoichiometric AFR, burn rate, spark advance, and flame color. Fuel composition (air%, fuel%, burned%) is tracked per cylinder.

### Audio (`src/audio.rs`)

Purely procedural — no samples. Exhaust pressure pulses from the physics loop are linearly interpolated to a 44.1 kHz stream with a low-pass filter and fed into rodio's lock-free ring buffer.

### Visuals (`src/visuals/`)

Dynamically spawns meshes for crank, connecting rods, pistons, and valves. Animation systems read `EngineCore` state each frame and update transforms. Combustion flash color is driven by the selected fuel's flame color parameter.

## Roadmap (`roadmap.md`)

Planned features: forced induction, VVT/VTEC, ignition timing control, drivetrain/dyno load, cooling system, and P-V diagram UI.
