//! egui-based control + telemetry panel.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::engine::{
    EngineCore, RunState, ENGINES, FUELS, MATERIAL_CATALOG, OIL_GRADES, P_ATM,
    engine_count, fuel_count, BearingState, DynoState, DynoPhase,
};
// Disambiguate from `bevy::prelude::Material` (a trait) — we want our
// engine-physics `Material` struct.
use crate::engine::material::Material as PhysMaterial;
use crate::visuals::EngineVisual;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, ui_panel);
    }
}

fn ui_panel(
    mut ctx: EguiContexts, 
    mut core: ResMut<EngineCore>,
    mut dyno: ResMut<DynoState>,
    mut visual_query: Query<(Entity, &Name, Option<&Children>, Option<&Parent>, &mut Visibility), With<EngineVisual>>,
) {
    let rpm = core.rpm();
    let state_text = match core.run_state {
        RunState::Off      => "OFF",
        RunState::Cranking => "CRANKING",
        RunState::Running  => "RUNNING",
    };
    let state_color = match core.run_state {
        RunState::Off      => egui::Color32::from_rgb(190, 70, 70),
        RunState::Cranking => egui::Color32::from_rgb(220, 180, 60),
        RunState::Running  => egui::Color32::from_rgb(80, 200, 100),
    };

    // ══════════════════════════════════════════════════════════════════════════
    // LEFT PANEL — Controls
    // ══════════════════════════════════════════════════════════════════════════
    egui::SidePanel::left("left_panel")
        .resizable(true)
        .default_width(260.0)
        .width_range(200.0..=360.0)
        .show(ctx.ctx_mut(), |ui| {
            egui::ScrollArea::vertical().id_salt("left_scroll").show(ui, |ui| {
                ui.add_space(6.0);
                ui.heading("Engine Control");
                ui.add_space(4.0);
                ui.separator();

                // ── Status header ────────────────────────────────────────────
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("State").strong());
                    ui.colored_label(state_color, egui::RichText::new(state_text).strong());
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("RPM").strong());
                    ui.label(egui::RichText::new(format!("{:>5.0}", rpm))
                        .monospace().size(18.0).color(state_color));
                });
                rpm_bar(ui, rpm, core.config.redline_rpm);

                ui.add_space(8.0);
                ui.separator();

                // ── Engine selector ─────────────────────────────────────────
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Engine").strong());
                let mut eng_idx = core.config_idx;
                egui::ComboBox::from_id_salt("engine_combo")
                    .width(ui.available_width() - 8.0)
                    .selected_text(ENGINES[eng_idx].name)
                    .show_ui(ui, |ui| {
                        for i in 0..engine_count() {
                            ui.selectable_value(&mut eng_idx, i, ENGINES[i].name);
                        }
                    });
                if eng_idx != core.config_idx {
                    core.select_engine(eng_idx);
                }

                ui.add_space(4.0);
                ui.separator();

                // ── Fuel selector ────────────────────────────────────────────
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Fuel").strong());
                let mut idx = core.fuel_idx;
                egui::ComboBox::from_id_salt("fuel_combo")
                    .width(ui.available_width() - 8.0)
                    .selected_text(FUELS[idx].name)
                    .show_ui(ui, |ui| {
                        for i in 0..fuel_count() {
                            ui.selectable_value(&mut idx, i, FUELS[i].name);
                        }
                    });
                if idx != core.fuel_idx {
                    core.select_fuel(idx);
                }
                let f = &core.fuel;
                ui.label(egui::RichText::new(format!(
                    "LHV {:.1} MJ/kg  AFRₛ {:.1}  burn {:.0}°  ign {:.0}° BTDC",
                    f.lhv / 1e6, f.afr_stoich, f.burn_duration_deg, f.spark_advance_deg
                )).small());

                ui.add_space(8.0);
                ui.separator();

                // ── Throttle ─────────────────────────────────────────────────
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Throttle").strong());
                ui.spacing_mut().slider_width = ui.available_width() - 60.0;
                ui.add(
                    egui::Slider::new(&mut core.throttle, 0.0..=1.0)
                        .show_value(true)
                        .custom_formatter(|v, _| format!("{:>3.0}%", v * 100.0))
                        .custom_parser(|s| s.trim_end_matches('%').parse::<f64>().ok().map(|v| v / 100.0)),
                );

                // ── Time scale ───────────────────────────────────────────────
                ui.add_space(6.0);
                ui.label(egui::RichText::new("Time Scale").strong());
                ui.add(
                    egui::Slider::new(&mut core.time_scale, 0.02..=1.0)
                        .show_value(true).logarithmic(true)
                        .custom_formatter(|v, _| {
                            if v >= 0.995 { "1.00×".to_string() }
                            else { format!("{:.2}×", v) }
                        })
                        .custom_parser(|s| s.trim_end_matches('×').trim().parse::<f64>().ok()),
                );
                ui.horizontal(|ui| {
                    if ui.small_button("1×").clicked()    { core.time_scale = 1.0; }
                    if ui.small_button("½×").clicked()    { core.time_scale = 0.5; }
                    if ui.small_button("¼×").clicked()    { core.time_scale = 0.25; }
                    if ui.small_button("⅒×").clicked()    { core.time_scale = 0.10; }
                    if ui.small_button("1/20×").clicked() { core.time_scale = 0.05; }
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // ── Audio ────────────────────────────────────────────────────
                ui.checkbox(&mut core.audio_enabled, "Audio Simulation");
                ui.checkbox(&mut core.particles_enabled, "Gas Flow Particles");
                ui.checkbox(&mut core.damage_view, "Damage View (FEA gradient)");

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // ── Materials & Damage ───────────────────────────────────────
                ui.collapsing(egui::RichText::new("Materials & Damage").strong(), |ui| {
                    material_selector(ui, "Block",        &mut core.config.materials.block);
                    material_selector(ui, "Cyl. Wall",    &mut core.config.materials.cylinder_wall);
                    material_selector(ui, "Piston",       &mut core.config.materials.piston);
                    material_selector(ui, "Piston Ring",  &mut core.config.materials.piston_ring);
                    material_selector(ui, "Connecting Rod", &mut core.config.materials.conrod);

                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Journal Bearings").small().strong());
                    material_selector(ui, "Main Brg Shell", &mut core.config.materials.main_bearing.shell_material);
                    material_selector(ui, "Main Brg Jrnl",  &mut core.config.materials.main_bearing.journal_material);
                    material_selector(ui, "Rod Brg Shell",  &mut core.config.materials.rod_bearing.shell_material);
                    material_selector(ui, "Rod Brg Jrnl",   &mut core.config.materials.rod_bearing.journal_material);
                    material_selector(ui, "Cam Brg Shell",  &mut core.config.materials.cam_bearing.shell_material);

                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Wear Time Scale").small());
                    ui.add(
                        egui::Slider::new(&mut core.wear_time_scale, 1.0..=1.0e6)
                            .logarithmic(true)
                            .custom_formatter(|v, _| format!("{:.0}×", v))
                    );
                    ui.label(egui::RichText::new(
                        "Multiplies the Archard wear constant. 1× = realistic-slow, \
                         1e4×+ = damage in seconds of abuse.",
                    ).small().weak());

                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Lubrication").small().strong());
                    
                    let g_idx = core.oil_config.grade_idx;
                    let selected_text = if core.oil_config.custom_grade { "Custom" } else { OIL_GRADES[g_idx].name };
                    
                    egui::ComboBox::from_id_salt("oil_grade_combo")
                        .width(ui.available_width() - 8.0)
                        .selected_text(selected_text)
                        .show_ui(ui, |ui| {
                            for (i, grade) in OIL_GRADES.iter().enumerate() {
                                let is_selected = !core.oil_config.custom_grade && g_idx == i;
                                if ui.selectable_label(is_selected, grade.name).clicked() {
                                    core.oil_config.grade_idx = i;
                                    core.oil_config.custom_grade = false;
                                    core.oil_config.v40 = grade.v40;
                                    core.oil_config.v100 = grade.v100;
                                    core.oil_config.recalculate_constants();
                                }
                            }
                        });
                    
                    if !core.oil_config.custom_grade {
                        ui.label(egui::RichText::new(OIL_GRADES[g_idx].description).small().weak());
                    } else {
                        ui.label(egui::RichText::new("Custom viscosity profile").small().weak());
                    }

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("40°C").small());
                        if ui.add(egui::Slider::new(&mut core.oil_config.v40, 5.0..=400.0).suffix(" cSt")).changed() {
                            core.oil_config.custom_grade = true;
                            core.oil_config.recalculate_constants();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("100°C").small());
                        if ui.add(egui::Slider::new(&mut core.oil_config.v100, 2.0..=60.0).suffix(" cSt")).changed() {
                            core.oil_config.custom_grade = true;
                            core.oil_config.recalculate_constants();
                        }
                    });

                    ui.add(
                        egui::Slider::new(&mut core.oil_config.viscosity_multiplier, 0.5..=2.0)
                            .text("Tuning Mult.")
                    );
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Slider::new(&mut core.oil_config.cooler_dissipation, 0.0..=1000.0)
                                .text("Cooler Size")
                                .suffix(" W/K")
                        );
                        ui.checkbox(&mut core.oil_config.fan_active, "Fan");
                    });

                    let therm_open = ((core.oil.temperature - 353.0) / 10.0).clamp(0.0, 1.0);
                    let state_text = if therm_open <= 0.0 {
                        egui::RichText::new("Thermostat: CLOSED").color(egui::Color32::from_rgb(100, 180, 255))
                    } else if therm_open >= 1.0 {
                        egui::RichText::new("Thermostat: OPEN").color(egui::Color32::from_rgb(255, 100, 100))
                    } else {
                        egui::RichText::new("Thermostat: OPENING").color(egui::Color32::from_rgb(255, 200, 100))
                    };
                    ui.label(state_text.small());

                    // Update current state viscosity immediately so telemetry stays in sync
                    core.oil.viscosity = crate::engine::viscosity_for(
                        core.oil.temperature,
                        core.oil_config.viscosity_a,
                        core.oil_config.viscosity_b,
                        core.oil_config.viscosity_multiplier,
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Drain Oil").clicked() {
                            core.oil.drain();
                        }
                        if ui.button("Refill Oil").clicked() {
                            let cfg = core.oil_config.clone();
                            core.oil.refill(&cfg);
                        }
                    });
                    if ui.button("Reset Damage (heal everything)").clicked() {
                        for c in core.cylinders.iter_mut() {
                            c.wall_wear = 0.0;
                            c.ring_wear = 0.0;
                            c.rod_damage = 0.0;
                            c.block_temp = 290.0;
                            c.piston_temp = 290.0;
                        }
                        for b in core.main_bearings.iter_mut() { *b = BearingState::fresh(); }
                        for b in core.rod_bearings.iter_mut()  { *b = BearingState::fresh(); }
                        for b in core.cam_bearings.iter_mut()  { *b = BearingState::fresh(); }
                        core.engine_seized = false;
                        let cfg = core.oil_config.clone();
                        core.oil.refill(&cfg);
                    }
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // ── Dyno Testing ─────────────────────────────────────────────
                ui.collapsing(egui::RichText::new("Dyno Testing").strong(), |ui| {
                    // Status
                    let (status_text, status_color) = match dyno.phase {
                        DynoPhase::Idle     => ("IDLE".to_string(), egui::Color32::from_gray(140)),
                        DynoPhase::Sweeping => (format!("SWEEP @ {:.0} RPM", dyno.target_rpm), egui::Color32::from_rgb(80, 200, 100)),
                        DynoPhase::Complete => ("COMPLETE".to_string(), egui::Color32::from_rgb(100, 180, 255)),
                    };
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Status").small());
                        ui.colored_label(status_color, egui::RichText::new(&status_text).strong().monospace());
                    });

                    // Progress bar during sweep
                    if dyno.active && dyno.end_rpm > dyno.start_rpm {
                        let progress = (dyno.target_rpm - dyno.start_rpm) / (dyno.end_rpm - dyno.start_rpm);
                        let bar = egui::ProgressBar::new(progress.clamp(0.0, 1.0))
                            .text(format!("{:.0}%", progress * 100.0));
                        ui.add(bar);
                    }

                    ui.add_space(4.0);

                    // Start / Stop button
                    ui.horizontal(|ui| {
                        if dyno.active {
                            if ui.button(egui::RichText::new("Stop Dyno").strong()).clicked() {
                                dyno.stop();
                            }
                        } else {
                            let can_start = core.run_state == RunState::Running;
                            let btn = egui::Button::new(egui::RichText::new("Start Dyno Run").strong());
                            if ui.add_enabled(can_start, btn).clicked() {
                                let name = core.config.name;
                                let redline = core.config.redline_rpm;
                                dyno.start(name, redline);
                            }
                            if !can_start {
                                ui.label(egui::RichText::new("(engine must be running)").small().weak());
                            }
                        }
                    });

                    ui.add_space(4.0);
                    ui.separator();
                    ui.add_space(2.0);

                    // Sweep config (only when not running)
                    ui.add_enabled_ui(!dyno.active, |ui| {
                        ui.label(egui::RichText::new("Sweep Config").small().strong());
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Start").small());
                            ui.add(egui::DragValue::new(&mut dyno.start_rpm).speed(50.0).range(500.0..=5000.0).suffix(" RPM"));
                        });
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Ramp").small());
                            ui.add(egui::DragValue::new(&mut dyno.ramp_rate).speed(10.0).range(50.0..=2000.0).suffix(" RPM/s"));
                        });
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Sample").small());
                            ui.add(egui::DragValue::new(&mut dyno.sample_interval).speed(10.0).range(25.0..=500.0).suffix(" RPM"));
                        });
                        ui.add_space(2.0);
                        let sweep_time = (core.config.redline_rpm - dyno.start_rpm) / dyno.ramp_rate;
                        ui.label(egui::RichText::new(format!(
                            "End: redline ({:.0})  ~{:.1}s sweep",
                            core.config.redline_rpm, sweep_time,
                        )).small().weak());
                    });


                    // Peak results
                    if !dyno.results.is_empty() {
                        ui.add_space(4.0);
                        ui.separator();
                        ui.add_space(2.0);
                        ui.label(egui::RichText::new("Peak Results").small().strong());
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(format!(
                                "{:.1} HP @ {:.0} RPM",
                                dyno.peak_hp, dyno.peak_hp_rpm,
                            )).monospace().strong().color(egui::Color32::from_rgb(255, 100, 100)));
                        });
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(format!(
                                "{:.1} Nm @ {:.0} RPM",
                                dyno.peak_torque, dyno.peak_torque_rpm,
                            )).monospace().strong().color(egui::Color32::from_rgb(100, 180, 255)));
                        });
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(format!(
                                "{:.1} kW peak",
                                dyno.peak_hp / 1.341,
                            )).monospace().small());
                        });
                    }
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // ── Controls + spec ──────────────────────────────────────────
                ui.label(egui::RichText::new("Controls").strong());
                ui.label("E — starter motor");
                ui.label("RMB — orbit");
                ui.label("MMB — pan");
                ui.label("Scroll — zoom");
                ui.label("F — frame engine");

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);
                ui.label(egui::RichText::new(format!(
                    "{:.1}L  bore {:.0}mm  stroke {:.0}mm",
                    core.config.total_displacement() * 1000.0,
                    core.config.bore * 1000.0,
                    core.config.stroke * 1000.0,
                )).small());
                ui.label(egui::RichText::new(format!(
                    "rod {:.0}mm  {} cyl  redline {:.0}",
                    core.config.rod_length * 1000.0,
                    core.config.num_cylinders,
                    core.config.redline_rpm,
                )).small());

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Render Tree").strong());
                ui.add_space(2.0);

                // Build a temporary tree to avoid borrow checker issues with recursion + &mut Query
                let mut roots = Vec::new();
                for (entity, _, _, parent, _) in visual_query.iter() {
                    let is_root = parent.map_or(true, |p| !visual_query.contains(p.get()));
                    if is_root {
                        roots.push(entity);
                    }
                }
                roots.sort_by_cached_key(|e| visual_query.get(*e).map(|(_, n, _, _, _)| n.to_string()).unwrap_or_default());

                for root in roots {
                    draw_render_tree(ui, root, &mut visual_query);
                }
            });
        });

    // ══════════════════════════════════════════════════════════════════════════
    // RIGHT PANEL — Telemetry
    // ══════════════════════════════════════════════════════════════════════════
    egui::SidePanel::right("right_panel")
        .resizable(true)
        .default_width(280.0)
        .width_range(220.0..=380.0)
        .show(ctx.ctx_mut(), |ui| {
            egui::ScrollArea::vertical().id_salt("right_scroll").show(ui, |ui| {
                ui.add_space(6.0);
                ui.heading("Telemetry");
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                telemetry_grid(ui, &core);

                ui.add_space(6.0);

                // ── Seized banner ────────────────────────────────────────────
                if core.engine_seized {
                    let rect_resp = ui.allocate_response(
                        egui::vec2(ui.available_width(), 45.0),
                        egui::Sense::hover(),
                    );
                    let painter = ui.painter();
                    painter.rect_filled(rect_resp.rect, 4.0, egui::Color32::from_rgb(170, 30, 30));
                    
                    let center = rect_resp.rect.center();
                    
                    painter.text(
                        center - egui::vec2(0.0, 8.0),
                        egui::Align2::CENTER_CENTER,
                        format!("ENGINE SEIZED: {}", core.seizure_reason),
                        egui::FontId::proportional(13.0),
                        egui::Color32::WHITE,
                    );
                    painter.text(
                        center + egui::vec2(0.0, 8.0),
                        egui::Align2::CENTER_CENTER,
                        "Reset Damage to recover",
                        egui::FontId::proportional(12.0),
                        egui::Color32::from_white_alpha(200),
                    );
                    ui.add_space(6.0);
                }

                // ── Oil gauges ───────────────────────────────────────────────
                ui.label(egui::RichText::new("Lubrication").strong());
                let oil_kpa = core.oil.pressure / 1000.0;
                let oil_pct_full = (core.oil.mass / core.oil_config.capacity).clamp(0.0, 1.0);
                let oil_temp_c = core.oil.temperature - 273.15;
                let lube = core.oil.lubrication_factor(&core.oil_config);
                egui::Grid::new("oil_grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                    cell(ui, "Oil press.", &format!("{:>5.1} kPa", oil_kpa));
                    cell(ui, "Oil temp",   &format!("{:>5.1} °C",  oil_temp_c));
                    ui.end_row();
                    cell(ui, "Sump",       &format!("{:>3.0}%",    oil_pct_full * 100.0));
                    cell(ui, "Lube",       &format!("{:>3.0}%",    lube * 100.0));
                    ui.end_row();
                    cell(ui, "Visc.",      &format!("{:.3} Pa·s",  core.oil.viscosity));
                    cell(ui, "VI Slope",   &format!("{:.0}",       core.oil_config.viscosity_b));
                    ui.end_row();
                    cell(ui, "Frict. Q",   &format!("{:>4.0} W",   core.friction_heat_smoothed));
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // ── Per-cylinder pressure mini-bars ──────────────────────────
                ui.label(egui::RichText::new("Cylinder Pressure").strong());
                ui.add_space(2.0);
                for i in 0..core.num_cyl() {
                    let cyl = &core.cylinders[i];
                    let pr = cyl.last_pressure / P_ATM;
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("Cyl {}", i + 1))
                            .monospace().size(11.0));
                        pressure_minibar(ui, pr, cyl.temperature, cyl.flash, &core.fuel.flame_color);
                    });
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // ── Per-cylinder wear / temp ─────────────────────────────────
                ui.label(egui::RichText::new("Damage").strong());
                ui.add_space(2.0);
                for i in 0..core.num_cyl() {
                    let cyl = &core.cylinders[i];
                    let rod_brg_wear = core.rod_bearings.get(i)
                        .map(|b| b.shell_wear).unwrap_or(0.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("Cyl {}", i + 1))
                            .monospace().size(11.0));
                        wear_minibar(ui, "Wall", cyl.wall_wear);
                        wear_minibar(ui, "Ring", cyl.ring_wear);
                        wear_minibar(ui, "Rod",  cyl.rod_damage);
                        wear_minibar(ui, "Brg",  rod_brg_wear);
                    });
                    let brg_temp = core.rod_bearings.get(i)
                        .map(|b| b.temperature).unwrap_or(290.0);
                    ui.label(egui::RichText::new(format!(
                        "    block {:.0} K   piston {:.0} K   brg {:.0} K   \u{03c3} {:.0} MPa",
                        cyl.block_temp, cyl.piston_temp, brg_temp, cyl.last_rod_stress / 1.0e6,
                    )).small().monospace());
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // ── Main & cam bearings ──────────────────────────────────────
                ui.label(egui::RichText::new("Journal Bearings").strong());
                ui.add_space(2.0);

                for (idx, brg) in core.main_bearings.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("Main {}", idx + 1))
                            .monospace().size(11.0));
                        wear_minibar(ui, "Shell", brg.shell_wear);
                        let status = if brg.spun { " SPUN" }
                            else if brg.wiped { " WIPED" }
                            else { "" };
                        ui.label(egui::RichText::new(format!(
                            "{:.0} K  S={:.2}{}",
                            brg.temperature, brg.sommerfeld, status,
                        )).small().monospace());
                    });
                }

                for (idx, brg) in core.cam_bearings.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("Cam  {}", idx + 1))
                            .monospace().size(11.0));
                        wear_minibar(ui, "Shell", brg.shell_wear);
                        let status = if brg.spun { " SPUN" }
                            else if brg.wiped { " WIPED" }
                            else { "" };
                        ui.label(egui::RichText::new(format!(
                            "{:.0} K  S={:.2}{}",
                            brg.temperature, brg.sommerfeld, status,
                        )).small().monospace());
                    });
                }

                ui.add_space(6.0);
                ui.collapsing(egui::RichText::new("Edit Damage").strong(), |ui| {
                    ui.label(egui::RichText::new("Cylinders").small().strong());
                    ui.add_space(2.0);
                    for i in 0..core.num_cyl() {
                        ui.label(egui::RichText::new(format!("Cyl {}", i + 1)).small().strong());
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Wall").small());
                            ui.add(egui::Slider::new(&mut core.cylinders[i].wall_wear, 0.0..=1.0)
                                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
                        });
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Ring").small());
                            ui.add(egui::Slider::new(&mut core.cylinders[i].ring_wear, 0.0..=1.0)
                                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
                        });
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Rod ").small());
                            ui.add(egui::Slider::new(&mut core.cylinders[i].rod_damage, 0.0..=1.0)
                                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
                        });
                        if let Some(brg) = core.rod_bearings.get_mut(i) {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Brg ").small());
                                ui.add(egui::Slider::new(&mut brg.shell_wear, 0.0..=1.0)
                                    .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
                            });
                        }
                        ui.add_space(3.0);
                    }

                    ui.add_space(2.0);
                    ui.label(egui::RichText::new("Main Bearings").small().strong());
                    for i in 0..core.main_bearings.len() {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(format!("Main {}", i + 1)).small());
                            ui.add(egui::Slider::new(&mut core.main_bearings[i].shell_wear, 0.0..=1.0)
                                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
                        });
                    }

                    ui.add_space(2.0);
                    ui.label(egui::RichText::new("Cam Bearings").small().strong());
                    for i in 0..core.cam_bearings.len() {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(format!("Cam  {}", i + 1)).small());
                            ui.add(egui::Slider::new(&mut core.cam_bearings[i].shell_wear, 0.0..=1.0)
                                .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
                        });
                    }
                });
            });
        });

    // ══════════════════════════════════════════════════════════════════════════
    // BOTTOM PANEL — Dyno Results Graph (hand-drawn with egui painter)
    // ══════════════════════════════════════════════════════════════════════════
    if !dyno.results.is_empty() {
        egui::TopBottomPanel::bottom("dyno_graph_panel")
            .resizable(true)
            .default_height(220.0)
            .height_range(140.0..=400.0)
            .show(ctx.ctx_mut(), |ui| {
                let title = if dyno.tested_engine_name.is_empty() {
                    "Dyno Results".to_string()
                } else {
                    format!("Dyno  \u{2014}  {}", dyno.tested_engine_name)
                };
                ui.horizontal(|ui| {
                    ui.heading(&title);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Clear").clicked() {
                            dyno.results.clear();
                            dyno.peak_hp = 0.0;
                            dyno.peak_torque = 0.0;
                        }
                        ui.label(egui::RichText::new(format!(
                            "Peak: {:.1} HP @ {:.0}  |  {:.1} Nm @ {:.0}",
                            dyno.peak_hp, dyno.peak_hp_rpm,
                            dyno.peak_torque, dyno.peak_torque_rpm,
                        )).monospace().small());
                    });
                });
                ui.separator();

                dyno_graph(ui, &dyno);
            });
    }
}

// ──────────────────────────────── Widgets ───────────────────────────────────
fn rpm_bar(ui: &mut egui::Ui, rpm: f32, redline: f32) {
    let bar_max = (redline * 1.15).max(1000.0);
    let frac = (rpm / bar_max).clamp(0.0, 1.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 14.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, egui::Color32::from_gray(40));
    let bar_color = if rpm > redline {
        egui::Color32::from_rgb(230, 70, 70)
    } else if rpm > redline - 1500.0 {
        egui::Color32::from_rgb(230, 180, 70)
    } else {
        egui::Color32::from_rgb(80, 180, 220)
    };
    let mut filled = rect;
    filled.set_width(rect.width() * frac);
    painter.rect_filled(filled, 3.0, bar_color);
    let redline_x = rect.left() + rect.width() * (redline / bar_max);
    painter.line_segment(
        [egui::pos2(redline_x, rect.top()), egui::pos2(redline_x, rect.bottom())],
        egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 80, 80)),
    );
}

fn telemetry_grid(ui: &mut egui::Ui, core: &EngineCore) {
    let map_kpa = core.map_smoothed / 1000.0;
    let exh_kpa = core.exhaust_pressure_smoothed / 1000.0;
    let kw = (core.power_smoothed / 1000.0).max(0.0);
    let nm = core.torque_smoothed.max(0.0);
    let afr = core.afr_smoothed;

    egui::Grid::new("telemetry").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
        cell(ui, "Torque",     &format!("{:>5.1} Nm", nm));
        cell(ui, "Power",      &format!("{:>5.1} kW",  kw));
        ui.end_row();
        cell(ui, "MAP",        &format!("{:>5.1} kPa", map_kpa));
        cell(ui, "Exh. press", &format!("{:>5.1} kPa", exh_kpa));
        ui.end_row();
        cell(ui, "AFR",        &format!("{:>5.2}", afr));
        cell(ui, "Exh. temp",  &format!("{:>5.0} K", core.exhaust_temp_smoothed));
        ui.end_row();
    });
}

fn cell(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).small());
        ui.label(egui::RichText::new(value).monospace());
    });
}

fn pressure_minibar(ui: &mut egui::Ui, pressure_ratio: f32, temp_k: f32, flash: f32, flame: &[f32; 3]) {
    let max_ratio = 80.0;
    let frac = (pressure_ratio / max_ratio).clamp(0.0, 1.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(180.0, 12.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, egui::Color32::from_gray(36));

    // Heat tint scales from blue (cool) to orange (hot)
    let hot = ((temp_k - 350.0) / 2200.0).clamp(0.0, 1.0);
    let r = (60.0 + 195.0 * hot) as u8;
    let g = (140.0 - 90.0 * hot) as u8;
    let b = (210.0 - 180.0 * hot) as u8;
    let mut filled = rect;
    filled.set_width(rect.width() * frac);
    painter.rect_filled(filled, 2.0, egui::Color32::from_rgb(r, g, b));

    if flash > 0.05 {
        // Flash overlay tinted by fuel flame colour
        let a = (flash * 230.0).clamp(0.0, 255.0) as u8;
        painter.rect_filled(filled, 2.0, egui::Color32::from_rgba_unmultiplied(
            (flame[0] * 255.0) as u8, (flame[1] * 255.0) as u8, (flame[2] * 255.0) as u8, a,
        ));
    }
    ui.label(egui::RichText::new(format!("{:>5.1} bar  {:>4.0} K", pressure_ratio, temp_k)).monospace().small());
}

fn draw_render_tree(
    ui: &mut egui::Ui,
    entity: Entity,
    query: &mut Query<(Entity, &Name, Option<&Children>, Option<&Parent>, &mut Visibility), With<EngineVisual>>,
) {
    // 1. Collect info and current state, then drop the borrow immediately
    let (name, is_visible, child_ids) = {
        let Ok((_, name, children, _, vis)) = query.get_mut(entity) else { return; };
        (
            name.to_string(),
            matches!(*vis, Visibility::Inherited | Visibility::Visible),
            children.map(|c| c.iter().cloned().collect::<Vec<_>>()).unwrap_or_default(),
        )
    };

    let mut new_visible = is_visible;

    ui.horizontal(|ui| {
        if ui.checkbox(&mut new_visible, "").changed() {
            if let Ok((_, _, _, _, mut vis)) = query.get_mut(entity) {
                *vis = if new_visible { Visibility::Inherited } else { Visibility::Hidden };
            }
        }

        if !child_ids.is_empty() {
            let id = ui.make_persistent_id(entity);
            let state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
            
            // We must wrap the collapsing header in a vertical layout because
            // its body (indentation) only works in vertical layouts.
            ui.vertical(|ui| {
                state.show_header(ui, |ui| {
                    ui.label(&name);
                }).body(|ui| {
                    for child in child_ids {
                        draw_render_tree(ui, child, query);
                    }
                });
            });
        } else {
            ui.label(&name);
        }
    });
}

// ──────────────── Per-part material picker ──────────────────────────────────
fn material_selector(ui: &mut egui::Ui, label: &str, current: &mut PhysMaterial) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("{:<13}", label)).small().monospace());
        let combo_id = format!("mat_{}", label);
        egui::ComboBox::from_id_salt(combo_id)
            .width(150.0)
            .selected_text(current.name)
            .show_ui(ui, |ui| {
                for m in MATERIAL_CATALOG {
                    let selected = current.name == m.name;
                    if ui.selectable_label(selected, m.name).clicked() {
                        *current = (*m).clone();
                    }
                }
            });
    });
}

// ──────────────── 0..1 damage bar in the FEA gradient ───────────────────────
fn wear_minibar(ui: &mut egui::Ui, tag: &str, value: f32) {
    let v = value.clamp(0.0, 1.0);
    ui.label(egui::RichText::new(tag).small().monospace());
    let (rect, _) = ui.allocate_exact_size(egui::vec2(48.0, 10.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, egui::Color32::from_gray(36));
    let mut filled = rect;
    filled.set_width(rect.width() * v);
    let (r, g, b) = jet_egui(v);
    painter.rect_filled(filled, 2.0, egui::Color32::from_rgb(r, g, b));
}

fn jet_egui(t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    let r = (1.5 - (4.0 * t - 3.0).abs()).clamp(0.0, 1.0);
    let g = (1.5 - (4.0 * t - 2.0).abs()).clamp(0.0, 1.0);
    let b = (1.5 - (4.0 * t - 1.0).abs()).clamp(0.0, 1.0);
    ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

// ──────────────────────── Dyno graph (painter-based) ────────────────────────
fn dyno_graph(ui: &mut egui::Ui, dyno: &DynoState) {
    let available = ui.available_size();
    let (rect, _) = ui.allocate_exact_size(available, egui::Sense::hover());
    let painter = ui.painter_at(rect);

    // Margins for axis labels
    let margin_left = 55.0;
    let margin_right = 55.0;
    let margin_top = 8.0;
    let margin_bottom = 22.0;

    let plot_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + margin_left, rect.top() + margin_top),
        egui::pos2(rect.right() - margin_right, rect.bottom() - margin_bottom),
    );
    if plot_rect.width() < 40.0 || plot_rect.height() < 30.0 {
        return;
    }

    // Background
    painter.rect_filled(plot_rect, 4.0, egui::Color32::from_gray(22));
    painter.rect_stroke(plot_rect, 4.0, egui::Stroke::new(1.0, egui::Color32::from_gray(50)));

    // Data ranges
    let rpm_min = dyno.results.iter().map(|s| s.rpm).fold(f32::INFINITY, f32::min);
    let rpm_max = dyno.results.iter().map(|s| s.rpm).fold(f32::NEG_INFINITY, f32::max);
    let max_torque = dyno.results.iter().map(|s| s.torque_nm).fold(0.0_f32, f32::max);
    let max_hp = dyno.results.iter().map(|s| s.power_hp).fold(0.0_f32, f32::max);
    let y_max = max_torque.max(max_hp).max(10.0) * 1.15; // 15% headroom

    let rpm_range = (rpm_max - rpm_min).max(100.0);

    // Map data to pixel coordinates
    let to_x = |rpm: f32| -> f32 {
        plot_rect.left() + (rpm - rpm_min) / rpm_range * plot_rect.width()
    };
    let to_y = |val: f32| -> f32 {
        plot_rect.bottom() - (val / y_max) * plot_rect.height()
    };

    // Grid lines — horizontal
    let grid_color = egui::Color32::from_gray(38);
    let label_color = egui::Color32::from_gray(120);
    let font = egui::FontId::monospace(10.0);

    let y_step = nice_step(y_max, 5);
    let mut y_val = 0.0;
    while y_val <= y_max {
        let py = to_y(y_val);
        if py >= plot_rect.top() && py <= plot_rect.bottom() {
            painter.line_segment(
                [egui::pos2(plot_rect.left(), py), egui::pos2(plot_rect.right(), py)],
                egui::Stroke::new(0.5, grid_color),
            );
            // Left label (torque)
            painter.text(
                egui::pos2(plot_rect.left() - 4.0, py),
                egui::Align2::RIGHT_CENTER,
                format!("{:.0}", y_val),
                font.clone(),
                egui::Color32::from_rgb(100, 180, 255),
            );
            // Right label (HP, same scale for simplicity)
            painter.text(
                egui::pos2(plot_rect.right() + 4.0, py),
                egui::Align2::LEFT_CENTER,
                format!("{:.0}", y_val),
                font.clone(),
                egui::Color32::from_rgb(255, 100, 100),
            );
        }
        y_val += y_step;
    }

    // Grid lines — vertical (RPM)
    let x_step = nice_step(rpm_range, 6);
    let mut x_val = (rpm_min / x_step).ceil() * x_step;
    while x_val <= rpm_max {
        let px = to_x(x_val);
        if px >= plot_rect.left() && px <= plot_rect.right() {
            painter.line_segment(
                [egui::pos2(px, plot_rect.top()), egui::pos2(px, plot_rect.bottom())],
                egui::Stroke::new(0.5, grid_color),
            );
            painter.text(
                egui::pos2(px, plot_rect.bottom() + 3.0),
                egui::Align2::CENTER_TOP,
                format!("{:.0}", x_val),
                font.clone(),
                label_color,
            );
        }
        x_val += x_step;
    }

    // Axis titles
    painter.text(
        egui::pos2(plot_rect.left() - 4.0, plot_rect.top() - 2.0),
        egui::Align2::RIGHT_BOTTOM,
        "Nm",
        egui::FontId::proportional(10.0),
        egui::Color32::from_rgb(100, 180, 255),
    );
    painter.text(
        egui::pos2(plot_rect.right() + 4.0, plot_rect.top() - 2.0),
        egui::Align2::LEFT_BOTTOM,
        "HP",
        egui::FontId::proportional(10.0),
        egui::Color32::from_rgb(255, 100, 100),
    );
    painter.text(
        egui::pos2(plot_rect.center().x, plot_rect.bottom() + 12.0),
        egui::Align2::CENTER_TOP,
        "RPM",
        egui::FontId::proportional(10.0),
        label_color,
    );

    // Draw torque curve
    let torque_color = egui::Color32::from_rgb(100, 180, 255);
    draw_curve(&painter, &dyno.results, |s| s.rpm, |s| s.torque_nm, to_x, to_y, torque_color, 2.5);

    // Draw HP curve
    let hp_color = egui::Color32::from_rgb(255, 100, 100);
    draw_curve(&painter, &dyno.results, |s| s.rpm, |s| s.power_hp, to_x, to_y, hp_color, 2.5);

    // Peak markers
    if dyno.peak_torque > 0.0 {
        let cx = to_x(dyno.peak_torque_rpm);
        let cy = to_y(dyno.peak_torque);
        painter.circle_filled(egui::pos2(cx, cy), 5.0, torque_color);
        painter.circle_stroke(egui::pos2(cx, cy), 5.0, egui::Stroke::new(1.5, egui::Color32::WHITE));
    }
    if dyno.peak_hp > 0.0 {
        let cx = to_x(dyno.peak_hp_rpm);
        let cy = to_y(dyno.peak_hp);
        painter.circle_filled(egui::pos2(cx, cy), 5.0, hp_color);
        painter.circle_stroke(egui::pos2(cx, cy), 5.0, egui::Stroke::new(1.5, egui::Color32::WHITE));
    }

    // Legend (top-right of plot area)
    let legend_x = plot_rect.right() - 100.0;
    let legend_y = plot_rect.top() + 8.0;
    painter.line_segment(
        [egui::pos2(legend_x, legend_y + 5.0), egui::pos2(legend_x + 20.0, legend_y + 5.0)],
        egui::Stroke::new(2.5, torque_color),
    );
    painter.text(
        egui::pos2(legend_x + 24.0, legend_y + 5.0),
        egui::Align2::LEFT_CENTER,
        "Torque (Nm)",
        egui::FontId::proportional(10.0),
        torque_color,
    );
    painter.line_segment(
        [egui::pos2(legend_x, legend_y + 20.0), egui::pos2(legend_x + 20.0, legend_y + 20.0)],
        egui::Stroke::new(2.5, hp_color),
    );
    painter.text(
        egui::pos2(legend_x + 24.0, legend_y + 20.0),
        egui::Align2::LEFT_CENTER,
        "Power (HP)",
        egui::FontId::proportional(10.0),
        hp_color,
    );
}

/// Draw a polyline curve through data points.
fn draw_curve(
    painter: &egui::Painter,
    samples: &[crate::engine::DynoSample],
    x_fn: impl Fn(&crate::engine::DynoSample) -> f32,
    y_fn: impl Fn(&crate::engine::DynoSample) -> f32,
    to_x: impl Fn(f32) -> f32,
    to_y: impl Fn(f32) -> f32,
    color: egui::Color32,
    width: f32,
) {
    if samples.len() < 2 { return; }
    let points: Vec<egui::Pos2> = samples.iter()
        .map(|s| egui::pos2(to_x(x_fn(s)), to_y(y_fn(s))))
        .collect();
    for w in points.windows(2) {
        painter.line_segment([w[0], w[1]], egui::Stroke::new(width, color));
    }
}

/// Choose a "nice" step value for grid lines given a data range and desired line count.
fn nice_step(range: f32, target_lines: usize) -> f32 {
    let raw = range / target_lines as f32;
    let mag = 10.0_f32.powf(raw.log10().floor());
    let norm = raw / mag;
    let nice = if norm <= 1.5 { 1.0 }
        else if norm <= 3.5 { 2.0 }
        else if norm <= 7.5 { 5.0 }
        else { 10.0 };
    (nice * mag).max(1.0)
}
