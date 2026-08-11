//! Global capture hotkeys.
//!
//! macOS registers these in-process; Linux and Windows do not, and that is not
//! a gap. COSMIC and GNOME already bind a command to a key, so there the answer
//! is `shotr --capture` in the desktop's own settings. macOS offers no such
//! facility for anything but its own five screenshot shortcuts, which is the
//! hole this fills.
//!
//! This module is the vocabulary: what a hotkey is, how it reads and writes as
//! text, and which actions can carry one. Nothing here touches a platform API —
//! the macOS backend does that, and it is the only place a `global_hotkey` type
//! will appear.
//!
//! Measurements behind the design:
//! `plans/reports/260809-1151-macos-global-hotkeys.md`.

use std::fmt;
use std::str::FromStr;

use crate::settings::Prefs;
use crate::tray::Command;

#[cfg(target_os = "macos")]
pub mod macos;

/// Whether this platform binds capture hotkeys in-process.
///
/// `false` is not a gap to be filled later: COSMIC and GNOME bind a command to
/// a key themselves, so there Preferences points at the desktop's own settings
/// instead of growing a picker that would duplicate it.
pub const EDITABLE: bool = cfg!(target_os = "macos");

/// Combinations the system itself is using right now.
///
/// Only macOS can answer, and only about Apple's own screenshot shortcuts. An
/// application holding the same keys is invisible on every platform, and
/// pressing them runs both — so a combination missing from this list means "the
/// system is not using it", never "this key is free".
///
/// Reading it costs a subprocess, so callers ask once and keep the answer.
pub fn system_bindings() -> Vec<Hotkey> {
    #[cfg(target_os = "macos")]
    return macos::system_bindings();
    #[cfg(not(target_os = "macos"))]
    Vec::new()
}

/// The modifiers a hotkey can hold, named the way `prefs.json` should read them
/// rather than the way any dependency spells them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Mods {
    pub cmd: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Mods {
    pub fn any(&self) -> bool {
        self.cmd || self.ctrl || self.alt || self.shift
    }

    /// Option composes text on macOS — `⌥⇧4` types `›` — so grabbing it without
    /// Cmd takes a character away from the whole keyboard.
    pub fn steals_typing(&self) -> bool {
        self.alt && !self.cmd
    }
}

/// A combination, stored and compared in one canonical form.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Hotkey {
    pub mods: Mods,
    /// Already normalised: `4`, `A`, `F5`.
    key: String,
}

impl Hotkey {
    /// `None` when the key is not one this can bind — see [`normalise_key`].
    pub fn new(mods: Mods, key: &str) -> Option<Self> {
        normalise_key(key).map(|key| Self { mods, key })
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    /// Binding a key with no modifier takes it from every other application.
    pub fn is_bindable(&self) -> bool {
        self.mods.any() && !self.mods.steals_typing()
    }
}

/// Digits, letters and function keys — a deliberate allowlist, not the limit of
/// what Carbon can register. Punctuation and Space register perfectly well; they
/// are left out because a hotkey built from them is harder to describe in the
/// interface than it is worth.
///
/// The function-key arm rebuilds the string rather than returning the input, so
/// `F07` and `F+7` — both of which `u8::from_str` accepts — cannot reach
/// `prefs.json` as spellings that never parse back.
fn normalise_key(key: &str) -> Option<String> {
    let key = key.trim().to_ascii_uppercase();
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphanumeric() => Some(key),
        (Some('F'), Some(d)) if d.is_ascii_digit() => match key[1..].parse::<u8>() {
            Ok(n @ 1..=20) => Some(format!("F{n}")),
            _ => None,
        },
        _ => None,
    }
}

/// Modifiers always render in this order, which is what makes the stored string
/// canonical rather than merely a string: two settings files describing one
/// binding cannot disagree about how to spell it.
impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [
            (self.mods.cmd, "Cmd"),
            (self.mods.ctrl, "Ctrl"),
            (self.mods.alt, "Alt"),
            (self.mods.shift, "Shift"),
        ] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        f.write_str(&self.key)
    }
}

impl FromStr for Hotkey {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        let mut mods = Mods::default();
        let mut key = None;
        for token in s.split('+') {
            let token = token.trim();
            let slot = match token.to_ascii_lowercase().as_str() {
                "cmd" | "command" | "super" | "meta" => Some(&mut mods.cmd),
                "ctrl" | "control" => Some(&mut mods.ctrl),
                "alt" | "opt" | "option" => Some(&mut mods.alt),
                "shift" => Some(&mut mods.shift),
                _ => None,
            };
            match slot {
                // A repeated modifier means the string was assembled by
                // something that did not understand it. Accepting it would let
                // a malformed row bind silently.
                Some(flag) if *flag => return Err(()),
                Some(flag) => *flag = true,
                None if key.is_none() => key = Some(normalise_key(token).ok_or(())?),
                None => return Err(()),
            }
        }
        Ok(Self {
            mods,
            key: key.ok_or(())?,
        })
    }
}

/// The six things that can carry a binding.
///
/// Not every [`Command`] can: `CaptureMonitor` and `CaptureWindow` need a
/// runtime argument, and `Quit` is not a capture. This names the subset and
/// nothing else — it deliberately carries no command-line knowledge, so
/// [`Command::args`] stays the only place an action becomes an invocation.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    Region,
    Full,
    RegionCopy,
    FullCopy,
    RegionPin,
    Hub,
}

impl Action {
    pub const ALL: [Action; 6] = [
        Action::Region,
        Action::Full,
        Action::RegionCopy,
        Action::FullCopy,
        Action::RegionPin,
        Action::Hub,
    ];

    pub fn command(self) -> Command {
        match self {
            Action::Region => Command::CaptureRegion,
            Action::Full => Command::CaptureFull,
            Action::RegionCopy => Command::CaptureRegionCopy,
            Action::FullCopy => Command::CaptureFullCopy,
            Action::RegionPin => Command::CaptureRegionPin,
            Action::Hub => Command::History,
        }
    }
}

/// Live registrations, or nothing at all where the desktop owns the binding.
///
/// The daemon holds one of these and never asks which platform it is on.
#[derive(Default)]
pub struct Hotkeys {
    #[cfg(target_os = "macos")]
    registrar: Option<macos::Registrar>,
}

impl Hotkeys {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop everything currently bound and bind what `stored` asks for.
    ///
    /// Wholesale rather than diffing: a registration left live behind a new one
    /// keeps firing, and no API reports that it did — so the only way to be sure
    /// the old key is dead is never to have kept it.
    pub fn rebind(&mut self, stored: &[(Action, String)]) {
        #[cfg(target_os = "macos")]
        {
            let live = bindings(stored);
            if live.is_empty() && self.registrar.is_none() {
                return;
            }
            let registrar = match self.registrar.as_mut() {
                Some(registrar) => registrar,
                None => match macos::Registrar::new() {
                    Ok(registrar) => self.registrar.insert(registrar),
                    Err(e) => {
                        eprintln!("Could not take global hotkeys: {e}");
                        return;
                    }
                },
            };
            registrar.clear();
            let mut bound = 0;
            for (action, hotkey) in live {
                match registrar.register(&hotkey, action) {
                    Ok(()) => bound += 1,
                    Err(e) => eprintln!("Could not bind {hotkey}: {e}"),
                }
            }
            // Worth a line: registering a hotkey cannot fail in a way that says
            // anything, so this is the only evidence that a binding took.
            eprintln!("Global hotkeys: {bound} bound.");
        }
        #[cfg(not(target_os = "macos"))]
        let _ = stored;
    }

    /// Every action whose key went down since the last call.
    pub fn pressed(&mut self) -> Vec<Action> {
        #[cfg(target_os = "macos")]
        {
            self.registrar
                .as_ref()
                .map(macos::Registrar::pressed)
                .unwrap_or_default()
        }
        #[cfg(not(target_os = "macos"))]
        Vec::new()
    }
}

/// Give a fresh install one working hotkey, once.
///
/// A screenshot tool with no shortcut until you find a Preferences pane reads
/// as a screenshot tool with no shortcut. The reason not to bind silently still
/// stands — macOS cannot report whether a combination is free, so a key nobody
/// knows about could quietly do two things — which is why this returns what it
/// chose: the caller has to say so out loud.
///
/// Only region capture. Claiming five combinations on behalf of someone who has
/// not asked is a different thing from getting them started.
///
/// `None` when there is nothing to do, which is every run after the first.
///
/// `system` is passed in rather than read here, so the decision is a pure
/// function: reading it spawns `defaults`, and a test that did that would
/// depend on whose machine it ran on.
pub fn first_run_binding(prefs: &Prefs, system: &[Hotkey]) -> Option<(Action, Hotkey)> {
    // A settings file that already names bindings was set up by somebody,
    // whatever the flag says — a file written before the flag existed still has
    // them. Without this check the starting shortcut lands *beside* the one they
    // already chose, and region capture answers to two combinations.
    if !EDITABLE || prefs.hotkeys_initialised || !prefs.hotkeys.is_empty() {
        return None;
    }
    suggestion(Action::Region, &[], system).map(|hotkey| (Action::Region, hotkey))
}

/// The bindings held in `Prefs`, with anything unusable dropped.
///
/// A settings file people edit by hand must not be able to stop the daemon
/// starting, and one bad row must not cost the user the rest of their hotkeys —
/// so this reports and skips rather than failing.
pub fn bindings(stored: &[(Action, String)]) -> Vec<(Action, Hotkey)> {
    stored
        .iter()
        .filter_map(|(action, text)| match text.parse::<Hotkey>() {
            Ok(hotkey) if hotkey.is_bindable() => Some((*action, hotkey)),
            _ => {
                eprintln!("Ignoring an unusable hotkey for {action:?}: {text:?}");
                None
            }
        })
        .collect()
}

/// The combination macOS uses for the same job, which is where an offer starts.
fn familiar(action: Action) -> &'static str {
    match action {
        Action::Region | Action::RegionCopy | Action::RegionPin => "Cmd+Shift+4",
        Action::Full | Action::FullCopy => "Cmd+Shift+3",
        Action::Hub => "Cmd+Shift+5",
    }
}

/// What to offer for an action nothing is bound to.
///
/// **The familiar combination comes first**, because when it is free that is
/// plainly the one to offer — someone who switched Apple's `⌘⇧4` off did it to
/// give those keys away. Only when it is taken does [`candidates`] apply,
/// keeping the number and moving the modifier hand.
///
/// `taken` is everything already spoken for: bound already, or offered to
/// another action in the same pass. Several actions share a familiar
/// combination, so working offers out in isolation proposes the same keys twice.
pub fn suggestion(action: Action, taken: &[Hotkey], system: &[Hotkey]) -> Option<Hotkey> {
    let familiar: Hotkey = familiar(action).parse().ok()?;
    std::iter::once(familiar.clone())
        .chain(candidates(&familiar))
        .find(|c| !taken.contains(c) && !system.contains(c))
}

/// Neighbours of a combination the user already knows: the number stays, the
/// modifier hand moves, so muscle memory survives.
///
/// This is a **display order and nothing more.** `register()` returns `Ok` for a
/// combination another application already holds, so no list can claim a key is
/// free — see the report. Nothing downstream may read position here as evidence.
pub fn candidates(taken: &Hotkey) -> Vec<Hotkey> {
    const LADDER: [Mods; 3] = [
        Mods {
            cmd: false,
            ctrl: true,
            alt: false,
            shift: true,
        },
        Mods {
            cmd: true,
            ctrl: false,
            alt: true,
            shift: true,
        },
        Mods {
            cmd: true,
            ctrl: true,
            alt: false,
            shift: true,
        },
    ];

    LADDER
        .iter()
        .filter(|mods| *mods != &taken.mods && !mods.steals_typing())
        .map(|mods| Hotkey {
            mods: *mods,
            key: taken.key.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_combination_survives_a_round_trip() {
        for text in [
            "Cmd+Shift+4",
            "Ctrl+Shift+4",
            "Cmd+Ctrl+Alt+Shift+A",
            "Cmd+F5",
            "Shift+9",
        ] {
            let parsed: Hotkey = text.parse().expect("should parse");
            assert_eq!(
                parsed.to_string(),
                text,
                "{text} did not survive parse then format, so a stored binding \
                 would change meaning on the next load"
            );
        }
    }

    #[test]
    fn modifier_order_is_normalised_not_preserved() {
        let parsed: Hotkey = "Shift+Cmd+4".parse().expect("should parse");
        assert_eq!(
            parsed.to_string(),
            "Cmd+Shift+4",
            "two spellings of one binding must store identically, or a settings \
             file and the interface disagree about what is bound"
        );
    }

    #[test]
    fn spelling_variants_reach_the_same_combination() {
        let a: Hotkey = "Command+Option+5".parse().expect("should parse");
        let b: Hotkey = "cmd+alt+5".parse().expect("should parse");
        assert_eq!(a, b, "an alias must not produce a different binding");
    }

    #[test]
    fn garbage_is_rejected_rather_than_guessed() {
        for text in [
            "",
            "Cmd+",
            "+4",
            "Nope+4",
            "Cmd+Shift",
            "Cmd+F21",
            "Cmd+F0",
            "Cmd+ab",
            "Cmd+4+5",
            // A repeated modifier, including through an alias.
            "Cmd+Cmd+4",
            "Cmd+super+4",
        ] {
            assert!(
                text.parse::<Hotkey>().is_err(),
                "{text:?} parsed, so a malformed settings row would bind \
                 something the user never asked for"
            );
        }
    }

    /// `u8::from_str` accepts `07` and `+7`, so an unguarded function-key arm
    /// stores a spelling that never parses back — a binding that vanishes on
    /// the next load.
    #[test]
    fn function_keys_normalise_to_one_spelling() {
        for text in ["Cmd+F07", "Cmd+F7"] {
            let parsed: Hotkey = text.parse().expect("should parse");
            assert_eq!(
                parsed.to_string(),
                "Cmd+F7",
                "{text} stored a spelling that is not the canonical one"
            );
        }
        assert!(
            Hotkey::new(Mods::default(), "F+7").is_none(),
            "a key built with a stray sign would render as Cmd+F+7 and never \
             parse back"
        );
    }

    #[test]
    fn no_two_actions_mean_the_same_command() {
        let commands: Vec<_> = Action::ALL.iter().map(|a| a.command()).collect();
        for (i, a) in commands.iter().enumerate() {
            for b in &commands[i + 1..] {
                assert_ne!(
                    a, b,
                    "two actions map to {a:?}, so binding one silently rebinds \
                     the other"
                );
            }
        }
    }

    #[test]
    fn a_key_with_no_modifier_is_not_bindable() {
        let bare = Hotkey::new(Mods::default(), "4").expect("valid key");
        assert!(
            !bare.is_bindable(),
            "binding a bare key globally takes it from every other application"
        );
    }

    #[test]
    fn option_without_cmd_is_not_bindable() {
        let mods = Mods {
            alt: true,
            shift: true,
            ..Mods::default()
        };
        let hk = Hotkey::new(mods, "4").expect("valid key");
        assert!(
            !hk.is_bindable(),
            "Option composes text on macOS, so grabbing it without Cmd costs \
             the keyboard a character"
        );
    }

    #[test]
    fn every_action_maps_to_a_real_invocation() {
        for action in Action::ALL {
            let args = action.command().args();
            assert!(
                !args.is_empty(),
                "{action:?} would launch the daemon instead of capturing"
            );
        }
    }

    /// A screenshot tool whose shortcut has to be found in a Preferences pane
    /// reads, to most people, as a screenshot tool with no shortcut.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_fresh_install_is_given_one_working_shortcut() {
        let fresh = Prefs::default();
        let (action, hotkey) =
            first_run_binding(&fresh, &[]).expect("a fresh install got nothing at all");
        assert_eq!(
            action,
            Action::Region,
            "region capture is the one worth claiming unasked; the rest is a \
             land grab on someone who has not asked for anything"
        );
        assert_eq!(hotkey.to_string(), "Cmd+Shift+4");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_starting_shortcut_avoids_what_macos_holds() {
        let system = vec!["Cmd+Shift+4".parse().expect("valid")];
        let (_, hotkey) = first_run_binding(&Prefs::default(), &system).expect("something is free");
        assert_ne!(
            hotkey.to_string(),
            "Cmd+Shift+4",
            "binding what macOS holds means one press runs two things, and \
             nothing anywhere reports it"
        );
    }

    /// Someone who cleared every binding meant it. Handing one back on the next
    /// restart is the app arguing with them.
    #[test]
    fn the_starting_shortcut_is_offered_once_and_never_again() {
        let settled = Prefs {
            hotkeys_initialised: true,
            ..Prefs::default()
        };
        assert_eq!(first_run_binding(&settled, &[]), None);

        let cleared = Prefs {
            hotkeys: Vec::new(),
            hotkeys_initialised: true,
            ..Prefs::default()
        };
        assert_eq!(
            first_run_binding(&cleared, &[]),
            None,
            "an empty list after the first run is a decision, not a fresh install"
        );
    }

    /// A settings file written before the flag existed still names bindings.
    /// Adding the starting shortcut beside them leaves region capture answering
    /// to two combinations, which is how this was found.
    #[test]
    fn an_existing_binding_settles_it_even_with_the_flag_unset() {
        let upgraded = Prefs {
            hotkeys: vec![(Action::Region, "Cmd+Shift+4".to_owned())],
            hotkeys_initialised: false,
            ..Prefs::default()
        };
        assert_eq!(
            first_run_binding(&upgraded, &[]),
            None,
            "a second combination was added for an action that already had one"
        );
    }

    #[test]
    fn one_bad_row_does_not_cost_the_others() {
        let stored = vec![
            (Action::Region, "Cmd+Shift+4".to_owned()),
            (Action::Full, "not a hotkey".to_owned()),
            // Parses, but binding a bare key takes it from everything else.
            (Action::Hub, "9".to_owned()),
            (Action::FullCopy, "Cmd+Ctrl+Shift+3".to_owned()),
        ];
        let live = bindings(&stored);
        assert_eq!(
            live.len(),
            2,
            "a typo in one row must not lose the hotkeys that are fine"
        );
        assert!(
            live.iter().all(|(_, hotkey)| hotkey.is_bindable()),
            "an unbindable row survived, so the daemon would grab a bare key"
        );
    }

    #[test]
    fn candidates_keep_the_key_and_avoid_the_input() {
        let taken: Hotkey = "Cmd+Shift+4".parse().expect("should parse");
        let offered = candidates(&taken);

        assert!(!offered.is_empty(), "a taken key must still offer neighbours");
        for candidate in &offered {
            assert_eq!(
                candidate.key(),
                taken.key(),
                "changing the key loses the muscle memory the ladder exists for"
            );
            assert_ne!(
                candidate, &taken,
                "offering the combination the user already has reads as broken"
            );
            assert!(
                candidate.is_bindable(),
                "{candidate} was offered but cannot be bound"
            );
        }
    }

    #[test]
    fn candidates_never_steal_a_typed_character() {
        for text in ["Cmd+Shift+4", "Ctrl+Shift+4", "Cmd+Alt+Shift+7"] {
            let taken: Hotkey = text.parse().expect("should parse");
            for candidate in candidates(&taken) {
                assert!(
                    !candidate.mods.steals_typing(),
                    "{candidate} holds Option without Cmd and would swallow a \
                     character the keyboard can no longer type"
                );
            }
        }
    }
}
