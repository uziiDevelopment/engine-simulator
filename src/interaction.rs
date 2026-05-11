//! Part interaction system — hover highlight, grab cursor, and drag-to-drive physics.
//!
//! ## How it works
//!
//! `bevy_mod_picking` raycasts every frame against all mesh entities and populates
//! the [`HoverMap`] resource.  We read that alongside [`Pointer<Drag>`] events to:
//!
//! 1. **Highlight** — any `EngineVisual` entity that is hovered gets a bright emissive
//!    rim tint applied to its [`StandardMaterial`] (or its GLB-descendant materials).
//!    The tint is removed as soon as the entity leaves hover.
//!
//! 2. **Cursor icon** — while any engine part is hovered the window cursor switches to
//!    the OS grab hand; while actively dragging it switches to the closed grabbing hand.
//!    The orbit camera suppresses its own drag handling while we are interacting.
//!
//! 3. **Drag physics** — LMB drag maps the 2-D screen delta to a physics impulse on
//!    [`EngineCore`].  The mapping depends on *which kind of part* was grabbed:
//!
//!    | Part              | Effect                                                |
//!    |-------------------|-------------------------------------------------------|
//!    | `Crankshaft` / `Flywheel` / `Clutch` | Torque impulse on `omega` (spin)  |
//!    | `Piston`          | Translational force along the cylinder axis           |
//!    | `ConRod`          | Same translational force as the piston it connects to |
//!    | Anything else     | Generic rotational impulse on `omega`                 |
//!
//! The impulse is intentionally large enough to be satisfying but capped so you
//! cannot instantly explode the engine.

use bevy::prelude::*;
use bevy::window::{CursorIcon, PrimaryWindow};
use bevy_egui::EguiContexts;
use bevy_mod_picking::prelude::*;
use bevy_mod_picking::focus::HoverMap;

use crate::engine::EngineCore;
use crate::engine::gearbox::{
    constrain_lever, gate_for_position, snapped_lever_for, GearSelector,
};
use crate::engine::try_select_gear;
use crate::visuals::{
    ConRod, Crankshaft, Clutch, EngineVisual, Flywheel, PedalControl, PedalKind,
    Piston, ShiftKnob,
};

pub struct InteractionPlugin;

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app
            // bevy_mod_picking core + mesh raycast backend
            .add_plugins(DefaultPickingPlugins)
            // Our systems
            .insert_resource(InteractionState::default())
            .add_systems(
                Update,
                (
                    handle_drag_events,
                    apply_hover_highlight,
                    update_cursor_icon,
                )
                    .chain(),
            );
    }
}

// ── Interaction state ──────────────────────────────────────────────────────────

/// Tracks which entity the player is actively dragging and accumulated drag.
#[derive(Resource, Default)]
pub struct InteractionState {
    /// Entity currently being dragged (LMB).
    pub dragged: Option<Entity>,
    /// Whether the cursor is over any engine part this frame.
    pub hovering: bool,
}

// ── Part classification ────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum PartKind {
    Rotating,   // Crank, flywheel, clutch — applies torque to omega
    Translating(usize), // Piston/rod idx — applies force along cylinder axis
    Pedal(PedalKind),
    ShiftKnob,
    Generic,
}

fn classify(
    entity: Entity,
    q_crank: &Query<(), With<Crankshaft>>,
    q_flywheel: &Query<(), With<Flywheel>>,
    q_clutch: &Query<(), With<Clutch>>,
    q_piston: &Query<&Piston>,
    q_rod: &Query<&ConRod>,
    q_pedal: &Query<&PedalControl>,
    q_knob: &Query<(), With<ShiftKnob>>,
    q_parent: &Query<&Parent>,
) -> PartKind {
    // Walk up hierarchy to find a tagged ancestor
    let mut cur = entity;
    for _ in 0..8 {
        if q_knob.get(cur).is_ok() {
            return PartKind::ShiftKnob;
        }
        if let Ok(p) = q_pedal.get(cur) {
            return PartKind::Pedal(p.kind);
        }
        if q_crank.get(cur).is_ok() || q_flywheel.get(cur).is_ok() || q_clutch.get(cur).is_ok() {
            return PartKind::Rotating;
        }
        if let Ok(p) = q_piston.get(cur) { return PartKind::Translating(p.idx); }
        if let Ok(r) = q_rod.get(cur)    { return PartKind::Translating(r.idx); }
        match q_parent.get(cur) {
            Ok(parent) => cur = parent.get(),
            Err(_) => break,
        }
    }
    PartKind::Generic
}

// ── Drag event handling ────────────────────────────────────────────────────────

fn handle_drag_events(
    mut evr_drag_start: EventReader<Pointer<DragStart>>,
    mut evr_drag:       EventReader<Pointer<Drag>>,
    mut evr_drag_end:   EventReader<Pointer<DragEnd>>,
    mut state:          ResMut<InteractionState>,
    mut core:           ResMut<EngineCore>,
    q_engine_vis:       Query<(), With<EngineVisual>>,
    q_crank:    Query<(), With<Crankshaft>>,
    q_flywheel: Query<(), With<Flywheel>>,
    q_clutch:   Query<(), With<Clutch>>,
    q_piston:   Query<&Piston>,
    q_rod:      Query<&ConRod>,
    q_pedal:    Query<&PedalControl>,
    q_knob:     Query<(), With<ShiftKnob>>,
    q_parent:   Query<&Parent>,
    mut egui:   EguiContexts,
) {
    if egui.ctx_mut().is_pointer_over_area() { return; }

    for ev in evr_drag_start.read() {
        if ev.button != PointerButton::Primary { continue; }
        let target = ev.target;
        // Only engage if it's part of the engine visual hierarchy
        if q_engine_vis.get(target).is_ok() || ancestor_is_engine(&target, &q_engine_vis, &q_parent) {
            state.dragged = Some(target);
        }
    }

    for ev in evr_drag.read() {
        if ev.button != PointerButton::Primary { continue; }
        let Some(dragged) = state.dragged else { continue };
        if ev.target != dragged { continue; }

        let delta = ev.event.delta; // screen-space pixels

        let kind = classify(
            dragged, &q_crank, &q_flywheel, &q_clutch, &q_piston, &q_rod,
            &q_pedal, &q_knob, &q_parent,
        );

        match kind {
            PartKind::Rotating | PartKind::Generic => {
                // Horizontal drag → spin. 0.35 rad/s per pixel feels natural.
                let impulse = delta.x * 0.35;
                core.omega = (core.omega + impulse).clamp(-500.0, 5000.0);
            }
            PartKind::Translating(_cyl_idx) => {
                let impulse = delta.x * 0.35 - delta.y * 0.25;
                core.omega = (core.omega + impulse).clamp(-500.0, 5000.0);
            }
            PartKind::Pedal(kind) => {
                // Drag downward (positive screen-Y) depresses the pedal.
                // 0.006 per pixel → ~170 px from rest to fully pressed.
                let depress_delta = delta.y * 0.006;
                match kind {
                    PedalKind::Clutch => {
                        // Engagement inversely linked to depression
                        let new_engagement = (core.clutch_engagement - depress_delta).clamp(0.0, 1.0);
                        core.clutch_engagement = new_engagement;
                    }
                    PedalKind::Throttle => {
                        let new_throttle = (core.throttle + depress_delta).clamp(0.0, 1.0);
                        core.throttle = new_throttle;
                    }
                }
            }
            PartKind::ShiftKnob => {
                // Map 2-D screen drag into the H-pattern plane.
                // X → left/right gate selection. Y → forward/back within gate.
                // Negative screen-Y is "up" (forward gate).
                // ~50 px per H-pattern unit feels snappy without overshooting
                // gates on quick flicks.
                let dx = delta.x * 0.02;
                let dy = -delta.y * 0.02;
                let prev = core.gearbox.lever_pos;
                let raw = prev + Vec2::new(dx, dy);
                let lever = constrain_lever(raw, prev, core.gearbox.reverse_armed);

                // Reverse-arm latch: same gesture as the keyboard path.
                if matches!(core.gearbox.selector, GearSelector::Neutral)
                    && lever.y > 0.9 && lever.x.abs() < 0.3
                {
                    core.gearbox.reverse_armed = true;
                }

                core.gearbox.lever_pos = lever;
                let sel = gate_for_position(lever, core.gearbox.reverse_armed);
                if sel != core.gearbox.selector {
                    try_select_gear(&mut *core, sel);
                    if !matches!(sel, GearSelector::Neutral | GearSelector::Reverse) {
                        core.gearbox.reverse_armed = false;
                    }
                }
            }
        }
    }

    for ev in evr_drag_end.read() {
        if ev.button != PointerButton::Primary { continue; }
        if state.dragged == Some(ev.target) {
            state.dragged = None;
            // Snap lever back to its slotted gate position so the visible
            // lever reads the engaged gear when released.
            core.gearbox.lever_pos = snapped_lever_for(core.gearbox.selector);
        }
    }
}

fn ancestor_is_engine(
    entity: &Entity,
    q_engine: &Query<(), With<EngineVisual>>,
    q_parent: &Query<&Parent>,
) -> bool {
    let mut cur = *entity;
    for _ in 0..8 {
        match q_parent.get(cur) {
            Ok(p) => {
                cur = p.get();
                if q_engine.get(cur).is_ok() { return true; }
            }
            Err(_) => return false,
        }
    }
    false
}

// ── Hover highlight ────────────────────────────────────────────────────────────

/// Emissive tint added to hovered parts (subtle but clearly visible).
const HOVER_EMISSIVE: LinearRgba = LinearRgba::new(0.30, 0.55, 1.0, 1.0);
const DRAG_EMISSIVE:  LinearRgba = LinearRgba::new(0.80, 0.40, 0.10, 1.0);

/// Tracks which entities we tinted last frame so we can clear them.
#[derive(Resource, Default)]
struct TintedEntities(Vec<Entity>);

// Register the resource in Plugin::build:
// We add it lazily via init_resource in the system itself the first time.

fn apply_hover_highlight(
    hover_map:      Res<HoverMap>,
    mut state:      ResMut<InteractionState>,
    mut materials:  ResMut<Assets<StandardMaterial>>,
    q_mat:          Query<&Handle<StandardMaterial>>,
    q_children:     Query<&Children>,
    q_engine:       Query<(), With<EngineVisual>>,
    q_parent:       Query<&Parent>,
    mut tinted:     Local<Vec<Entity>>,
) {
    // Collect all hovered engine-vis entities from the primary pointer
    let mut hovered: Vec<Entity> = Vec::new();
    let primary = PointerId::Mouse;
    if let Some(hits) = hover_map.get(&primary) {
        for (&entity, _) in hits {
            if q_engine.get(entity).is_ok()
                || ancestor_is_engine(&entity, &q_engine, &q_parent)
            {
                hovered.push(entity);
            }
        }
    }
    state.hovering = !hovered.is_empty();

    // Clear old tints
    for entity in tinted.drain(..) {
        let emissive = if Some(entity) == state.dragged {
            DRAG_EMISSIVE
        } else {
            LinearRgba::BLACK
        };
        apply_emissive_to_entity(entity, emissive, &mut materials, &q_mat, &q_children);
    }

    // Apply new tints
    for entity in &hovered {
        let emissive = if Some(*entity) == state.dragged {
            DRAG_EMISSIVE
        } else {
            HOVER_EMISSIVE
        };
        apply_emissive_to_entity(*entity, emissive, &mut materials, &q_mat, &q_children);
        tinted.push(*entity);
    }

    // Also tint dragged entity even if not currently hovered (cursor moved off during drag)
    if let Some(dragged) = state.dragged {
        if !hovered.contains(&dragged) {
            apply_emissive_to_entity(dragged, DRAG_EMISSIVE, &mut materials, &q_mat, &q_children);
            tinted.push(dragged);
        }
    }
}

fn apply_emissive_to_entity(
    entity: Entity,
    emissive: LinearRgba,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    q_mat: &Query<&Handle<StandardMaterial>>,
    q_children: &Query<&Children>,
) {
    if let Ok(mat_handle) = q_mat.get(entity) {
        if let Some(mat) = materials.get_mut(mat_handle) {
            mat.emissive = emissive;
        }
    }
    // Also tint children (for GLB scene hierarchies)
    for child in q_children.iter_descendants(entity) {
        if let Ok(mat_handle) = q_mat.get(child) {
            if let Some(mat) = materials.get_mut(mat_handle) {
                mat.emissive = emissive;
            }
        }
    }
}

// ── Cursor icon ────────────────────────────────────────────────────────────────

fn update_cursor_icon(
    state: Res<InteractionState>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    let Ok(mut window) = windows.get_single_mut() else { return };
    window.cursor.icon = if state.dragged.is_some() {
        CursorIcon::Grabbing
    } else if state.hovering {
        CursorIcon::Grab
    } else {
        CursorIcon::Default
    };
}
