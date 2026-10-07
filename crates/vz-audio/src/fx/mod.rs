//! Sound processing: equaliser, dynamics, noise reduction, voice effects, loudness and beat finding.
//! Everything works on interleaved stereo `f32`.

pub mod beats;
pub mod biquad;
pub mod denoise;
pub mod dynamics;
pub mod eq;
pub mod fft;
pub mod loudness;
pub mod voice;

use vz_core::AudioFx;

/// The effects of one clip, in the order they are heard:
/// noise reduction, equaliser, de-esser, compressor, voice effect.
pub struct Chain {
    denoise: Option<denoise::Denoiser>,
    eq: Option<eq::Equaliser>,
    deess: Option<dynamics::DeEsser>,
    compressor: Option<dynamics::Compressor>,
    voice: Option<voice::VoiceFx>,
}

impl Chain {
    pub fn new(rate: u32, fx: &AudioFx) -> Self {
        Self {
            denoise: fx
                .denoise
                .as_ref()
                .filter(|d| d.amount > 0.0 && d.profile.len() == denoise::BINS)
                .map(|d| {
                    denoise::Denoiser::new(d.amount, &denoise::profile_for_rate(&d.profile, rate))
                }),
            eq: (!fx.eq.is_flat()).then(|| eq::Equaliser::new(rate, &fx.eq)),
            deess: (fx.deess > 0.0).then(|| dynamics::DeEsser::new(rate, fx.deess)),
            compressor: fx
                .compressor
                .as_ref()
                .map(|c| dynamics::Compressor::new(rate, c)),
            voice: fx.voice.as_ref().map(|v| voice::VoiceFx::new(rate, v)),
        }
    }

    pub fn process(&mut self, samples: &mut [f32]) {
        if let Some(d) = &mut self.denoise {
            d.process(samples);
        }
        if let Some(e) = &mut self.eq {
            e.process(samples);
        }
        if let Some(d) = &mut self.deess {
            d.process(samples);
        }
        if let Some(c) = &mut self.compressor {
            c.process(samples);
        }
        if let Some(v) = &mut self.voice {
            v.process(samples);
        }
    }
}
