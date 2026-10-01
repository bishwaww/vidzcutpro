//! Window layout choices that are remembered between runs.
//!
//! These are about the screen (panel sizes), not about a project, so they live in their own
//! file in the user's config folder. Set `VIDZCUT_CONFIG_DIR` to use a different folder
//! (the developer checks do this, so they never touch the real settings).

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The ways the editor's panels can be arranged (spec §5.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayoutPreset {
    /// Assets left, player in the middle, inspector right, timeline below.
    #[default]
    Default,
    /// Assets left, inspector in the middle, player on the right.
    PlayerRight,
    /// Like Default but with a taller timeline.
    BigTimeline,
}

impl LayoutPreset {
    pub const ALL: [Self; 3] = [Self::Default, Self::PlayerRight, Self::BigTimeline];

    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::PlayerRight => "Player on the right",
            Self::BigTimeline => "Big timeline",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::PlayerRight => "player_right",
            Self::BigTimeline => "big_timeline",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.key() == key)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UiSettings {
    pub layout: LayoutPreset,
    /// Panel sizes in points, keyed like `default.left`.
    pub sizes: BTreeMap<String, f32>,
    /// Recently opened or saved project files, newest first.
    #[serde(default)]
    pub recent: Vec<PathBuf>,
    /// The folder the last export went to.
    #[serde(default)]
    pub export_dir: Option<PathBuf>,
}

impl UiSettings {
    fn path() -> Option<PathBuf> {
        let dir = match std::env::var_os("VIDZCUT_CONFIG_DIR") {
            Some(d) => PathBuf::from(d),
            None => ProjectDirs::from("", "", "vidzcut")?
                .config_dir()
                .to_path_buf(),
        };
        Some(dir.join("ui_settings.json"))
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match serde_json::to_string_pretty(self) {
            Ok(text) => {
                if let Err(e) = std::fs::write(&path, text) {
                    tracing::warn!("could not save UI settings to {}: {e}", path.display());
                }
            }
            Err(e) => tracing::warn!("could not encode UI settings: {e}"),
        }
    }

    /// A remembered panel size, or `default` the first time.
    pub fn size(&self, layout: LayoutPreset, panel: &str, default: f32) -> f32 {
        self.sizes
            .get(&format!("{}.{panel}", layout.key()))
            .copied()
            .unwrap_or(default)
    }

    /// Remember a panel size. Returns true when it actually changed.
    pub fn set_size(&mut self, layout: LayoutPreset, panel: &str, value: f32) -> bool {
        let key = format!("{}.{panel}", layout.key());
        let old = self.sizes.get(&key).copied();
        if old.is_some_and(|o| (o - value).abs() < 0.5) {
            return false;
        }
        self.sizes.insert(key, value.round());
        true
    }

    /// Put a project file at the top of the recent list.
    pub fn remember_recent(&mut self, path: &std::path::Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(12);
    }

    /// Forget the sizes of one layout ("Reset layout").
    pub fn reset_layout(&mut self, layout: LayoutPreset) {
        let prefix = format!("{}.", layout.key());
        self.sizes.retain(|k, _| !k.starts_with(&prefix));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_remembered_per_layout() {
        let mut s = UiSettings::default();
        assert_eq!(s.size(LayoutPreset::Default, "left", 380.0), 380.0);
        assert!(s.set_size(LayoutPreset::Default, "left", 420.2));
        assert!(!s.set_size(LayoutPreset::Default, "left", 420.4));
        assert_eq!(s.size(LayoutPreset::Default, "left", 380.0), 420.0);
        assert_eq!(s.size(LayoutPreset::PlayerRight, "left", 380.0), 380.0);
    }

    #[test]
    fn reset_only_touches_one_layout() {
        let mut s = UiSettings::default();
        s.set_size(LayoutPreset::Default, "left", 500.0);
        s.set_size(LayoutPreset::BigTimeline, "left", 510.0);
        s.reset_layout(LayoutPreset::Default);
        assert_eq!(s.size(LayoutPreset::Default, "left", 380.0), 380.0);
        assert_eq!(s.size(LayoutPreset::BigTimeline, "left", 380.0), 510.0);
    }

    #[test]
    fn settings_round_trip_as_json() {
        let mut s = UiSettings {
            layout: LayoutPreset::PlayerRight,
            ..Default::default()
        };
        s.set_size(LayoutPreset::PlayerRight, "timeline", 333.0);
        let back: UiSettings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.layout, LayoutPreset::PlayerRight);
        assert_eq!(back.size(LayoutPreset::PlayerRight, "timeline", 0.0), 333.0);
    }

    #[test]
    fn keys_round_trip() {
        for p in LayoutPreset::ALL {
            assert_eq!(LayoutPreset::from_key(p.key()), Some(p));
        }
    }
}
