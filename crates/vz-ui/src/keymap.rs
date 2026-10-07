//! Keyboard shortcuts: the commands that can be changed, two ready-made sets, and the user's own changes.

use crate::settings::UiSettings;
use eframe::egui::{Key, Modifiers};
use std::collections::BTreeMap;

/// A key with the modifier keys held with it. Written as text like `Ctrl+Shift+Z`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Combo {
    /// Ctrl on Windows and Linux, Cmd on a Mac.
    pub command: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: Key,
}

impl Combo {
    pub const fn new(key: Key) -> Self {
        Self {
            command: false,
            shift: false,
            alt: false,
            key,
        }
    }

    pub const fn ctrl(key: Key) -> Self {
        Self {
            command: true,
            ..Self::new(key)
        }
    }

    pub const fn ctrl_shift(key: Key) -> Self {
        Self {
            command: true,
            shift: true,
            ..Self::new(key)
        }
    }

    pub const fn alt(key: Key) -> Self {
        Self {
            alt: true,
            ..Self::new(key)
        }
    }

    pub fn modifiers(self) -> Modifiers {
        let mut m = Modifiers::NONE;
        if self.command {
            m |= Modifiers::COMMAND;
        }
        if self.shift {
            m |= Modifiers::SHIFT;
        }
        if self.alt {
            m |= Modifiers::ALT;
        }
        m
    }

    fn weight(self) -> u8 {
        u8::from(self.command) + u8::from(self.shift) + u8::from(self.alt)
    }

    /// `Ctrl+Shift+Z`
    pub fn text(self) -> String {
        let mut parts = Vec::new();
        if self.command {
            parts.push("Ctrl");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.alt {
            parts.push("Alt");
        }
        parts.push(self.key.name());
        parts.join("+")
    }

    pub fn parse(text: &str) -> Option<Self> {
        let mut combo = Self::new(Key::Space);
        let mut key = None;
        for part in text.split('+') {
            match part.trim().to_ascii_lowercase().as_str() {
                "ctrl" | "cmd" | "command" => combo.command = true,
                "shift" => combo.shift = true,
                "alt" => combo.alt = true,
                other => key = Key::from_name(other).or_else(|| Key::from_name(part.trim())),
            }
        }
        combo.key = key?;
        Some(combo)
    }
}

/// Everything in vidzcut that has a shortcut you can change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Cmd {
    PlayPause,
    Undo,
    Redo,
    Save,
    SaveAs,
    Open,
    Import,
    Export,
    Split,
    Duplicate,
    Group,
    Ungroup,
    Compound,
    Marker,
    Delete,
    FrameBack,
    FrameForward,
    GoToStart,
    GoToEnd,
    Deselect,
}

impl Cmd {
    pub const ALL: [Self; 20] = [
        Self::PlayPause,
        Self::Undo,
        Self::Redo,
        Self::Save,
        Self::SaveAs,
        Self::Open,
        Self::Import,
        Self::Export,
        Self::Split,
        Self::Duplicate,
        Self::Group,
        Self::Ungroup,
        Self::Compound,
        Self::Marker,
        Self::Delete,
        Self::FrameBack,
        Self::FrameForward,
        Self::GoToStart,
        Self::GoToEnd,
        Self::Deselect,
    ];

    /// The name used in the settings file.
    pub fn id(self) -> &'static str {
        match self {
            Self::PlayPause => "play_pause",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::Save => "save",
            Self::SaveAs => "save_as",
            Self::Open => "open",
            Self::Import => "import",
            Self::Export => "export",
            Self::Split => "split",
            Self::Duplicate => "duplicate",
            Self::Group => "group",
            Self::Ungroup => "ungroup",
            Self::Compound => "compound",
            Self::Marker => "marker",
            Self::Delete => "delete",
            Self::FrameBack => "frame_back",
            Self::FrameForward => "frame_forward",
            Self::GoToStart => "go_to_start",
            Self::GoToEnd => "go_to_end",
            Self::Deselect => "deselect",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::PlayPause => "Play / pause",
            Self::Undo => "Undo",
            Self::Redo => "Redo",
            Self::Save => "Save project",
            Self::SaveAs => "Save project as…",
            Self::Open => "Open project…",
            Self::Import => "Import media…",
            Self::Export => "Export…",
            Self::Split => "Split at the playhead",
            Self::Duplicate => "Duplicate clips",
            Self::Group => "Group clips",
            Self::Ungroup => "Ungroup clips",
            Self::Compound => "Make a compound clip",
            Self::Marker => "Add / remove a marker",
            Self::Delete => "Delete selected clips",
            Self::FrameBack => "One picture back",
            Self::FrameForward => "One picture forward",
            Self::GoToStart => "Go to the start",
            Self::GoToEnd => "Go to the end",
            Self::Deselect => "Select nothing",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.id() == id)
    }
}

/// The ready-made sets of shortcuts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    /// The layout most phone-and-desktop editors share. The default.
    Vidzcut,
    /// For people used to Premiere Pro.
    Premiere,
}

impl Preset {
    pub const ALL: [Self; 2] = [Self::Vidzcut, Self::Premiere];

    pub fn id(self) -> &'static str {
        match self {
            Self::Vidzcut => "vidzcut",
            Self::Premiere => "premiere",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Vidzcut => "vidzcut (CapCut-style)",
            Self::Premiere => "Premiere-style",
        }
    }

    pub fn from_id(id: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|p| p.id() == id)
            .unwrap_or(Self::Vidzcut)
    }

    /// The shortcut a preset gives a command.
    pub fn combo(self, cmd: Cmd) -> Combo {
        match (self, cmd) {
            (_, Cmd::PlayPause) => Combo::new(Key::Space),
            (_, Cmd::Undo) => Combo::ctrl(Key::Z),
            (_, Cmd::Redo) => Combo::ctrl_shift(Key::Z),
            (_, Cmd::Save) => Combo::ctrl(Key::S),
            (_, Cmd::SaveAs) => Combo::ctrl_shift(Key::S),
            (_, Cmd::Open) => Combo::ctrl(Key::O),
            (_, Cmd::Import) => Combo::ctrl(Key::I),
            (Self::Vidzcut, Cmd::Export) => Combo::ctrl(Key::E),
            (Self::Premiere, Cmd::Export) => Combo::ctrl(Key::M),
            (Self::Vidzcut, Cmd::Split) => Combo::ctrl(Key::B),
            (Self::Premiere, Cmd::Split) => Combo::ctrl(Key::K),
            (_, Cmd::Duplicate) => Combo::ctrl(Key::D),
            (_, Cmd::Group) => Combo::ctrl(Key::G),
            (_, Cmd::Ungroup) => Combo::ctrl_shift(Key::G),
            (_, Cmd::Compound) => Combo::alt(Key::G),
            (_, Cmd::Marker) => Combo::new(Key::M),
            (_, Cmd::Delete) => Combo::new(Key::Delete),
            (_, Cmd::FrameBack) => Combo::new(Key::ArrowLeft),
            (_, Cmd::FrameForward) => Combo::new(Key::ArrowRight),
            (_, Cmd::GoToStart) => Combo::new(Key::Home),
            (_, Cmd::GoToEnd) => Combo::new(Key::End),
            (_, Cmd::Deselect) => Combo::new(Key::Escape),
        }
    }
}

/// The shortcuts in use: a preset plus the user's own changes.
#[derive(Clone, Debug)]
pub struct Keymap {
    map: BTreeMap<Cmd, Combo>,
}

impl Keymap {
    pub fn from_settings(settings: &UiSettings) -> Self {
        let preset = Preset::from_id(&settings.shortcut_preset);
        let mut map: BTreeMap<Cmd, Combo> =
            Cmd::ALL.iter().map(|&c| (c, preset.combo(c))).collect();
        for (id, text) in &settings.shortcuts {
            if let (Some(cmd), Some(combo)) = (Cmd::from_id(id), Combo::parse(text)) {
                map.insert(cmd, combo);
            }
        }
        Self { map }
    }

    pub fn get(&self, cmd: Cmd) -> Combo {
        self.map[&cmd]
    }

    /// The command that already uses `combo`, other than `except`.
    pub fn used_by(&self, combo: Combo, except: Cmd) -> Option<Cmd> {
        self.map
            .iter()
            .find(|(c, k)| **c != except && **k == combo)
            .map(|(c, _)| *c)
    }

    /// Commands ordered so shortcuts with more modifier keys are tried first
    /// (Ctrl+Shift+Z must be looked at before Ctrl+Z).
    pub fn in_matching_order(&self) -> Vec<(Cmd, Combo)> {
        let mut all: Vec<_> = self.map.iter().map(|(c, k)| (*c, *k)).collect();
        all.sort_by_key(|(_, k)| std::cmp::Reverse(k.weight()));
        all
    }
}

/// Save a changed shortcut into the settings (or forget the change if it equals the preset).
pub fn set_override(settings: &mut UiSettings, cmd: Cmd, combo: Combo) {
    let preset = Preset::from_id(&settings.shortcut_preset);
    if preset.combo(cmd) == combo {
        settings.shortcuts.remove(cmd.id());
    } else {
        settings
            .shortcuts
            .insert(cmd.id().to_string(), combo.text());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combos_round_trip_through_text() {
        for cmd in Cmd::ALL {
            for preset in Preset::ALL {
                let c = preset.combo(cmd);
                assert_eq!(Combo::parse(&c.text()), Some(c), "{cmd:?} {c:?}");
            }
        }
        assert_eq!(
            Combo::parse("ctrl+shift+z"),
            Some(Combo::ctrl_shift(Key::Z))
        );
        assert_eq!(Combo::parse("Ctrl+"), None);
        assert_eq!(Combo::parse(""), None);
    }

    #[test]
    fn presets_have_no_clashes_inside_themselves() {
        for preset in Preset::ALL {
            let mut seen = std::collections::HashSet::new();
            for cmd in Cmd::ALL {
                assert!(seen.insert(preset.combo(cmd)), "{preset:?} repeats {cmd:?}");
            }
        }
    }

    #[test]
    fn the_users_changes_win_and_the_preset_can_change() {
        let mut s = UiSettings::default();
        assert_eq!(
            Keymap::from_settings(&s).get(Cmd::Split),
            Combo::ctrl(Key::B)
        );
        s.shortcut_preset = "premiere".into();
        assert_eq!(
            Keymap::from_settings(&s).get(Cmd::Split),
            Combo::ctrl(Key::K)
        );
        set_override(&mut s, Cmd::Split, Combo::new(Key::X));
        assert_eq!(
            Keymap::from_settings(&s).get(Cmd::Split),
            Combo::new(Key::X)
        );
        // setting it back to the preset's own key forgets the change
        set_override(&mut s, Cmd::Split, Combo::ctrl(Key::K));
        assert!(s.shortcuts.is_empty());
        // rubbish in the file is ignored
        s.shortcuts.insert("nonsense".into(), "Ctrl+Q".into());
        s.shortcuts.insert("split".into(), "???".into());
        assert_eq!(
            Keymap::from_settings(&s).get(Cmd::Split),
            Combo::ctrl(Key::K)
        );
    }

    #[test]
    fn conflicts_are_found_and_longer_combos_come_first() {
        let k = Keymap::from_settings(&UiSettings::default());
        assert_eq!(k.used_by(Combo::ctrl(Key::S), Cmd::Split), Some(Cmd::Save));
        assert_eq!(k.used_by(Combo::ctrl(Key::S), Cmd::Save), None);
        let order = k.in_matching_order();
        let redo = order.iter().position(|(c, _)| *c == Cmd::Redo).unwrap();
        let undo = order.iter().position(|(c, _)| *c == Cmd::Undo).unwrap();
        assert!(redo < undo);
    }
}
