//! A tiny WAV file writer (16-bit), used for voice-overs, extracted sound and our own sound effects.

use std::fs::File;
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

pub struct WavWriter {
    out: BufWriter<File>,
    channels: u16,
    rate: u32,
    data_bytes: u64,
}

impl WavWriter {
    pub fn create(path: &Path, rate: u32, channels: u16) -> io::Result<Self> {
        let mut w = Self {
            out: BufWriter::new(File::create(path)?),
            channels,
            rate,
            data_bytes: 0,
        };
        w.header()?;
        Ok(w)
    }

    fn header(&mut self) -> io::Result<()> {
        let data = self.data_bytes.min(u64::from(u32::MAX - 36)) as u32;
        let block = self.channels * 2;
        let o = &mut self.out;
        o.write_all(b"RIFF")?;
        o.write_all(&(36 + data).to_le_bytes())?;
        o.write_all(b"WAVEfmt ")?;
        o.write_all(&16u32.to_le_bytes())?;
        o.write_all(&1u16.to_le_bytes())?; // PCM
        o.write_all(&self.channels.to_le_bytes())?;
        o.write_all(&self.rate.to_le_bytes())?;
        o.write_all(&(self.rate * u32::from(block)).to_le_bytes())?;
        o.write_all(&block.to_le_bytes())?;
        o.write_all(&16u16.to_le_bytes())?;
        o.write_all(b"data")?;
        o.write_all(&data.to_le_bytes())
    }

    /// Append interleaved samples (-1.0 to 1.0).
    pub fn write(&mut self, samples: &[f32]) -> io::Result<()> {
        for s in samples {
            let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
            self.out.write_all(&v.to_le_bytes())?;
        }
        self.data_bytes += samples.len() as u64 * 2;
        Ok(())
    }

    /// Frames (one sample per channel) written so far.
    pub fn frames(&self) -> u64 {
        self.data_bytes / (2 * u64::from(self.channels.max(1)))
    }

    /// Fix the sizes in the header and close the file.
    pub fn finish(mut self) -> io::Result<()> {
        self.out.flush()?;
        self.out.seek(SeekFrom::Start(0))?;
        self.header()?;
        self.out.flush()
    }
}

/// Write a whole buffer in one go.
pub fn write_wav(path: &Path, rate: u32, channels: u16, samples: &[f32]) -> io::Result<()> {
    let mut w = WavWriter::create(path, rate, channels)?;
    w.write(samples)?;
    w.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_file_has_a_correct_header_and_size() {
        let dir = std::env::temp_dir().join("vidzcut-wav-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.wav");
        let samples: Vec<f32> = (0..2000).map(|i| (i as f32 / 100.0).sin()).collect();
        write_wav(&path, 44_100, 2, &samples).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..16], b"WAVEfmt ");
        assert_eq!(bytes.len(), 44 + 4000);
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            36 + 4000
        );
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 4000);
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            44_100
        );
    }
}
