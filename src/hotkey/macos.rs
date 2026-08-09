//! Registering hotkeys with Carbon, and reading what macOS has bound already.
//!
//! Four measurements shape this file. All of them are in
//! `plans/reports/260809-1151-macos-global-hotkeys.md`:
//!
//! 1. No Accessibility grant is needed, so there is no permission flow here.
//! 2. `register` returns `Ok` for a combination another application already
//!    holds. Its result means the manager accepted the request, never that the
//!    combination was free — which is why nothing in this module is called
//!    `is_available`.
//! 3. When two receivers hold one combination, **both** fire. Nothing can
//!    detect that; [`used_by_system`] answers for Apple's shortcuts and for
//!    nothing else.
//! 4. One press delivers `Pressed` and `Released`. [`Registrar::pressed`]
//!    drops the second, so no caller can capture twice by forgetting to.

use std::collections::HashMap;
use std::process::Command;

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

use super::{Action, Hotkey, Mods};

/// Live registrations, and the action each one stands for.
pub struct Registrar {
    manager: GlobalHotKeyManager,
    /// Kept whole, not just by id: unregistering needs the `HotKey` back, and
    /// one cannot be rebuilt from the id the events carry.
    live: Vec<HotKey>,
    bound: HashMap<u32, Action>,
}

impl Registrar {
    pub fn new() -> Result<Self, String> {
        GlobalHotKeyManager::new()
            .map(|manager| Self {
                manager,
                live: Vec::new(),
                bound: HashMap::new(),
            })
            .map_err(|e| e.to_string())
    }

    /// `Ok` means the manager took it, not that the combination was free.
    pub fn register(&mut self, hotkey: &Hotkey, action: Action) -> Result<(), String> {
        let native =
            to_native(hotkey).ok_or_else(|| format!("{hotkey} is not a key we can bind"))?;
        self.manager.register(native).map_err(|e| e.to_string())?;
        self.live.push(native);
        self.bound.insert(native.id(), action);
        Ok(())
    }

    /// Drop every registration. The reload path rebinds from scratch rather
    /// than diffing, because a binding left live behind a new one keeps firing
    /// and nothing reports it.
    pub fn clear(&mut self) {
        if !self.live.is_empty() {
            let _ = self.manager.unregister_all(&self.live);
            self.live.clear();
        }
        self.bound.clear();
    }

    /// Every action whose key went down since the last call. `Released` is
    /// dropped here so a caller cannot capture twice by forgetting to filter.
    pub fn pressed(&self) -> Vec<Action> {
        let mut out = Vec::new();
        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state != HotKeyState::Pressed {
                continue;
            }
            if let Some(action) = self.bound.get(&event.id) {
                out.push(*action);
            }
        }
        out
    }
}

fn to_native(hotkey: &Hotkey) -> Option<HotKey> {
    let mut mods = Modifiers::empty();
    if hotkey.mods.cmd {
        mods |= Modifiers::SUPER;
    }
    if hotkey.mods.ctrl {
        mods |= Modifiers::CONTROL;
    }
    if hotkey.mods.alt {
        mods |= Modifiers::ALT;
    }
    if hotkey.mods.shift {
        mods |= Modifiers::SHIFT;
    }
    Some(HotKey::new(Some(mods), code_for(hotkey.key())?))
}

/// `Code` names a **physical position**, not the character printed on the cap.
/// `Cmd+Shift+4` therefore means the key where a US keyboard has `4`, which is
/// what every screenshot tool on this platform means by it — and what Apple's
/// own shortcut does. On a layout that disagrees, the position wins.
fn code_for(key: &str) -> Option<Code> {
    let name = match key.as_bytes() {
        [d] if d.is_ascii_digit() => format!("Digit{key}"),
        [c] if c.is_ascii_uppercase() => format!("Key{key}"),
        [b'F', ..] => key.to_owned(),
        _ => return None,
    };
    name.parse().ok()
}

// ------------------------------------------------------------ Apple's own

/// Apple's screenshot shortcuts, by the id they carry in the preference file.
const SCREENSHOT_IDS: [u32; 5] = [28, 29, 30, 31, 184];

/// `NSEvent.ModifierFlags`, which is what this file stores — *not* Carbon's
/// constants, even though the keycode beside it is a Carbon virtual code.
/// Reading one with the other's table yields plausible nonsense rather than an
/// error, so the two are kept apart deliberately.
const NS_SHIFT: i64 = 1 << 17;
const NS_CONTROL: i64 = 1 << 18;
const NS_OPTION: i64 = 1 << 19;
const NS_COMMAND: i64 = 1 << 20;

/// Carbon virtual keycodes for everything [`code_for`] can bind, so a shortcut
/// read back from the preference file lands in the same vocabulary.
const KEYCODES: [(i64, &str); 48] = [
    (0, "A"), (1, "S"), (2, "D"), (3, "F"), (4, "H"), (5, "G"), (6, "Z"), (7, "X"),
    (8, "C"), (9, "V"), (11, "B"), (12, "Q"), (13, "W"), (14, "E"), (15, "R"),
    (16, "Y"), (17, "T"), (18, "1"), (19, "2"), (20, "3"), (21, "4"), (22, "6"),
    (23, "5"), (25, "9"), (26, "7"), (28, "8"), (29, "0"), (31, "O"), (32, "U"),
    (34, "I"), (35, "P"), (37, "L"), (38, "J"), (40, "K"), (45, "N"), (46, "M"),
    (96, "F5"), (97, "F6"), (98, "F7"), (99, "F3"), (100, "F8"), (101, "F9"),
    (103, "F11"), (109, "F10"), (111, "F12"), (118, "F4"), (120, "F2"), (122, "F1"),
];

/// Apple's screenshot shortcuts that are currently switched on.
///
/// These are the *only* conflicts that can be detected. A third-party
/// application holding the same keys is invisible, and pressing them runs both.
///
/// Reading this spawns `defaults`, so the interface asks once and keeps the
/// answer rather than asking per row.
pub fn system_bindings() -> Vec<Hotkey> {
    let Some(text) = read_symbolic_hotkeys() else {
        return Vec::new();
    };
    parse_symbolic_hotkeys(&text)
        .into_iter()
        .filter_map(|(hotkey, enabled)| enabled.then_some(hotkey))
        .collect()
}

fn read_symbolic_hotkeys() -> Option<String> {
    let out = Command::new("defaults")
        .args(["read", "com.apple.symbolichotkeys", "AppleSymbolicHotKeys"])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The screenshot entries, paired with whether they are switched on.
///
/// Apple has added identifiers before and the file is a plist rendered for
/// humans, so anything unrecognised or short is skipped rather than fatal.
fn parse_symbolic_hotkeys(text: &str) -> Vec<(Hotkey, bool)> {
    let mut out = Vec::new();
    let mut wanted = false;
    let mut enabled = None;
    let mut params: Vec<i64> = Vec::new();
    let mut in_params = false;

    for line in text.lines() {
        let line = line.trim();

        // Only `28 =     {` opens an entry. The test for the trailing brace is
        // load-bearing: the last number inside `parameters` carries no comma,
        // so `1179648` on its own parses as an identifier and would silently
        // reset the entry being read.
        if line.ends_with('{')
            && let Some(head) = line.split('=').next()
            && let Ok(id) = head.trim().parse::<u32>()
        {
            wanted = SCREENSHOT_IDS.contains(&id);
            enabled = None;
            params.clear();
            in_params = false;
            continue;
        }
        if !wanted {
            continue;
        }

        if let Some(value) = line.strip_prefix("enabled = ") {
            enabled = value.trim_end_matches(';').trim().parse::<i64>().ok();
        } else if line.starts_with("parameters") {
            in_params = true;
            params.clear();
        } else if in_params && line.starts_with(')') {
            in_params = false;
            if let (Some(on), [_, keycode, mask]) = (enabled, params.as_slice())
                && let Some(hotkey) = from_parameters(*keycode, *mask)
            {
                out.push((hotkey, on != 0));
            }
        } else if in_params
            && let Ok(n) = line.trim_end_matches(',').trim().parse::<i64>()
        {
            params.push(n);
        }
    }
    out
}

fn from_parameters(keycode: i64, mask: i64) -> Option<Hotkey> {
    let key = KEYCODES.iter().find(|(c, _)| *c == keycode)?.1;
    let mods = Mods {
        cmd: mask & NS_COMMAND != 0,
        ctrl: mask & NS_CONTROL != 0,
        alt: mask & NS_OPTION != 0,
        shift: mask & NS_SHIFT != 0,
    };
    Hotkey::new(mods, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured from a real machine. The suite must never run `defaults`: that
    /// would read whoever's settings the tests happen to run under, and pass or
    /// fail accordingly.
    const FIXTURE: &str = include_str!("testdata/symbolichotkeys.txt");

    fn parsed() -> Vec<(Hotkey, bool)> {
        parse_symbolic_hotkeys(FIXTURE)
    }

    #[test]
    fn the_five_screenshot_shortcuts_are_recognised() {
        let found: Vec<String> = parsed().iter().map(|(h, _)| h.to_string()).collect();
        for expected in [
            "Cmd+Shift+3",
            "Cmd+Ctrl+Shift+3",
            "Cmd+Shift+4",
            "Cmd+Ctrl+Shift+4",
            "Cmd+Shift+5",
        ] {
            assert!(
                found.iter().any(|f| f == expected),
                "{expected} was not read back, so the interface cannot warn that \
                 macOS holds it. Found: {found:?}"
            );
        }
    }

    /// The whole quick-switch flow keys off this flag: a user frees a
    /// combination in System Settings and the warning has to clear.
    #[test]
    fn a_disabled_shortcut_reads_as_disabled() {
        let by_name = |name: &str| {
            parsed()
                .into_iter()
                .find(|(h, _)| h.to_string() == name)
                .map(|(_, on)| on)
        };
        assert_eq!(
            by_name("Cmd+Shift+3"),
            Some(false),
            "a switched-off shortcut read as on would warn about a key that is free"
        );
        assert_eq!(
            by_name("Cmd+Shift+4"),
            Some(true),
            "a switched-on shortcut read as off would let a collision through silently"
        );
    }

    #[test]
    fn nsevent_masks_decode_to_the_right_modifiers() {
        let hk = from_parameters(21, 1179648).expect("Cmd+Shift+4");
        assert_eq!(
            hk.to_string(),
            "Cmd+Shift+4",
            "1179648 is NSEvent's command|shift; reading it with Carbon's table \
             gives a plausible wrong answer instead of an error"
        );
        let hk = from_parameters(20, 1441792).expect("Cmd+Ctrl+Shift+3");
        assert_eq!(hk.to_string(), "Cmd+Ctrl+Shift+3");
    }

    #[test]
    fn malformed_entries_are_skipped_not_fatal() {
        assert_eq!(
            parsed().len(),
            5,
            "an entry with too few parameters, or none at all, must not stop the \
             rest of the file from being read"
        );
    }

    #[test]
    fn an_unknown_keycode_yields_nothing() {
        assert!(
            from_parameters(65535, 1179648).is_none(),
            "65535 is the placeholder Apple writes for 'no key', and must not \
             become a binding"
        );
    }

    /// The façade admits digits, letters and F1..F20; every one of them has to
    /// survive the trip into a `Code`, or a combination the interface offered
    /// fails at the moment it is bound.
    #[test]
    fn every_key_the_facade_admits_maps_to_a_physical_code() {
        for key in ["0", "4", "9", "A", "M", "Z", "F1", "F9", "F12", "F20"] {
            assert!(
                code_for(key).is_some(),
                "{key} passes the façade's allowlist but cannot be registered"
            );
        }
        assert!(
            code_for("Space").is_none(),
            "the allowlist does not admit it, so neither should the mapping"
        );
    }
}
