//! P1 single-pass input routing (UI_BUG_AUDIT P1, kills F1.1/F1.3/F1.4).
//!
//! The old pipeline read `ctx.input()` in ~7 places and mutated the shared
//! event queue with `events.retain` in 4 of them (`handle_alt_menu` x2,
//! end-of-frame arrow strip, `handle_tab` remove). Correctness depended on
//! call order — see AGENTS.md §11 (`58265b3` regression).
//!
//! The new pipeline:
//! 1. **Capture**: `handle_keyboard_input` snapshots events + modifiers +
//!    focus state in ONE `ctx.input()` call.
//! 2. **Route**: [`route_intents`] assigns each event an [`IntentOwner`] in
//!    one place (pure function, unit-tested, no `ctx`).
//! 3. **Dispatch**: handlers consume routed intents; they never touch the
//!    event queue.
//! 4. **Strip**: ONE `ctx.input_mut().retain()` at end-of-frame removes the
//!    consumed key events so egui widgets don't double-handle them. Text
//!    events are never stripped (widgets still need them for text fields).
//!
//! [`MenuNavState`] folds the 5 scattered Alt-menu fields (§24) into one
//! struct with its own `handle_event` method.

use eframe::egui;

/// Number of top-level menus (File, Edit, View, Audio, Help). See §24.
pub(crate) const NUM_MENUS: usize = 5;

/// Map a key to a top-level menu index. `None` for non-menu keys.
pub(crate) fn menu_index_for_key(key: egui::Key) -> Option<usize> {
    match key {
        egui::Key::F => Some(0),
        egui::Key::E => Some(1),
        egui::Key::V => Some(2),
        egui::Key::A => Some(3),
        egui::Key::H => Some(4),
        _ => None,
    }
}

/// Alt-menu keyboard-navigation state (§24). Replaces the 5 scattered
/// `HtrkApp` fields (`menu_bar_active`, `active_menu`, `force_open_menu`,
/// `alt_prev_frame`, `alt_intercepted`).
#[derive(Debug, Clone, Default)]
pub(crate) struct MenuNavState {
    /// Menu bar in keyboard-nav mode (toggled by Alt tap).
    pub bar_active: bool,
    /// Highlighted top-level menu index.
    pub active_menu: usize,
    /// One-shot force-open, consumed via [`MenuNavState::take_force_open`].
    pub force_open: Option<usize>,
    /// Previous frame's Alt state (press/release edge detection).
    pub alt_prev: bool,
    /// True when a key was pressed while Alt held (release is not a tap).
    pub intercepted: bool,
}

/// Outcome of feeding one key event to [`MenuNavState::handle_key`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuOutcome {
    /// Not a menu key; caller should route normally.
    Ignored,
    /// Menu consumed the key; strip it from the egui queue.
    Consumed,
}

impl MenuNavState {
    /// One-shot: take the pending force-open menu index.
    pub fn take_force_open(&mut self) -> Option<usize> {
        self.force_open.take()
    }

    /// Feed one pressed-key event (with its per-event modifiers) through
    /// the Alt-menu state machine. Pure w.r.t. egui — needs only the
    /// frame-level `alt_now` and `popup_open` snapshots.
    ///
    /// Covers all three §24 behaviours: Alt+letter shortcuts, Alt-tap
    /// interception marking, and menu-bar-active navigation.
    /// Alt press/release edge transitions go through [`MenuNavState::press_edge`]
    /// (before routing) and [`MenuNavState::release_edge`] (after).
    pub fn handle_key(
        &mut self,
        key: egui::Key,
        modifiers: egui::Modifiers,
        any_dialog_open: bool,
        popup_open: bool,
    ) -> MenuOutcome {
        // Alt+letter shortcuts (work in any view/mode, gated on no dialog).
        // Per-event modifiers (not frame-level alt_now): eguidev script
        // injection sets per-event modifiers only.
        if !any_dialog_open
            && modifiers.alt
            && !modifiers.ctrl
            && !modifiers.shift
        {
            if let Some(idx) = menu_index_for_key(key) {
                self.bar_active = true;
                self.active_menu = idx;
                self.force_open = Some(idx);
                self.intercepted = true;
                return MenuOutcome::Consumed;
            }
        }
        // Menu-bar-active navigation (only when no popup is open).
        if self.bar_active && !any_dialog_open && !popup_open && !modifiers.any() {
            match key {
                egui::Key::ArrowLeft => {
                    self.active_menu = (self.active_menu + NUM_MENUS - 1) % NUM_MENUS;
                    return MenuOutcome::Consumed;
                }
                egui::Key::ArrowRight => {
                    self.active_menu = (self.active_menu + 1) % NUM_MENUS;
                    return MenuOutcome::Consumed;
                }
                egui::Key::ArrowDown | egui::Key::Enter => {
                    self.force_open = Some(self.active_menu);
                    return MenuOutcome::Consumed;
                }
                egui::Key::Escape => {
                    self.bar_active = false;
                    return MenuOutcome::Consumed;
                }
                _ => {
                    if let Some(idx) = menu_index_for_key(key) {
                        self.active_menu = idx;
                        self.force_open = Some(idx);
                        return MenuOutcome::Consumed;
                    }
                }
            }
        }
        MenuOutcome::Ignored
    }

    /// Per-frame Alt press edge: reset the tap interceptor when Alt goes
    /// down. Call BEFORE routing this frame's events (routing marks
    /// `intercepted` for keys pressed while Alt is held).
    pub fn press_edge(&mut self, alt_now: bool) {
        if alt_now && !self.alt_prev {
            self.intercepted = false;
        }
    }

    /// Per-frame Alt release edge: a release with no intervening key is a
    /// tap and toggles the menu bar. Call AFTER routing so this frame's
    /// interceptor marks are visible. Also advances `alt_prev`.
    pub fn release_edge(&mut self, alt_now: bool, any_dialog_open: bool) {
        if !alt_now && self.alt_prev && !self.intercepted && !any_dialog_open {
            self.bar_active = !self.bar_active;
            if self.bar_active {
                self.active_menu = 0;
            }
        }
        self.alt_prev = alt_now;
    }

    /// Mark that a key was pressed while Alt was held (tap interceptor).
    /// The old code set this from a second `ctx.input()` scan; the router
    /// now sets it while routing (single pass).
    pub fn mark_alt_used(&mut self) {
        self.intercepted = true;
    }
}

/// A single captured input event, decoupled from egui's queue.
#[derive(Debug, Clone)]
pub(crate) enum InputIntent {
    /// Text input (note preview + cell entry + widget text).
    Text(char),
    /// Pressed key with its per-event modifiers.
    Key {
        key: egui::Key,
        modifiers: egui::Modifiers,
    },
}

/// Who owns a routed intent. Decided in ONE place ([`route_intents`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IntentOwner {
    /// Menu accelerator / menu-nav key (stripped from egui queue).
    Menu,
    /// Tab channel-steal in the pattern editor (stripped, pre-focus-gate).
    Tab {
        shift: bool,
    },
    /// Text: always dispatched to preview/edit AND left in the queue for
    /// widgets (never stripped).
    Text,
    /// A widget has focus or a dialog owns the key: leave it alone.
    Widget,
    /// Normal app shortcut / editor key.
    App,
}

/// A routed intent: what happened, who owns it, and which captured event
/// it came from (`source_idx`, used by the single end-of-frame strip).
#[derive(Debug, Clone)]
pub(crate) struct RoutedIntent {
    pub intent: InputIntent,
    pub owner: IntentOwner,
    /// Index into the captured event snapshot (key events only matter
    /// for stripping; text events use `usize::MAX`).
    pub source_idx: usize,
}

/// Snapshot of everything `handle_keyboard_input` needs from egui,
/// captured in ONE `ctx.input()` call.
#[derive(Debug, Clone, Default)]
pub(crate) struct InputSnapshot {
    /// Cloned raw events (order-preserved).
    pub events: Vec<egui::Event>,
    /// Frame-level Alt state (tap edge detection).
    pub alt_now: bool,
    /// Whether an egui widget has keyboard focus.
    pub has_focus: bool,
    /// Whether any egui popup is open (menu-nav gating).
    pub popup_open: bool,
}

/// Context flags that are NOT part of the egui snapshot (computed outside
/// `ctx.input()`, per §11 — never inside).
#[derive(Debug, Clone, Copy)]
pub(crate) struct RouteFlags {
    pub any_dialog_open: bool,
    pub is_pattern: bool,
    pub edit_mode: bool,
}

/// Route a snapshot into owned intents. Pure function (no `ctx`) so it is
/// unit-testable. Preserves the exact priority of the old 6-stage chain:
///
/// 1. `Event::Text` → [`IntentOwner::Text`] (always, pre-focus-gate, §11).
/// 2. Alt+letter / menu-nav keys → [`IntentOwner::Menu`] (pre-gate, §24).
/// 3. Tab in pattern+edit mode → [`IntentOwner::Tab`] (pre-gate steal).
/// 4. Focus gate: remaining keys with a focused widget → [`IntentOwner::Widget`].
/// 5. Everything else → [`IntentOwner::App`] (per-key `!any_dialog_open`
///    guards in the dispatch handlers still apply).
///
/// Side effect: marks `menu.intercepted` when any key is pressed while Alt
/// is held (tap interceptor, previously a separate `ctx.input()` scan),
/// and runs the Alt tap edge transition via [`MenuNavState::frame`].
pub(crate) fn route_intents(
    snap: &InputSnapshot,
    flags: RouteFlags,
    menu: &mut MenuNavState,
) -> Vec<RoutedIntent> {
    let mut out = Vec::with_capacity(snap.events.len());
    // First Tab/Shift-Tab wins (old code took the first match and removed
    // only that one event).
    let mut tab_taken = false;

    // Press edge BEFORE routing: routing marks `intercepted` for keys
    // pressed while Alt is held, and must not be wiped after.
    menu.press_edge(snap.alt_now);

    for (idx, event) in snap.events.iter().enumerate() {
        match event {
            egui::Event::Text(text) => {
                for ch in text.chars() {
                    out.push(RoutedIntent {
                        intent: InputIntent::Text(ch),
                        owner: IntentOwner::Text,
                        source_idx: usize::MAX,
                    });
                }
            }
            egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => {
                // Tap interceptor: any key press while Alt held (old code
                // scanned the queue a second time for this).
                if snap.alt_now {
                    menu.mark_alt_used();
                }
                // 2. Menu keys (pre-focus-gate, §24).
                if menu.handle_key(*key, *modifiers, flags.any_dialog_open, snap.popup_open)
                    == MenuOutcome::Consumed
                {
                    out.push(RoutedIntent {
                        intent: InputIntent::Key {
                            key: *key,
                            modifiers: *modifiers,
                        },
                        owner: IntentOwner::Menu,
                        source_idx: idx,
                    });
                    continue;
                }
                // 3. Tab steal (pre-focus-gate; pattern + edit mode only).
                if !tab_taken
                    && matches!(key, egui::Key::Tab)
                    && flags.is_pattern
                    && !flags.any_dialog_open
                    && flags.edit_mode
                    && (!modifiers.any() || modifiers.shift_only())
                {
                    out.push(RoutedIntent {
                        intent: InputIntent::Key {
                            key: *key,
                            modifiers: *modifiers,
                        },
                        owner: IntentOwner::Tab {
                            shift: modifiers.shift_only(),
                        },
                        source_idx: idx,
                    });
                    tab_taken = true;
                    continue;
                }
                // 4. Focus gate.
                if snap.has_focus {
                    out.push(RoutedIntent {
                        intent: InputIntent::Key {
                            key: *key,
                            modifiers: *modifiers,
                        },
                        owner: IntentOwner::Widget,
                        source_idx: idx,
                    });
                    continue;
                }
                // 5. Normal app key.
                out.push(RoutedIntent {
                    intent: InputIntent::Key {
                        key: *key,
                        modifiers: *modifiers,
                    },
                    owner: IntentOwner::App,
                    source_idx: idx,
                });
            }
            _ => {}
        }
    }

    // Release edge AFTER routing so the interceptor marks from this
    // frame's keys are visible (a release with no intervening key is a tap).
    menu.release_edge(snap.alt_now, flags.any_dialog_open);

    out
}

/// Should the end-of-frame strip remove this leftover key event?
/// Preserves the old behaviour exactly: in the pattern view with no dialog
/// and no focused widget, remaining Tab/Arrow presses are stripped so
/// egui's widget system doesn't also interpret them as focus-navigation.
/// Must run AFTER dispatch (arrows move the cursor first) — §11 rule.
pub(crate) fn strip_leftover_key(key: egui::Key, flags: RouteFlags, has_focus: bool) -> bool {
    if has_focus || flags.any_dialog_open || !flags.is_pattern {
        return false;
    }
    matches!(
        key,
        egui::Key::Tab
            | egui::Key::ArrowUp
            | egui::Key::ArrowDown
            | egui::Key::ArrowLeft
            | egui::Key::ArrowRight
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_event(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn snap_with(events: Vec<egui::Event>) -> InputSnapshot {
        InputSnapshot {
            events,
            ..Default::default()
        }
    }

    fn flags() -> RouteFlags {
        RouteFlags {
            any_dialog_open: false,
            is_pattern: true,
            edit_mode: true,
        }
    }

    #[test]
    fn text_routes_to_text_owner() {
        let mut menu = MenuNavState::default();
        let snap = snap_with(vec![egui::Event::Text("q".into())]);
        let routed = route_intents(&snap, flags(), &mut menu);
        assert_eq!(routed.len(), 1);
        assert_eq!(routed[0].owner, IntentOwner::Text);
        assert!(matches!(routed[0].intent, InputIntent::Text('q')));
    }

    #[test]
    fn alt_letter_routes_to_menu() {
        let mut menu = MenuNavState::default();
        let snap = snap_with(vec![key_event(
            egui::Key::F,
            egui::Modifiers {
                alt: true,
                ..Default::default()
            },
        )]);
        let routed = route_intents(&snap, flags(), &mut menu);
        assert_eq!(routed[0].owner, IntentOwner::Menu);
        assert!(menu.bar_active);
        assert_eq!(menu.active_menu, 0);
        assert_eq!(menu.force_open, Some(0));
    }

    #[test]
    fn alt_letter_blocked_when_dialog_open() {
        let mut menu = MenuNavState::default();
        let snap = snap_with(vec![key_event(
            egui::Key::F,
            egui::Modifiers {
                alt: true,
                ..Default::default()
            },
        )]);
        let f = RouteFlags {
            any_dialog_open: true,
            ..flags()
        };
        let routed = route_intents(&snap, f, &mut menu);
        assert_eq!(routed[0].owner, IntentOwner::App);
        assert!(!menu.bar_active);
    }

    #[test]
    fn menu_nav_arrows_consumed_when_bar_active() {
        let mut menu = MenuNavState {
            bar_active: true,
            ..Default::default()
        };
        let snap = snap_with(vec![key_event(egui::Key::ArrowRight, egui::Modifiers::default())]);
        let routed = route_intents(&snap, flags(), &mut menu);
        assert_eq!(routed[0].owner, IntentOwner::Menu);
        assert_eq!(menu.active_menu, 1);
    }

    #[test]
    fn tab_steals_pre_focus_gate() {
        let mut menu = MenuNavState::default();
        let snap = InputSnapshot {
            events: vec![key_event(egui::Key::Tab, egui::Modifiers::default())],
            has_focus: true, // Tab still steals even with a focused widget.
            ..Default::default()
        };
        let routed = route_intents(&snap, flags(), &mut menu);
        assert_eq!(routed[0].owner, IntentOwner::Tab { shift: false });
    }

    #[test]
    fn tab_ignored_outside_pattern_edit() {
        let mut menu = MenuNavState::default();
        let snap = snap_with(vec![key_event(egui::Key::Tab, egui::Modifiers::default())]);
        let f = RouteFlags {
            is_pattern: false,
            ..flags()
        };
        let routed = route_intents(&snap, f, &mut menu);
        assert_eq!(routed[0].owner, IntentOwner::App);
    }

    #[test]
    fn focus_gate_parks_keys_on_widget() {
        let mut menu = MenuNavState::default();
        let snap = InputSnapshot {
            events: vec![key_event(egui::Key::ArrowDown, egui::Modifiers::default())],
            has_focus: true,
            ..Default::default()
        };
        let routed = route_intents(&snap, flags(), &mut menu);
        assert_eq!(routed[0].owner, IntentOwner::Widget);
    }

    #[test]
    fn text_still_dispatched_with_focus() {
        let mut menu = MenuNavState::default();
        let snap = InputSnapshot {
            events: vec![egui::Event::Text("z".into())],
            has_focus: true,
            ..Default::default()
        };
        let routed = route_intents(&snap, flags(), &mut menu);
        // Text reaches the dispatcher (preview-only when focused, §11)
        // and is never stripped.
        assert_eq!(routed[0].owner, IntentOwner::Text);
    }

    #[test]
    fn alt_tap_toggles_menu_bar() {
        let mut menu = MenuNavState::default();
        // Press Alt (no keys): edge sets alt_prev, no toggle yet.
        let snap = InputSnapshot {
            alt_now: true,
            ..Default::default()
        };
        route_intents(&snap, flags(), &mut menu);
        assert!(!menu.bar_active);
        // Release Alt with no intervening key: tap toggles on.
        let snap = InputSnapshot {
            alt_now: false,
            ..Default::default()
        };
        route_intents(&snap, flags(), &mut menu);
        assert!(menu.bar_active);
        assert_eq!(menu.active_menu, 0);
    }

    #[test]
    fn alt_release_after_key_is_not_a_tap() {
        let mut menu = MenuNavState::default();
        let snap = InputSnapshot {
            events: vec![key_event(egui::Key::M, egui::Modifiers::default())],
            alt_now: true,
            ..Default::default()
        };
        route_intents(&snap, flags(), &mut menu);
        let snap = InputSnapshot {
            alt_now: false,
            ..Default::default()
        };
        route_intents(&snap, flags(), &mut menu);
        assert!(!menu.bar_active);
    }

    #[test]
    fn strip_leftover_only_in_pattern_no_dialog_no_focus() {
        let f = flags();
        assert!(strip_leftover_key(egui::Key::ArrowDown, f, false));
        assert!(strip_leftover_key(egui::Key::Tab, f, false));
        assert!(!strip_leftover_key(egui::Key::ArrowDown, f, true));
        assert!(!strip_leftover_key(
            egui::Key::F5,
            f,
            false,
        ));
        let dlg = RouteFlags {
            any_dialog_open: true,
            ..f
        };
        assert!(!strip_leftover_key(egui::Key::ArrowDown, dlg, false));
    }
}
