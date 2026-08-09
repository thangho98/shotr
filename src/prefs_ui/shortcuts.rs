//! Binding global capture hotkeys.
//!
//! What this window can honestly say is narrow, and the wording matters more
//! than the widgets. macOS publishes its own screenshot shortcuts and nothing
//! else, so a combination no other *application* is using looks identical to one
//! that is — and pressing a combination two receivers hold runs both, with no
//! error anywhere. Hence "macOS is not using this" and never "this key is free".
//!
//! Measurements: `plans/reports/260809-1151-macos-global-hotkeys.md`.

use eframe::egui;

use crate::app::theme;
use crate::hotkey::{self, Action, Hotkey, Mods};
use crate::i18n::{t, tf};
use crate::settings::Prefs;

/// What one row offers, decided away from the drawing code so it can be tested
/// without a display.
#[derive(Clone, PartialEq, Debug)]
pub enum RowState {
    /// Nothing bound yet; this is the neighbour worth offering.
    Unbound(Option<Hotkey>),
    /// Bound, and macOS is not using the same combination.
    Bound(Hotkey),
    /// Bound, but macOS holds it too — so one press does two things.
    Clashes(Hotkey),
}

pub fn row_state(bound: Option<&Hotkey>, system: &[Hotkey], suggestion: Option<Hotkey>) -> RowState {
    match bound {
        None => RowState::Unbound(suggestion),
        Some(hotkey) if system.contains(hotkey) => RowState::Clashes(hotkey.clone()),
        Some(hotkey) => RowState::Bound(hotkey.clone()),
    }
}

fn label(action: Action) -> &'static str {
    match action {
        Action::Region => t("Capture a region"),
        Action::Full => t("Capture every screen"),
        Action::RegionCopy => t("Copy a region"),
        Action::FullCopy => t("Copy every screen"),
        Action::Hub => t("Open recent shots"),
    }
}

#[derive(Default)]
pub struct State {
    /// The action whose next keystroke is being read.
    recording: Option<Action>,
    /// Apple's enabled screenshot shortcuts. Re-read when the window comes
    /// back, because freeing one in System Settings is the whole point.
    system: Vec<Hotkey>,
    loaded: bool,
    was_focused: bool,
}

impl State {
    /// Re-read Apple's shortcuts when the window regains focus.
    ///
    /// Not on a timer: this changes when the user goes to System Settings and
    /// comes back, and a poll would spawn `defaults` forever to catch something
    /// that happens once.
    fn refresh_on_return(&mut self, ctx: &egui::Context) {
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        if !self.loaded || (focused && !self.was_focused) {
            self.system = hotkey::system_bindings();
            self.loaded = true;
        }
        self.was_focused = focused;
    }
}

pub fn ui(ui: &mut egui::Ui, prefs: &mut Prefs, state: &mut State) {
    if !hotkey::EDITABLE {
        unsupported(ui);
        return;
    }
    state.refresh_on_return(ui.ctx());

    if let Some(action) = state.recording {
        match read_key(ui.ctx()) {
            Recorded::Nothing => {}
            Recorded::Cancelled => state.recording = None,
            Recorded::Key(hotkey) => {
                state.recording = None;
                bind(prefs, action, Some(hotkey));
            }
        }
    }

    let live = hotkey::bindings(&prefs.hotkeys);

    // Worked out in one pass, and each offer joins the list the next row avoids:
    // several actions share a familiar combination — region and copy-a-region
    // both start from ⌘⇧4 — so offering per row in isolation proposes the same
    // keys twice, and accepting both binds one action over the other.
    let mut spoken_for: Vec<Hotkey> = live.iter().map(|(_, hotkey)| hotkey.clone()).collect();
    let mut rows = Vec::with_capacity(Action::ALL.len());
    for action in Action::ALL {
        let current = live
            .iter()
            .find(|(a, _)| *a == action)
            .map(|(_, hotkey)| hotkey.clone());
        let offer = match current {
            Some(_) => None,
            None => {
                let offer = hotkey::suggestion(action, &spoken_for, &state.system);
                if let Some(offer) = &offer {
                    spoken_for.push(offer.clone());
                }
                offer
            }
        };
        rows.push((action, row_state(current.as_ref(), &state.system, offer)));
    }

    for (action, state_for_row) in rows {
        row(ui, prefs, state, action, state_for_row);
        ui.add_space(6.0);
    }

    ui.add_space(12.0);
    ui.label(
        egui::RichText::new(t(
            "macOS cannot report shortcuts held by other apps. If one press does two things, choose another combination.",
        ))
        .weak()
        .small(),
    );

    ui.add_space(16.0);
    super::about::editor_keys(ui);
}

fn row(ui: &mut egui::Ui, prefs: &mut Prefs, state: &mut State, action: Action, row: RowState) {
    ui.horizontal(|ui| {
        ui.add_sized(egui::vec2(190.0, 18.0), egui::Label::new(label(action)));

        if state.recording == Some(action) {
            ui.label(egui::RichText::new(t("Press a combination…")).strong());
            if ui.button(t("Cancel")).clicked() {
                state.recording = None;
            }
            return;
        }

        match row.clone() {
            RowState::Unbound(None) => {
                ui.label(egui::RichText::new(t("Not set")).weak());
                if ui.button(t("Choose…")).clicked() {
                    state.recording = Some(action);
                }
            }
            RowState::Unbound(Some(offer)) => {
                if ui
                    .button(tf("Use {keys}", &[("keys", &offer.to_string())]))
                    .clicked()
                {
                    bind(prefs, action, Some(offer));
                }
                if ui.button(t("Choose…")).clicked() {
                    state.recording = Some(action);
                }
            }
            RowState::Bound(hotkey) | RowState::Clashes(hotkey) => {
                ui.add_sized(
                    egui::vec2(150.0, 18.0),
                    egui::Label::new(egui::RichText::new(hotkey.to_string()).monospace()),
                );
                if ui.button(t("Change…")).clicked() {
                    state.recording = Some(action);
                }
                if ui.button(t("Clear")).clicked() {
                    bind(prefs, action, None);
                }
            }
        }
    });

    if let RowState::Clashes(hotkey) = row {
        ui.horizontal(|ui| {
            ui.add_space(190.0);
            ui.label(
                egui::RichText::new(tf(
                    "macOS is using {keys} for its own screenshot. Both will run.",
                    &[("keys", &hotkey.to_string())],
                ))
                .small(),
            );
            if ui.button(t("Open System Settings")).clicked() {
                open_keyboard_settings();
            }
            // The link opens the shortcut list itself, but lands on whichever
            // category was open last — so the one step left is naming ours.
            ui.label(
                egui::RichText::new(t("then choose Screenshots"))
                    .weak()
                    .small(),
            );
        });
    }
}

/// Linux and Windows leave this to the desktop, which already does it well.
/// Showing a picker here would duplicate a facility the system provides, and on
/// Wayland it could not work at all.
fn unsupported(ui: &mut egui::Ui) {
    theme::section(ui, t("A shortcut for capturing"));
    ui.label(
        egui::RichText::new(t("Bind a system shortcut to: shotr --capture"))
            .weak()
            .small(),
    );
    ui.add_space(16.0);
    super::about::editor_keys(ui);
}

/// Write the binding and tell the daemon, which is a different process and
/// cannot see the file change.
fn bind(prefs: &mut Prefs, action: Action, hotkey: Option<Hotkey>) {
    prefs.hotkeys.retain(|(a, _)| *a != action);
    if let Some(hotkey) = hotkey {
        prefs.hotkeys.push((action, hotkey.to_string()));
    }
    prefs.save();
    crate::ipc::poke(crate::ipc::Request::ReloadHotkeys);
}

enum Recorded {
    Nothing,
    Cancelled,
    Key(Hotkey),
}

/// The next usable combination the user presses.
///
/// Escape cancels rather than binding: someone who opened recording by accident
/// needs a way out, and Escape is where every hand goes.
fn read_key(ctx: &egui::Context) -> Recorded {
    ctx.input(|i| {
        if i.key_pressed(egui::Key::Escape) {
            return Recorded::Cancelled;
        }
        let mods = Mods {
            cmd: i.modifiers.mac_cmd || i.modifiers.command,
            ctrl: i.modifiers.ctrl,
            alt: i.modifiers.alt,
            shift: i.modifiers.shift,
        };
        for event in &i.events {
            if let egui::Event::Key {
                key, pressed: true, ..
            } = event
                && let Some(name) = key_name(*key)
                && let Some(hotkey) = Hotkey::new(mods, name)
                // A bare key would be taken from every other application, and
                // Option without Cmd swallows a character the keyboard can no
                // longer type. Neither is offered, so neither is accepted.
                && hotkey.is_bindable()
            {
                return Recorded::Key(hotkey);
            }
        }
        Recorded::Nothing
    })
}

fn key_name(key: egui::Key) -> Option<&'static str> {
    use egui::Key;
    Some(match key {
        Key::Num0 => "0",
        Key::Num1 => "1",
        Key::Num2 => "2",
        Key::Num3 => "3",
        Key::Num4 => "4",
        Key::Num5 => "5",
        Key::Num6 => "6",
        Key::Num7 => "7",
        Key::Num8 => "8",
        Key::Num9 => "9",
        Key::A => "A",
        Key::B => "B",
        Key::C => "C",
        Key::D => "D",
        Key::E => "E",
        Key::F => "F",
        Key::G => "G",
        Key::H => "H",
        Key::I => "I",
        Key::J => "J",
        Key::K => "K",
        Key::L => "L",
        Key::M => "M",
        Key::N => "N",
        Key::O => "O",
        Key::P => "P",
        Key::Q => "Q",
        Key::R => "R",
        Key::S => "S",
        Key::T => "T",
        Key::U => "U",
        Key::V => "V",
        Key::W => "W",
        Key::X => "X",
        Key::Y => "Y",
        Key::Z => "Z",
        Key::F1 => "F1",
        Key::F2 => "F2",
        Key::F3 => "F3",
        Key::F4 => "F4",
        Key::F5 => "F5",
        Key::F6 => "F6",
        Key::F7 => "F7",
        Key::F8 => "F8",
        Key::F9 => "F9",
        Key::F10 => "F10",
        Key::F11 => "F11",
        Key::F12 => "F12",
        _ => return None,
    })
}

/// The `?Shortcuts` anchor is load-bearing: without it this stops at the
/// Keyboard pane and the user has to find the button themselves. With it,
/// System Settings opens the shortcut list outright — measured, because the
/// bare identifier looks like the obvious one and does less.
#[cfg(target_os = "macos")]
fn open_keyboard_settings() {
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.Keyboard-Settings.extension?Shortcuts")
        .spawn();
}

#[cfg(not(target_os = "macos"))]
fn open_keyboard_settings() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn hk(text: &str) -> Hotkey {
        text.parse().expect("valid")
    }

    #[test]
    fn an_unbound_action_offers_its_suggestion() {
        let offer = hk("Ctrl+Shift+4");
        assert_eq!(
            row_state(None, &[], Some(offer.clone())),
            RowState::Unbound(Some(offer))
        );
    }

    /// The whole quick-switch flow: while macOS holds the combination the row
    /// has to warn, and the moment it does not the warning has to go.
    #[test]
    fn a_row_warns_only_while_macos_holds_the_same_keys() {
        let bound = hk("Cmd+Shift+4");
        assert_eq!(
            row_state(Some(&bound), &[hk("Cmd+Shift+4")], None),
            RowState::Clashes(bound.clone()),
            "macOS holds it, so one press runs two things and the row must say so"
        );
        assert_eq!(
            row_state(Some(&bound), &[hk("Cmd+Shift+5")], None),
            RowState::Bound(bound),
            "macOS let it go, so the warning has to clear without a restart"
        );
    }

    /// The point of the whole ladder. Someone who switched Apple's `⌘⇧4` off did
    /// it to give those keys away — offering them a different combination while
    /// the familiar one sits free reads as the feature not understanding what it
    /// is for.
    #[test]
    fn the_familiar_combination_is_offered_when_it_is_free() {
        assert_eq!(
            hotkey::suggestion(Action::Region, &[], &[]),
            Some(hk("Cmd+Shift+4")),
            "the combination macOS uses for this job was free and was not offered"
        );
        assert_eq!(hotkey::suggestion(Action::Full, &[], &[]), Some(hk("Cmd+Shift+3")));
        assert_eq!(hotkey::suggestion(Action::Hub, &[], &[]), Some(hk("Cmd+Shift+5")));
    }

    #[test]
    fn the_ladder_takes_over_once_macos_holds_the_familiar_keys() {
        let system = vec![hk("Cmd+Shift+4")];
        let offer = hotkey::suggestion(Action::Region, &[], &system).expect("something is free");
        assert_ne!(
            offer,
            hk("Cmd+Shift+4"),
            "offering what macOS holds creates the clash the row then warns about"
        );
        assert_eq!(
            offer.key(),
            "4",
            "the ladder keeps the number so muscle memory survives, got {offer}"
        );
    }

    #[test]
    fn two_rows_are_never_offered_the_same_combination() {
        // Region and RegionCopy share a familiar combination, so this is the
        // pair that collides if offers are worked out in isolation.
        let mut spoken_for = Vec::new();
        for action in [Action::Region, Action::RegionCopy] {
            let offer = hotkey::suggestion(action, &spoken_for, &[]).expect("something is free");
            assert!(
                !spoken_for.contains(&offer),
                "{action:?} was offered {offer}, which another row already offers — \
                 accepting both binds one action over the other"
            );
            spoken_for.push(offer);
        }
    }

    #[test]
    fn every_action_has_something_to_offer() {
        for action in Action::ALL {
            assert!(
                hotkey::suggestion(action, &[], &[]).is_some(),
                "{action:?} offered nothing, so its row shows only a bare Choose button"
            );
        }
    }
}
