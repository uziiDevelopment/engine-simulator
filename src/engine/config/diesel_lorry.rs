use super::{EngineConfig, EngineLayout, MaterialsConfig};
use crate::engine::turbo::TurboConfig;
use std::f32::consts::PI;

pub fn preset() -> EngineConfig {
    // ── 12.8 L Inline-6 Turbodiesel (Euro Heavy Truck) ──────────────────────
    //
    // Modelled after the Volvo D13 / Scania DC13 class of heavy-truck diesel
    // straight-sixes (bore 131 mm × stroke 158 mm = 12.78 L).  Key physics
    // differences from SI presets:
    //
    //   • CR 17.3 : 1  — far higher than gasoline; produces ~940 K at TDC
    //     under adiabatic compression, guaranteeing auto-ignition of diesel
    //     fuel (threshold ≈ 523 K) on every stroke once the engine is warm.
    //
    //   • is_ci fuel  — the cylinder model takes pure-air intake and adds
    //     fuel via direct injection at 5° BTDC; no spark event is required.
    //
    //   • Governed redline 2 200 RPM — torque peak around 950–1 200 RPM,
    //     rated power at ~1 800 RPM.
    //
    //   • Large VGT with 2.0 bar boost and a highly effective intercooler
    //     (85 % effectiveness), matching Euro-class truck specifications.
    //
    // Valve timing (from Scania DC13 data):
    //   EVO 47° BBDC (133°), EVC 11° ATDC (371°)
    //   IVO  9° BTDC (351°), IVC 37° ABDC (577°)
    //   Overlap: 20° — minimal, typical for CI engines.

    let bore   = 0.131_f32;   // 131 mm
    let stroke = 0.158_f32;   // 158 mm

    EngineConfig {
        name: "12.8L I6 Diesel (Euro Truck)",
        layout: EngineLayout::Inline,
        bank_angle: 0.0,
        num_cylinders: 6,
        bore,
        stroke,
        // rod-length / crank-radius ratio ≈ 3.2  (conservative heavy-duty ratio)
        rod_length: 0.253,
        compression_ratio: 17.3,

        // Standard inline-6 mirror-symmetrical crank (three shared throws at
        // 0°, 120°, 240°) with firing order 1-5-3-6-2-4.
        crank_phases: vec![
            0.0,
            4.0 * PI / 3.0,
            2.0 * PI / 3.0,
            2.0 * PI / 3.0,
            4.0 * PI / 3.0,
            0.0,
        ],
        firing_offsets_deg: vec![0.0, 480.0, 240.0, 600.0, 120.0, 360.0],

        // ── Dynamics ────────────────────────────────────────────────────────
        // Heavy rotating assembly: 6-cylinder forged crank + large flywheel.
        flywheel_inertia:    3.0,    // kg·m² — high inertia keeps idle smooth
        clutch_max_torque:   2_800.0,// Nm  — heavy-duty twin-plate clutch
        clutch_thermal_mass: 6_000.0,// J/K
        clutch_cooling_coeff: 2.5,
        drivetrain_inertia:  2.5,    // kg·m² — heavy truck gearbox + prop shaft

        // Large bore + heavy pistons = higher baseline friction than a car engine.
        friction_base:    70.0,      // Nm
        friction_viscous:  0.22,     // Nm·s/rad
        friction_windage:  0.00035,  // Nm·s²/rad²

        // ── Starter ─────────────────────────────────────────────────────────
        // 24 V heavy-duty starter; disengages once CI firing sustains > 400 RPM.
        starter_torque:          550.0,
        starter_disengage_rpm:   400.0,

        // ── Limits ──────────────────────────────────────────────────────────
        redline_rpm: 2_200.0,
        stall_rpm:     300.0,  // diesel can idle much lower than gasoline

        // ── Throttle body ───────────────────────────────────────────────────
        // Diesel engines are air-unthrottled; the throttle plate is always
        // near-fully open.  Power is controlled by injected fuel quantity
        // (represented here by the AFR of the CI fuel preset).
        // Using a large throttle area with high idle bleed approximates
        // full-time unthrottled air admission.
        throttle_area_max: 0.016,    // m² — very large (no restriction intent)
        idle_bleed_frac:   0.90,     // 90 % open at zero "throttle"
        idle_throttle_min: 0.88,     // minimum effective throttle ≈ full-open

        // ── Manifolds ───────────────────────────────────────────────────────
        intake_volume:  0.022,   // m³  — 22 L charge pipe + intercooler plenum
        exhaust_volume: 0.014,   // m³  — 14 L exhaust manifold + DPF pre-volume
        tailpipe_area:  0.0085,  // m²  — 85 cm² single large exhaust stack

        // ── Valve timing (Scania DC13 reference) ────────────────────────────
        intake_open_deg:   351.0,   //  9° BTDC intake TDC
        intake_close_deg:  577.0,   // 37° ABDC
        exhaust_open_deg:  133.0,   // 47° BBDC
        exhaust_close_deg: 371.0,   // 11° ATDC exhaust TDC
        intake_peak_lift:  0.013,   // 13 mm
        exhaust_peak_lift: 0.013,
        // Single large valves per cylinder (model simplification).
        // Real DC13 uses 2 intake + 2 exhaust valves; effective area is similar.
        intake_valve_diameter:  0.048, // 48 mm effective
        exhaust_valve_diameter: 0.042, // 42 mm effective

        // ── Intake runner ────────────────────────────────────────────────────
        // Long runners tune volumetric efficiency toward the 900–1 200 RPM
        // torque peak typical of heavy-truck diesels.
        intake_runner_length: 0.50,   // 500 mm — long for low-RPM torque
        intake_runner_area:   3.2e-3, // 32 cm²

        cylinder_spacing: 0.158, // = stroke — typical for undersquare diesel

        // ── Materials ────────────────────────────────────────────────────────
        // Cast-iron block and cylinder walls, aluminium-alloy pistons (most
        // modern diesels use composite/monotherm crowns but Al-alloy is the
        // closest single-material approximation), forged-steel conrods.
        materials: MaterialsConfig::default_for_bore(bore),

        // ── Turbocharger — large VGT with intercooler ────────────────────────
        // Sized for 12.78 L displacement at 2.0 bar gauge boost.
        // The slow-spooling heavy shaft_inertia reproduces diesel turbo lag.
        turbos: vec![
            TurboConfig {
                enabled: true,
                target_boost_pa: 2.0e5,    // 2.0 bar gauge (3.0 bar absolute)
                shaft_inertia:   1.5e-4,   // large VGT rotor — notable spool lag
                max_shaft_rad_s: 8_800.0,  // ≈ 84 000 RPM — large turbo speed limit
                turbine_efficiency:    0.76,
                compressor_efficiency: 0.74,
                // Flow areas scaled proportionally for 12.78 L vs 500 cc base.
                turbine_area:  0.00330,    // m²  — large turbine inlet
                wastegate_area: 0.00215,   // m²  — VGT bypass gate
                impeller_radius: 0.062,    // 124 mm impeller — large truck wheel
                compressor_area: 0.00445,  // m²
                boost_plenum_volume: 0.016,// 16 L charge pipe + intercooler volume
                intercooler_effectiveness: 0.85, // highly effective truck intercooler
                bov_threshold_pa: 0.60e5,  // BOV opens at 0.6 bar above target
                blade_count: 15,           // more blades on large-diameter wheel
            },
        ],
    }
}
