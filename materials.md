# Engine Material System Implementation Plan (Diegetic/Dynamic)

This revised plan introduces a generalized, physics-based material system. Rather than hardcoding interactions (like "rings score the block"), the system will simulate fundamental physical interactions—like sliding contacts and mechanical stress—using the properties of the assigned materials. The results (friction, heat, wear, failure) emerge naturally from the physics. It also includes visualizing part wear/damage dynamically.

## User Review Required

> [!WARNING]
> This moves the engine simulation towards a generalized physics solver for component interactions. Please review the generic `ContactSurface` approach and the visual damage representation to ensure it aligns with the level of detail and abstraction you want.

## Open Questions

> [!IMPORTANT]
> 1. **Wear Rate Scale**: Since real engine wear takes thousands of hours, should we introduce a global `wear_time_scale` multiplier to make wear visible during a short gameplay session?
> 2. **Failure Modes**: When a material fails (e.g., rod exceeds yield strength, or part reaches melting point), should the part be marked "destroyed" causing an immediate stall, or should it degrade gracefully until seizure?

## Proposed Changes

---

### `src/engine/material.rs`

[NEW] `src/engine/material.rs`

Define the generic `Material` structure and the fundamental math for how two materials interact.

*   `struct Material`:
    *   `name: &'static str`
    *   `hardness: f32` (e.g., Brinell hardness - dictates which part wears faster)
    *   `yield_strength: f32` (MPa - dictates mechanical failure point)
    *   `friction_coeff: f32` (Baseline dynamic friction)
    *   `thermal_conductivity: f32` (W/m·K)
    *   `specific_heat: f32` (J/kg·K)
    *   `melting_point: f32` (K)
    *   `density: f32` (kg/m³)
*   `struct ContactSurface`: Represents the boundary where two parts meet.
    *   `material_a: &Material`, `material_b: &Material`
    *   `pub fn evaluate(normal_force, sliding_velocity, dt)` -> Returns `(friction_force, heat_generated, wear_a, wear_b)`.
    *   **Logic**:
        *   Friction = normal force * combined friction coefficient.
        *   Heat = Friction * sliding velocity. Heat is distributed to A and B proportionally to their thermal conductivities.
        *   Wear = Archard wear equation. Volume loss is proportional to normal force and sliding distance, inversely proportional to the material's hardness. If `A` is tungsten and `B` is aluminum, `B` will suffer extreme wear while `A` suffers almost none.

---

### `src/engine/config.rs`

[MODIFY] `src/engine/config.rs`

Update the engine definition to assign `Material` references to each discrete part, rather than just geometric dimensions.

*   Add materials to `EngineConfig`:
    *   `block_material: Material`
    *   `piston_material: Material`
    *   `ring_material: Material`
    *   `rod_material: Material`
    *   `crank_material: Material`
    *   `bearing_material: Material`
*   Initialize default presets (e.g., Cast Iron Block, Forged Steel Crank, Babbit Bearings, Aluminum Pistons, Tungsten Rings).

---

### `src/engine/cylinder.rs` & `src/engine/crank.rs`

[MODIFY] `src/engine/cylinder.rs`
[MODIFY] `src/engine/crank.rs` (or equivalent where rotating assembly is handled)

Replace hardcoded friction, heat loss, and invulnerable parts with dynamic evaluations.

*   **Piston Ring vs Cylinder Wall Contact**:
    *   In `step_cylinder_cfg`, evaluate the `ContactSurface` between `ring_material` and `block_material`.
    *   Normal force = piston side-thrust (derived from rod angle and cylinder pressure) + ring tension.
    *   Sliding velocity = piston speed.
    *   Apply resulting friction torque against the crankshaft.
    *   Apply resulting heat to cylinder wall and piston temperatures.
    *   Accumulate wear. If `block_wear` increases, expand the clearance volume or reduce effective seal, causing compression loss (blow-by).
*   **Connecting Rod Stress**:
    *   Calculate the axial force through the connecting rod (gas pressure force + inertial force of piston mass).
    *   Calculate stress = Force / Cross-sectional area.
    *   If `stress > rod_material.yield_strength`, the rod yields (bends or snaps).
*   **Thermal Dynamics**:
    *   Replace hardcoded `wall_temp = 410.0` and `h_w = 480.0` with a dynamic thermal mass calculation using the block's `specific_heat`, `mass`, and `thermal_conductivity`.

---

### `src/engine/state.rs`

[MODIFY] `src/engine/state.rs`

Track the evolving health of the engine parts over time.

*   Add state variables to track wear/damage:
    *   In `CylinderState`: `wall_wear_depth: f32`, `ring_wear: f32`, `rod_health: f32`, `piston_temp: f32`, `block_temp: f32`.
    *   In `EngineCore`: `crank_journal_wear: Vec<f32>`.
*   Update the `EngineCore` state to check for catastrophic failures (e.g., rod snap drops cylinder contribution to 0 and adds massive friction/noise).

---

### `src/visuals/parts.rs` (or equivalent rendering module)

[MODIFY] `src/visuals/parts.rs`
[NEW/MODIFY] Explicit Piston Ring Geometry

Implement a system for viewing dynamic material damage visually.

*   **Visualizing Damage/Heat**:
    *   Create a linear color gradient scale mapping damage (or extreme heat) from `0.0` to `1.0`.
    *   `0.0` (Healthy) -> `Blue` or original material color.
    *   `0.5` (Moderate Damage/Warning) -> `Orange` glow.
    *   `1.0` (Severe Damage/Destroyed) -> `Red` glow.
    *   Apply this calculated color to the generated mesh vertices or materials in Bevy for each specific part (Block, Piston, Rod).
*   **Explicit Piston Rings**:
    *   Add geometry generation for the piston rings within the cylinder.
    *   Apply the same damage color mapping directly to the piston rings so the player can see if the rings themselves are failing vs the block failing.

## Verification Plan

### Manual Verification
1.  **Baseline Test**: Run with default materials (Cast Iron, Aluminum pistons). Verify temperatures stabilize and wear is negligible over a short run (parts stay blue/normal).
2.  **Hardness Mismatch Test**: Change piston rings to Tungsten and Block to Aluminum. The dynamic `ContactSurface` should calculate massive wear on the block. The cylinder block visual should glow orange and then red, while the tungsten rings remain blue (undamaged).
3.  **Strength Test**: Change rods to a weak material (e.g., Cast Aluminum) and over-rev the engine or add forced induction. Inertial or pressure forces should exceed the yield strength and break the rod dynamically, causing the rod's visual to instantly snap to red.
