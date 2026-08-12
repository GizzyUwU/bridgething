use std::io::Cursor;

use bridgething_companion::backend::ImageScaler;
use image::{DynamicImage, ImageDecoder, ImageReader, codecs::jpeg::JpegEncoder, imageops::FilterType};

const QUALITY_FLOOR: u8 = 1;
const QUALITY_CEILING: u8 = 100;

pub struct PortableScaler;

impl ImageScaler for PortableScaler {
  fn downsample_jpeg(&self, bytes: Vec<u8>, max_edge: u32, quality: f32) -> Option<Vec<u8>> {
    let mut decoder = ImageReader::new(Cursor::new(bytes))
      .with_guessed_format()
      .ok()?
      .into_decoder()
      .ok()?;
    let orientation = decoder.orientation().ok()?;
    let mut decoded = DynamicImage::from_decoder(decoder).ok()?;
    decoded.apply_orientation(orientation);

    let edge = decoded.width().max(decoded.height());
    let scaled = if max_edge > 0 && edge > max_edge {
      decoded.resize(max_edge, max_edge, FilterType::Lanczos3)
    } else {
      decoded
    };

    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, jpeg_quality(quality))
      .encode_image(&scaled.into_rgb8())
      .ok()?;
    Some(out)
  }
}

fn jpeg_quality(quality: f32) -> u8 {
  ((quality.clamp(0.0, 1.0) * f32::from(QUALITY_CEILING)).round() as u8).clamp(QUALITY_FLOOR, QUALITY_CEILING)
}
