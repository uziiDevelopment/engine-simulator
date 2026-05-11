//! 6-speed manual transmission: constant-mesh gearbox + simplified vehicle load.
//!
//! The clutch torque is computed in `engine_step` as today; what this module
//! adds is the *downstream* side: gear-ratio reflection, vehicle inertia and
//! road load, plus the bookkeeping for shift mechanics (lockout, money-shift,
//! no-clutch grind, clutch wear).

use std::f32::consts::TAU;

/// Selector position — what's slotted into the gear gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GearSelector {
    Reverse,
    Neutral,
    /// 1..=6
    Gear(u8),
}

impl GearSelector {
    /// Human-readable label for the dashboard.
    pub fn label(self) -> &'static str {
        match self {
            GearSelector::Reverse => "R",
            GearSelector::Neutral => "N",
            GearSelector::Gear(1) => "1",
            GearSelector::Gear(2) => "2",
            GearSelector::Gear(3) => "3",
            GearSelector::Gear(4) => "4",
            GearSelector::Gear(5) => "5",
            GearSelector::Gear(6) => "6",
            GearSelector::Gear(_) => "?",
        }
    }
}

/// Static gearbox + vehicle parameters. Lives alongside [`EngineConfig`].
#[derive(Clone, Debug)]
pub struct GearboxConfig {
    /// Forward gear ratios, 1st → 6th. Higher = more torque, lower speed.
    pub ratios: [f32; 6],
    /// Reverse gear ratio (negative — sign drives direction).
    pub reverse: f32,
    /// Final drive ratio (differential).
    pub final_drive: f32,
    /// Tyre rolling radius (m).
    pub tyre_radius: f32,
    /// Vehicle mass (kg).
    pub vehicle_mass: f32,
    /// Rolling-resistance coefficient (dimensionless, typ. 0.012).
    pub crr: f32,
    /// Frontal area × drag coefficient (m²).
    pub cd_a: f32,
}

impl Default for GearboxConfig {
    fn default() -> Self {
        Self {
            // Generic sports-car spread
            ratios: [3.6, 2.1, 1.5, 1.15, 0.95, 0.75],
            reverse: -3.5,
            final_drive: 3.9,
            tyre_radius: 0.32,
            vehicle_mass: 1500.0,
            crr: 0.012,
            cd_a: 0.7,
        }
    }
}

impl GearboxConfig {
    /// Total drive ratio (gear * final). Returns `None` in neutral.
    pub fn total_ratio(&self, sel: GearSelector) -> Option<f32> {
        match sel {
            GearSelector::Neutral => None,
            GearSelector::Reverse => Some(self.reverse * self.final_drive),
            GearSelector::Gear(n) if (1..=6).contains(&n) => {
                Some(self.ratios[(n - 1) as usize] * self.final_drive)
            }
            GearSelector::Gear(_) => None,
        }
    }
}

/// Mutable runtime state of the gearbox + vehicle.
#[derive(Clone, Debug)]
pub struct GearboxState {
    pub selector: GearSelector,
    /// Last frame's selector — used to detect shift events.
    pub prev_selector: GearSelector,
    /// Logical lever position on the H-pattern (XZ in cockpit-local space, range ~[-1,1]).
    pub lever_pos: bevy::math::Vec2,
    /// Lift-collar latched up so the next leftward+forward gate becomes reverse.
    pub reverse_armed: bool,
    /// Vehicle wheel angular velocity (rad/s, signed — negative in reverse).
    pub vehicle_omega: f32,
    /// Road speed (m/s, signed).
    pub road_speed: f32,
    /// Clutch wear 0..1 (irreversible, reduces max torque).
    pub clutch_wear: f32,
    /// Accumulated gearbox damage 0..1.
    pub gearbox_damage: f32,
    /// Animated angle of the layshaft (rad) — for visuals.
    pub layshaft_angle: f32,
    /// Animated angle of each forward gear cog (rad) — visual only.
    pub cog_angles: [f32; 6],
    /// Animated reverse idler angle (rad).
    pub reverse_idler_angle: f32,
    /// Last grind-shock impulse magnitude (Nm·s); decays after each frame for VFX.
    pub last_grind_impulse: f32,
}

impl Default for GearboxState {
    fn default() -> Self {
        Self {
            selector: GearSelector::Neutral,
            prev_selector: GearSelector::Neutral,
            lever_pos: bevy::math::Vec2::ZERO,
            reverse_armed: false,
            vehicle_omega: 0.0,
            road_speed: 0.0,
            clutch_wear: 0.0,
            gearbox_damage: 0.0,
            layshaft_angle: 0.0,
            cog_angles: [0.0; 6],
            reverse_idler_angle: 0.0,
            last_grind_impulse: 0.0,
        }
    }
}

impl GearboxState {
    #[inline]
    pub fn road_speed_kmh(&self) -> f32 { self.road_speed * 3.6 }
}

/// Result of `step_gearbox`: torques to feed back into the existing
/// drivetrain/engine integrator.
pub struct GearboxStepOutput {
    /// Net torque to apply to the drivetrain shaft this substep (Nm).
    pub drivetrain_tau: f32,
    /// Effective inertia at the drivetrain shaft (kg·m²) for this step.
    pub drivetrain_inertia_eff: f32,
    /// True if a money-shift just happened — caller should trigger seizure.
    pub money_shift: bool,
    /// True if a no-clutch grind shock just happened.
    pub grind_shock: bool,
}

/// Constant: standard gravity used for rolling resistance.
const G: f32 = 9.81;
/// Air density (kg/m³) at sea level, 15 °C.
const RHO_AIR: f32 = 1.225;

/// Step the gearbox+vehicle one substep. Splits the existing clutch_torque
/// between accelerating the gearbox/vehicle and being absorbed by drag, and
/// reflects vehicle inertia back to the drivetrain shaft.
///
/// `clutch_torque` is the torque from the clutch into the drivetrain (Nm).
/// `drivetrain_inertia` is the baseline shaft inertia (kg·m²).
/// `drivetrain_omega` is the current shaft angular velocity (rad/s).
pub fn step_gearbox(
    cfg: &GearboxConfig,
    state: &mut GearboxState,
    clutch_torque: f32,
    drivetrain_inertia: f32,
    drivetrain_omega: f32,
    redline_omega: f32,
    dt: f32,
) -> GearboxStepOutput {
    let mut money_shift = false;
    let mut grind_shock = false;

    // Detect shift events (selector change)
    if state.selector != state.prev_selector {
        // Money-shift: predict resulting engine omega when the clutch closes
        if let Some(r) = cfg.total_ratio(state.selector) {
            let predicted_engine_omega = state.vehicle_omega * r;
            if predicted_engine_omega.abs() > redline_omega * 1.15 {
                money_shift = true;
            }
        }
        state.prev_selector = state.selector;
    }

    let in_gear_ratio = cfg.total_ratio(state.selector);

    // Decay the grind-shock VFX accumulator
    state.last_grind_impulse = (state.last_grind_impulse - 30.0 * dt).max(0.0);

    let g = G;
    let rho = RHO_AIR;

    match in_gear_ratio {
        // ── In gear: drivetrain rigidly linked to vehicle via `r` ──────────
        Some(r) => {
            let r2 = r * r;
            // Reflect vehicle mass to drivetrain shaft as additional inertia
            let j_vehicle = cfg.vehicle_mass * (cfg.tyre_radius * cfg.tyre_radius) / r2;
            let j_eff = drivetrain_inertia + j_vehicle.max(0.0);

            // Road load force (N) at the wheel — always opposes motion
            let v = state.road_speed;
            let f_rolling = cfg.crr * cfg.vehicle_mass * g * v.signum();
            let f_aero = 0.5 * rho * cfg.cd_a * v * v.abs();
            let f_road = f_rolling + f_aero;
            // Reflect to drivetrain shaft (sign: drag opposes vehicle_omega)
            let tau_drag = f_road * cfg.tyre_radius / r;

            // Light shaft viscous drag retained from old code
            let tau_visc = drivetrain_omega * 0.05;

            let drivetrain_tau = clutch_torque - tau_drag - tau_visc;

            GearboxStepOutput {
                drivetrain_tau,
                drivetrain_inertia_eff: j_eff,
                money_shift,
                grind_shock,
            }
        }
        // ── Neutral: drivetrain free, vehicle coasts on road load ─────────
        None => {
            // Vehicle decelerates independently of the drivetrain
            let v = state.road_speed;
            if v.abs() > 1e-3 {
                let f_rolling = cfg.crr * cfg.vehicle_mass * g * v.signum();
                let f_aero = 0.5 * rho * cfg.cd_a * v * v.abs();
                let a = -(f_rolling + f_aero) / cfg.vehicle_mass;
                let new_v = v + a * dt;
                // Don't let road-load drag flip sign through zero
                state.road_speed = if v.signum() != new_v.signum() { 0.0 } else { new_v };
                state.vehicle_omega = state.road_speed / cfg.tyre_radius;
            }

            // Drivetrain free-spins: clutch_torque accelerates it, viscous drag pulls back
            let tau_visc = drivetrain_omega * 0.05;
            let drivetrain_tau = clutch_torque - tau_visc;

            // No grind path checked here — handled by the in-gear arm above
            let _ = grind_shock;

            GearboxStepOutput {
                drivetrain_tau,
                drivetrain_inertia_eff: drivetrain_inertia,
                money_shift,
                grind_shock,
            }
        }
    }
}

/// After the drivetrain has been integrated, propagate the result back to the
/// vehicle when in gear and advance the cosmetic cog angles.
pub fn post_integrate(
    cfg: &GearboxConfig,
    state: &mut GearboxState,
    engine_omega: f32,
    drivetrain_omega: f32,
    dt: f32,
) {
    if let Some(r) = cfg.total_ratio(state.selector) {
        state.vehicle_omega = drivetrain_omega / r;
        state.road_speed = state.vehicle_omega * cfg.tyre_radius;
    }
    // Layshaft is rigidly coupled to the engine input shaft via the constant-
    // mesh input pair. We use a fixed input-pair ratio of 1.4 — typical for a
    // sports 6-speed. Even in neutral, the layshaft spins with the engine.
    let layshaft_omega = engine_omega * 1.4;
    state.layshaft_angle = (state.layshaft_angle + layshaft_omega * dt).rem_euclid(TAU);
    for (i, ratio) in cfg.ratios.iter().enumerate() {
        let cog_omega = layshaft_omega / ratio;
        state.cog_angles[i] = (state.cog_angles[i] + cog_omega * dt).rem_euclid(TAU);
    }
    let rev_omega = layshaft_omega / cfg.reverse.abs();
    state.reverse_idler_angle = (state.reverse_idler_angle + rev_omega * dt).rem_euclid(TAU);
}

/// Apply a no-clutch grind shock: called when the user changes gear with the
/// clutch substantially engaged. Returns the impulse magnitude applied.
pub fn apply_grind_shock(state: &mut GearboxState, slip_at_input: f32) -> f32 {
    // Energy proxy ∝ slip². Damage accumulates fast.
    let energy = 0.5 * slip_at_input * slip_at_input;
    let damage_inc = (energy * 1e-5).min(0.15);
    state.gearbox_damage = (state.gearbox_damage + damage_inc).min(1.0);
    let impulse = energy.sqrt().min(200.0);
    state.last_grind_impulse = state.last_grind_impulse.max(impulse);
    impulse
}

/// Integrate clutch wear from `clutch_temp` (called each substep).
/// Wear only accumulates above 700 K (a hot, burning clutch).
pub fn step_clutch_wear(state: &mut GearboxState, clutch_temp: f32, dt: f32) {
    if clutch_temp > 700.0 {
        let inc = (clutch_temp - 700.0) * 2.0e-6 * dt;
        state.clutch_wear = (state.clutch_wear + inc).min(1.0);
    }
}

/// Effective max clutch torque after wear (multiplier).
#[inline]
pub fn clutch_wear_factor(state: &GearboxState) -> f32 {
    1.0 - 0.5 * state.clutch_wear
}

// ════════════════════════════════════════════════════════════════════════
// H-pattern gate geometry — shared between mouse and keyboard input
// ════════════════════════════════════════════════════════════════════════

/// Half-width of the centre "neutral channel". While `|lever.y| ≤ NEUTRAL_Y`
/// the lever rides freely on the centre rail; pushing past commits it to a
/// column gate.
const NEUTRAL_Y: f32 = 0.35;
/// How far off-axis the lever can stray from a column centre while still
/// counting as inside that gate's mouth.
const COLUMN_HALF_WIDTH: f32 = 0.35;
/// X positions of the three forward columns (1-2, 3-4, 5-6).
const COLUMNS: [f32; 3] = [-1.0, 0.0, 1.0];
/// X position of the reverse dogleg.
const REVERSE_X: f32 = -1.4;
/// Y threshold that counts as "engaged" once inside a gate.
const ENGAGE_Y: f32 = 0.55;

/// Project a proposed lever motion onto the H-pattern, mimicking the gate
/// plate of a real manual. Rules:
/// 1. On the centre rail (|prev.y| ≤ NEUTRAL_Y), x roams freely; y is clamped.
/// 2. Once in a gate (|prev.y| > NEUTRAL_Y), x is pinned to that gate's column
///    until y returns to the centre rail.
/// 3. To enter the reverse dogleg, `reverse_armed` must be true and the move
///    must come from the centre column in a forward direction.
pub fn constrain_lever(
    target: bevy::math::Vec2,
    prev: bevy::math::Vec2,
    reverse_armed: bool,
) -> bevy::math::Vec2 {
    use bevy::math::Vec2;

    // If we were already committed to a gate, stay in it.
    if prev.y.abs() > NEUTRAL_Y {
        let pin_x = if reverse_armed && prev.x < -1.05 && prev.y > 0.0 {
            REVERSE_X
        } else {
            nearest_column(prev.x)
        };
        let y = if pin_x <= REVERSE_X {
            target.y.clamp(-1.1, 1.6) // reverse can sit deeper
        } else {
            target.y.clamp(-1.1, 1.1)
        };
        return Vec2::new(pin_x, y);
    }

    // Centre rail: x free, y bounded
    if target.y.abs() <= NEUTRAL_Y {
        return Vec2::new(target.x.clamp(-1.5, 1.5), target.y);
    }

    // Trying to enter a gate from the rail. Either reverse (if armed) or the
    // nearest forward column if we're within its mouth.
    if reverse_armed && target.y > NEUTRAL_Y && target.x < -1.05 {
        return Vec2::new(REVERSE_X, target.y.clamp(-1.1, 1.6));
    }
    let col = nearest_column(target.x);
    if (target.x - col).abs() <= COLUMN_HALF_WIDTH {
        Vec2::new(col, target.y.clamp(-1.1, 1.1))
    } else {
        // Not aligned with any column — gate plate blocks. Cap y at neutral.
        Vec2::new(target.x.clamp(-1.5, 1.5), target.y.signum() * NEUTRAL_Y)
    }
}

#[inline]
fn nearest_column(x: f32) -> f32 {
    let mut best = COLUMNS[0];
    let mut best_d = (x - COLUMNS[0]).abs();
    for &c in &COLUMNS[1..] {
        let d = (x - c).abs();
        if d < best_d { best_d = d; best = c; }
    }
    best
}

/// Map a constrained lever position to the gate it currently occupies.
/// Returns `Neutral` if the lever is still on the centre rail.
pub fn gate_for_position(
    lever: bevy::math::Vec2,
    reverse_armed: bool,
) -> GearSelector {
    let x = lever.x;
    let y = lever.y;

    if reverse_armed && x <= REVERSE_X + 0.15 && y > ENGAGE_Y {
        return GearSelector::Reverse;
    }
    let col = if x < -0.5 { 0 } else if x > 0.5 { 2 } else { 1 };
    let centre = COLUMNS[col];
    if (x - centre).abs() > COLUMN_HALF_WIDTH {
        return GearSelector::Neutral;
    }
    if y > ENGAGE_Y { return GearSelector::Gear([1, 3, 5][col]); }
    if y < -ENGAGE_Y { return GearSelector::Gear([2, 4, 6][col]); }
    GearSelector::Neutral
}

/// Default lever position for a given selector — used when the player presses
/// number/letter shortcuts (no continuous drag) or releases the mouse.
pub fn snapped_lever_for(sel: GearSelector) -> bevy::math::Vec2 {
    use bevy::math::Vec2;
    match sel {
        GearSelector::Neutral => Vec2::ZERO,
        GearSelector::Gear(1) => Vec2::new(-1.0,  1.0),
        GearSelector::Gear(2) => Vec2::new(-1.0, -1.0),
        GearSelector::Gear(3) => Vec2::new( 0.0,  1.0),
        GearSelector::Gear(4) => Vec2::new( 0.0, -1.0),
        GearSelector::Gear(5) => Vec2::new( 1.0,  1.0),
        GearSelector::Gear(6) => Vec2::new( 1.0, -1.0),
        GearSelector::Reverse => Vec2::new(REVERSE_X,  1.3),
        GearSelector::Gear(_) => Vec2::ZERO,
    }
}
