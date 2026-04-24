use std::path::Path;
use std::sync::Arc;

use crate::capacity::{CHANNELS, COVER_BYTES, COVER_H, COVER_W};
use crate::{Error, Result};

/// Embedded cover image bytes (the one true monke).
pub const MONKE_PNG: &[u8] = include_bytes!("../../../monke.png");

/// Decoded cover image: raw 8-bit RGBA pixel bytes, dimensions.
#[derive(Clone)]
pub struct CoverImage {
    pub width: u32,
    pub height: u32,
    pub raw_rgba: Arc<[u8]>,
}

impl CoverImage {
    /// Load and decode the embedded monke.png.
    pub fn monke_default() -> Result<Self> {
        Self::from_png_bytes(MONKE_PNG)
    }

    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        Self::from_png_bytes(&bytes)
    }

    pub fn from_png_bytes(bytes: &[u8]) -> Result<Self> {
        let decoder = png::Decoder::new(bytes);
        let mut reader = decoder.read_info()?;
        let info = reader.info();
        if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
            return Err(Error::UnsupportedCover(format!(
                "need 8-bit RGBA; got {:?} / {:?}",
                info.color_type, info.bit_depth
            )));
        }
        let width = info.width;
        let height = info.height;
        if width != COVER_W || height != COVER_H {
            return Err(Error::UnsupportedCover(format!(
                "cover must be {}x{}, got {}x{}",
                COVER_W, COVER_H, width, height
            )));
        }
        let mut buf = vec![0u8; reader.output_buffer_size()];
        reader.next_frame(&mut buf)?;
        if buf.len() != COVER_BYTES {
            return Err(Error::UnsupportedCover(format!(
                "decoded {} bytes, expected {}",
                buf.len(),
                COVER_BYTES
            )));
        }
        Ok(Self {
            width,
            height,
            raw_rgba: Arc::from(buf),
        })
    }

    pub fn channels(&self) -> usize {
        CHANNELS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_default_monke() {
        let cover = CoverImage::monke_default().expect("monke loads");
        assert_eq!(cover.width, COVER_W);
        assert_eq!(cover.height, COVER_H);
        assert_eq!(cover.raw_rgba.len(), COVER_BYTES);
    }
}
