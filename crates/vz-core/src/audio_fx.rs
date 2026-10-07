//! Sound settings of a clip: loudness, noise reduction, equaliser, compressor, voice effects, ducking.
//! The sound itself is made in `vz-audio`; this is only the data that is saved with the project.

use crate::time::Micros;
use serde::{Deserialize, Serialize};

/// Which channels of the clip are heard.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelMode {
    #[default]
    Stereo,
    Left,
    Right,
    Mono,
}

impl ChannelMode {
    pub const ALL: [Self; 4] = [Self::Stereo, Self::Left, Self::Right, Self::Mono];

    pub fn label(self) -> &'static str {
        match self {
            Self::Stereo => "Stereo",
            Self::Left => "Left only",
            Self::Right => "Right only",
            Self::Mono => "Mono",
        }
    }
}

/// Make the clip as loud as a target. `measured_lufs` was measured on the clip's own part of the file.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Normalize {
    pub target_lufs: f32,
    pub measured_lufs: f32,
}

impl Normalize {
    /// Loudness targets people ask for: social media / streaming, TV, quiet music.
    pub const SOCIAL: f32 = -14.0;
    pub const TV: f32 = -23.0;

    /// The gain (in decibels) that brings the clip to the target. Limited so a silent clip is not blown up.
    pub fn gain_db(&self) -> f32 {
        (self.target_lufs - self.measured_lufs).clamp(-40.0, 30.0)
    }
}

/// Noise reduction (spectral gate). `profile` is the measured noise level of each frequency bin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Denoise {
    /// 0.0 to 1.0.
    pub amount: f32,
    pub profile: Vec<f32>,
}

/// Number of equaliser bands.
pub const EQ_BANDS: usize = 5;
/// Middle frequency of every equaliser band (the first is a low shelf, the last a high shelf).
pub const EQ_FREQS: [f32; EQ_BANDS] = [100.0, 350.0, 1000.0, 3500.0, 10000.0];

/// A five-band equaliser; gains in decibels (-12 to +12).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Eq {
    pub gains: [f32; EQ_BANDS],
}

impl Eq {
    pub fn is_flat(&self) -> bool {
        self.gains.iter().all(|g| g.abs() < 0.05)
    }
}

/// Ready-made equaliser settings: (name, five gains).
pub const EQ_PRESETS: [(&str, [f32; EQ_BANDS]); 8] = [
    ("Flat", [0.0, 0.0, 0.0, 0.0, 0.0]),
    ("Voice boost", [-3.0, 0.0, 2.0, 4.0, 1.0]),
    ("Warm", [4.0, 2.0, 0.0, -2.0, -3.0]),
    ("Bright", [-2.0, -1.0, 0.0, 3.0, 5.0]),
    ("Bass boost", [7.0, 3.0, 0.0, 0.0, 0.0]),
    ("Treble boost", [0.0, 0.0, 0.0, 4.0, 7.0]),
    ("Less mud", [0.0, -5.0, 0.0, 1.0, 1.0]),
    ("Small speaker", [-8.0, -2.0, 3.0, 2.0, -6.0]),
];

/// Compressor: makes loud parts quieter so the level is more even.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Compressor {
    pub threshold_db: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub makeup_db: f32,
    /// Also stop the clip going above -1 dB.
    pub limit: bool,
}

impl Default for Compressor {
    fn default() -> Self {
        Self {
            threshold_db: -18.0,
            ratio: 3.0,
            attack_ms: 10.0,
            release_ms: 120.0,
            makeup_db: 3.0,
            limit: true,
        }
    }
}

/// Changes to the voice, made with ordinary sound processing (no AI).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoiceEffect {
    Chipmunk,
    Deep,
    Robot,
    Echo,
    Room,
    Hall,
    Telephone,
    Megaphone,
    Radio,
    Underwater,
    Vibrato,
}

impl VoiceEffect {
    pub const ALL: [Self; 11] = [
        Self::Chipmunk,
        Self::Deep,
        Self::Robot,
        Self::Echo,
        Self::Room,
        Self::Hall,
        Self::Telephone,
        Self::Megaphone,
        Self::Radio,
        Self::Underwater,
        Self::Vibrato,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Chipmunk => "Chipmunk",
            Self::Deep => "Deep voice",
            Self::Robot => "Robot",
            Self::Echo => "Echo",
            Self::Room => "Room",
            Self::Hall => "Hall",
            Self::Telephone => "Telephone",
            Self::Megaphone => "Megaphone",
            Self::Radio => "Radio",
            Self::Underwater => "Underwater",
            Self::Vibrato => "Vibrato",
        }
    }
}

/// A voice effect and how strong it is (0.0 to 1.0).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Voice {
    pub effect: VoiceEffect,
    pub amount: f32,
}

/// Lower this clip while other sound (for example a voice) is playing.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Duck {
    /// How many decibels to lower (a positive number).
    pub amount_db: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
}

impl Default for Duck {
    fn default() -> Self {
        Self {
            amount_db: 12.0,
            attack_ms: 80.0,
            release_ms: 500.0,
        }
    }
}

/// Everything that changes the sound of a clip. All off by default.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AudioFx {
    /// -1.0 (left) to 1.0 (right).
    #[serde(default)]
    pub pan: f32,
    #[serde(default)]
    pub channel: ChannelMode,
    #[serde(default)]
    pub normalize: Option<Normalize>,
    #[serde(default)]
    pub denoise: Option<Denoise>,
    #[serde(default)]
    pub eq: Eq,
    #[serde(default)]
    pub compressor: Option<Compressor>,
    /// De-esser strength, 0.0 (off) to 1.0.
    #[serde(default)]
    pub deess: f32,
    #[serde(default)]
    pub voice: Option<Voice>,
    #[serde(default)]
    pub duck: Option<Duck>,
    /// Beats found in the file, as times inside the media file.
    #[serde(default)]
    pub beats: Vec<Micros>,
}

impl AudioFx {
    /// True when the clip's sound is only changed by volume and fades.
    pub fn is_plain(&self) -> bool {
        self.pan == 0.0
            && self.channel == ChannelMode::Stereo
            && self.normalize.is_none()
            && self.denoise.is_none()
            && self.eq.is_flat()
            && self.compressor.is_none()
            && self.deess <= 0.0
            && self.voice.is_none()
            && self.duck.is_none()
    }

    /// Does the sound need the processing chain (anything but gain, pan and channels)?
    pub fn needs_chain(&self) -> bool {
        self.denoise.is_some()
            || !self.eq.is_flat()
            || self.compressor.is_some()
            || self.deess > 0.0
            || self.voice.is_some()
    }

    /// The loudness gain as a factor (1.0 when not normalising).
    pub fn normalize_factor(&self) -> f32 {
        self.normalize
            .map_or(1.0, |n| 10f32.powf(n.gain_db() / 20.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_plain_and_round_trips() {
        let fx = AudioFx::default();
        assert!(fx.is_plain());
        assert!(!fx.needs_chain());
        let json = serde_json::to_string(&fx).unwrap();
        assert_eq!(serde_json::from_str::<AudioFx>(&json).unwrap(), fx);
        // an old project without the field still loads
        assert_eq!(serde_json::from_str::<AudioFx>("{}").unwrap(), fx);
    }

    #[test]
    fn normalize_gain_is_the_distance_to_the_target() {
        let n = Normalize {
            target_lufs: -14.0,
            measured_lufs: -20.0,
        };
        assert!((n.gain_db() - 6.0).abs() < 1e-6);
        let fx = AudioFx {
            normalize: Some(n),
            ..AudioFx::default()
        };
        assert!((fx.normalize_factor() - 1.995_262_3).abs() < 1e-4);
        // silence is not boosted without limit
        let silent = Normalize {
            target_lufs: -14.0,
            measured_lufs: -120.0,
        };
        assert_eq!(silent.gain_db(), 30.0);
    }

    #[test]
    fn effects_change_what_the_chain_needs() {
        let mut fx = AudioFx::default();
        fx.eq.gains[0] = 4.0;
        assert!(fx.needs_chain());
        assert!(!fx.is_plain());
        let fx = AudioFx {
            pan: 0.5,
            ..AudioFx::default()
        };
        assert!(!fx.is_plain());
        assert!(!fx.needs_chain());
    }
}
