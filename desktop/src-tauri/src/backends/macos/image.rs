use bridgething_companion::backend::ImageScaler;
use objc2_core_foundation::{
  CFData, CFDictionary, CFMutableData, CFNumber, CFRetained, CFString, CFType, kCFBooleanTrue,
};
use objc2_image_io::{
  CGImageDestination, CGImageSource, kCGImageDestinationLossyCompressionQuality,
  kCGImageSourceCreateThumbnailFromImageAlways, kCGImageSourceCreateThumbnailWithTransform,
  kCGImageSourceThumbnailMaxPixelSize,
};
use objc2_uniform_type_identifiers::UTTypeJPEG;

type Options = CFRetained<CFDictionary<CFString, CFType>>;

pub struct ImageIoScaler;

impl ImageScaler for ImageIoScaler {
  fn downsample_jpeg(&self, bytes: Vec<u8>, max_edge: u32, quality: f32) -> Option<Vec<u8>> {
    let data = CFData::from_bytes(&bytes);
    let source = unsafe { CGImageSource::with_data(&data, None) }?;

    let always = unsafe { kCFBooleanTrue }?;
    let edge = CFNumber::new_i64(i64::from(max_edge));
    let thumbnail = dictionary(&[
      (unsafe { kCGImageSourceCreateThumbnailFromImageAlways }, always),
      (unsafe { kCGImageSourceCreateThumbnailWithTransform }, always),
      (unsafe { kCGImageSourceThumbnailMaxPixelSize }, &edge),
    ]);
    let image = unsafe { source.thumbnail_at_index(0, Some(thumbnail.as_opaque())) }?;

    let out = CFMutableData::new(None, 0)?;
    let kind = CFString::from_str(&unsafe { UTTypeJPEG }.identifier().to_string());
    let destination = unsafe { CGImageDestination::with_data(&out, &kind, 1, None) }?;

    let compression = CFNumber::new_f64(f64::from(quality.clamp(0.0, 1.0)));
    let encode = dictionary(&[(unsafe { kCGImageDestinationLossyCompressionQuality }, &compression)]);
    unsafe { destination.add_image(&image, Some(encode.as_opaque())) };
    unsafe { destination.finalize() }.then(|| out.to_vec())
  }
}

fn dictionary(entries: &[(&CFString, &CFType)]) -> Options {
  let keys: Vec<&CFString> = entries.iter().map(|(key, _)| *key).collect();
  let values: Vec<&CFType> = entries.iter().map(|(_, value)| *value).collect();
  CFDictionary::from_slices(&keys, &values)
}
