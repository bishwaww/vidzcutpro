//! Saving one picture of the timeline (the "current frame") as PNG or JPG.

use crate::encode::ExportError;
use std::path::Path;
use vz_core::{Micros, Project};
use vz_render::Renderer;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PictureFormat {
    #[default]
    Png,
    Jpg,
}

impl PictureFormat {
    pub const ALL: [Self; 2] = [Self::Png, Self::Jpg];

    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG (sharp, bigger)",
            Self::Jpg => "JPG (smaller)",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpg => "jpg",
        }
    }
}

/// Draw the project at time `t` and save it at `size`.
pub fn save_picture(
    project: &Project,
    t: Micros,
    size: (u32, u32),
    format: PictureFormat,
    path: &Path,
) -> Result<(), ExportError> {
    let picture = Renderer::new().render(project, t, size);
    let (w, h) = size;
    match format {
        PictureFormat::Png => {
            image::save_buffer(path, &picture.rgba, w, h, image::ColorType::Rgba8)
                .map_err(|e| ExportError::Message(e.to_string()))
        }
        PictureFormat::Jpg => {
            let rgb: Vec<u8> = picture
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2]])
                .collect();
            let file = std::io::BufWriter::new(std::fs::File::create(path)?);
            image::codecs::jpeg::JpegEncoder::new_with_quality(file, 92)
                .encode(&rgb, w, h, image::ExtendedColorType::Rgb8)
                .map_err(|e| ExportError::Message(e.to_string()))
        }
    }
}
