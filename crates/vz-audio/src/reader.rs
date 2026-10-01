//! Reading sound out of a media file as stereo `f32` at any sample rate.

use ff::format::sample::{Sample, Type as SampleType};
use ff::media::Type;
use ff::software::resampling::Context as Resampler;
use ffmpeg_next as ff;
use std::collections::VecDeque;
use std::path::Path;
use vz_core::Micros;

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("could not open {path}: {message}")]
    Open { path: String, message: String },
    #[error("{0} has no sound")]
    NoAudio(String),
    #[error("audio decode error: {0}")]
    Decode(#[from] ff::Error),
    #[error("no sound device available: {0}")]
    Device(String),
}

/// How far ahead of what we have (in seconds) a request may be before we seek instead of reading on.
const SEEK_AHEAD_SECONDS: i64 = 2;

pub struct AudioReader {
    input: ff::format::context::Input,
    stream_index: usize,
    decoder: ff::decoder::Audio,
    resampler: Resampler,
    time_base: (i32, i32),
    /// Time of the first sample in the file, in microseconds (subtracted so media time starts at 0).
    origin: Micros,
    rate: u32,
    /// Decoded stereo samples (interleaved) that have not been used yet.
    buffer: VecDeque<f32>,
    /// File sample index of `buffer[0]` (a frame is two floats).
    buffer_start: i64,
    /// True right after a seek: the next decoded frame says where we really are.
    needs_position: bool,
    ended: bool,
    /// False until the first seek or read has told us where in the file we are.
    positioned: bool,
}

fn new_resampler(decoder: &ff::decoder::Audio, rate: u32) -> Result<Resampler, ff::Error> {
    Resampler::get(
        decoder.format(),
        decoder.channel_layout(),
        decoder.rate(),
        Sample::F32(SampleType::Packed),
        ff::ChannelLayout::STEREO,
        rate,
    )
}

impl AudioReader {
    pub fn open(path: &Path, rate: u32) -> Result<Self, AudioError> {
        vz_media::init()?;
        let input = ff::format::input(&path).map_err(|e| AudioError::Open {
            path: path.display().to_string(),
            message: e.to_string(),
        })?;
        let stream = input
            .streams()
            .best(Type::Audio)
            .ok_or_else(|| AudioError::NoAudio(path.display().to_string()))?;
        let stream_index = stream.index();
        let tb = stream.time_base();
        let origin = if stream.start_time() == ff::ffi::AV_NOPTS_VALUE {
            0
        } else {
            (i128::from(stream.start_time()) * 1_000_000 * i128::from(tb.numerator())
                / i128::from(tb.denominator().max(1))) as Micros
        };
        let context = ff::codec::context::Context::from_parameters(stream.parameters())?;
        let mut decoder = context.decoder().audio()?;
        if decoder.channel_layout().is_empty() {
            decoder.set_channel_layout(ff::ChannelLayout::default(i32::from(decoder.channels())));
        }
        let resampler = new_resampler(&decoder, rate)?;
        Ok(Self {
            input,
            stream_index,
            decoder,
            resampler,
            time_base: (tb.numerator(), tb.denominator()),
            origin,
            rate,
            buffer: VecDeque::new(),
            buffer_start: 0,
            needs_position: true,
            ended: false,
            positioned: false,
        })
    }

    fn buffer_end(&self) -> i64 {
        self.buffer_start + (self.buffer.len() / 2) as i64
    }

    fn samples_at(&self, pts: i64) -> i64 {
        let us = (i128::from(pts) * 1_000_000 * i128::from(self.time_base.0)
            / i128::from(self.time_base.1.max(1))) as i64
            - self.origin;
        (i128::from(us) * i128::from(self.rate) / 1_000_000) as i64
    }

    fn seek(&mut self, sample: i64) -> Result<(), AudioError> {
        let us =
            (i128::from(sample.max(0)) * 1_000_000 / i128::from(self.rate)) as i64 + self.origin;
        self.input.seek(us, ..us + 1)?;
        self.decoder.flush();
        // a fresh resampler, so nothing from before the jump leaks into the sound
        self.resampler = new_resampler(&self.decoder, self.rate)?;
        self.buffer.clear();
        self.buffer_start = sample;
        self.needs_position = true;
        self.ended = false;
        self.positioned = true;
        Ok(())
    }

    /// Decode some more sound into the buffer. Returns false when the file has ended.
    fn decode_more(&mut self) -> Result<bool, AudioError> {
        let mut frame = ff::frame::Audio::empty();
        loop {
            if self.decoder.receive_frame(&mut frame).is_ok() {
                if frame.channel_layout().is_empty() {
                    frame.set_channel_layout(self.decoder.channel_layout());
                }
                if self.needs_position {
                    if let Some(pts) = frame.pts() {
                        self.buffer_start = self.samples_at(pts);
                    }
                    self.needs_position = false;
                }
                let mut out = ff::frame::Audio::empty();
                self.resampler.run(&frame, &mut out)?;
                self.push(&out);
                return Ok(true);
            }
            if self.ended {
                return Ok(false);
            }
            let mut sent = false;
            for (stream, packet) in self.input.packets() {
                if stream.index() == self.stream_index {
                    if self.decoder.send_packet(&packet).is_err() {
                        continue; // a damaged packet should not stop the whole clip
                    }
                    sent = true;
                    break;
                }
            }
            if !sent {
                self.ended = true;
                let _ = self.decoder.send_eof();
            }
        }
    }

    fn push(&mut self, frame: &ff::frame::Audio) {
        let n = frame.samples();
        if n == 0 {
            return;
        }
        let data = frame.plane::<f32>(0);
        self.buffer.extend(&data[..(n * 2).min(data.len())]);
    }

    /// Fill `out` (interleaved stereo, `out.len() / 2` frames) with the sound starting at file sample
    /// `start`. Anything the file does not have is silence.
    pub fn read(&mut self, start: i64, out: &mut [f32]) -> Result<(), AudioError> {
        let frames = (out.len() / 2) as i64;
        let end = start + frames;
        let jumped_back = start < self.buffer_start;
        let jumped_far = start > self.buffer_end() + SEEK_AHEAD_SECONDS * i64::from(self.rate);
        if !self.positioned || jumped_back || jumped_far {
            self.seek(start)?;
        }
        while self.buffer_end() < end && !self.ended {
            if !self.decode_more()? {
                break;
            }
        }
        // throw away sound before `start`
        if start > self.buffer_start {
            let drop = (((start - self.buffer_start) * 2) as usize).min(self.buffer.len());
            self.buffer.drain(..drop);
            self.buffer_start += (drop / 2) as i64;
        }
        out.fill(0.0);
        // the sound may begin later than asked for (a seek landed late, or the file starts late)
        let offset = (self.buffer_start - start).clamp(0, frames) as usize;
        let take = self.buffer.len().min((frames as usize - offset) * 2);
        for (slot, v) in out[offset * 2..].iter_mut().zip(self.buffer.drain(..take)) {
            *slot = v;
        }
        self.buffer_start += (take / 2) as i64;
        Ok(())
    }
}
