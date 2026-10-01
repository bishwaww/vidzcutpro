use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("could not open {path}: {message}")]
    Open { path: PathBuf, message: String },
    #[error("{0} has no video or audio that vidzcut can read")]
    NothingToRead(String),
    #[error("could not decode: {0}")]
    Decode(String),
    #[error("FFmpeg error: {0}")]
    Ffmpeg(#[from] ffmpeg_next::Error),
    #[error("image error: {0}")]
    Image(#[from] image::ImageError),
}

pub type Result<T> = std::result::Result<T, MediaError>;
