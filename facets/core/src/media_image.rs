//! Pictures that are uploaded are decoded, shrunk and written out again.
//!
//! * **Decoding proves it is a picture**, not just something that starts with the right
//!   bytes, and is done under size limits so a small file cannot expand into gigabytes.
//! * **Writing it again drops what the camera or editor left in**: location, device,
//!   author, an embedded thumbnail. A photo uploaded to a record should not carry the
//!   place it was taken.
//! * **Shrinking** keeps a record's picture at a size a screen can use (at most
//!   [`MAX_EDGE`] on the long side), and a small copy ([`THUMB_EDGE`]) is made for lists.
//!
//! Animated GIFs are kept as uploaded (writing one again would keep only its first
//! frame); WebP pictures that are already small are kept as uploaded too, because the
//! writer available here is lossless only and would make them larger.

use std::io::Cursor;

use image::{
    DynamicImage, ImageDecoder, ImageEncoder, ImageFormat, ImageReader, Limits,
    codecs::jpeg::JpegEncoder, imageops::FilterType,
};

use crate::data_model::media::Kind;

/// Longest side of the stored picture, in pixels.
pub const MAX_EDGE: u32 = 2048;
/// Longest side of the list thumbnail, in pixels.
pub const THUMB_EDGE: u32 = 256;
/// What a picture may declare as its size before decoding is refused.
const MAX_DIMENSION: u32 = 16_384;
/// Most memory decoding may use.
const MAX_ALLOC: u64 = 256 * 1024 * 1024;
const JPEG_QUALITY: u8 = 90;
const THUMB_QUALITY: u8 = 80;

#[derive(Debug)]
pub struct Processed {
    /// The picture to store.
    pub bytes: Vec<u8>,
    /// What `bytes` is (a WebP that had to shrink becomes a JPEG or PNG).
    pub kind: Kind,
    /// A small copy for lists, as PNG when the picture has transparency and JPEG otherwise.
    pub thumbnail: Vec<u8>,
}

fn format_of(kind: Kind) -> Option<ImageFormat> {
    match kind {
        Kind::Png => Some(ImageFormat::Png),
        Kind::Jpeg => Some(ImageFormat::Jpeg),
        Kind::Gif => Some(ImageFormat::Gif),
        Kind::Webp => Some(ImageFormat::WebP),
        _ => None,
    }
}

/// Decode, shrink and re-encode a picture of the given sniffed `kind`.
pub fn process(bytes: &[u8], kind: Kind) -> Result<Processed, String> {
    let format = format_of(kind).ok_or("that is not a picture")?;
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);

    let mut decoder = reader
        .into_decoder()
        .map_err(|_| "that file is not a usable picture".to_string())?;
    // A photo is stored turned the way it is meant to be seen, since the turn is dropped
    // with the rest of the metadata.
    let orientation = decoder.orientation().ok();
    let mut picture = DynamicImage::from_decoder(decoder)
        .map_err(|_| "that file is not a usable picture".to_string())?;
    if let Some(orientation) = orientation {
        picture.apply_orientation(orientation);
    }

    let large = picture.width().max(picture.height()) > MAX_EDGE;
    let (stored, stored_kind) = match kind {
        Kind::Gif => (bytes.to_vec(), Kind::Gif),
        Kind::Webp if !large => (bytes.to_vec(), Kind::Webp),
        _ => {
            if large {
                picture = picture.resize(MAX_EDGE, MAX_EDGE, FilterType::Lanczos3);
            }
            // PNG stays PNG; everything else is written as JPEG, unless it has
            // transparency, which only PNG keeps.
            if kind == Kind::Png || picture.color().has_alpha() {
                (encode_png(&picture)?, Kind::Png)
            } else {
                (encode_jpeg(&picture, JPEG_QUALITY)?, Kind::Jpeg)
            }
        }
    };

    let small = picture.thumbnail(THUMB_EDGE, THUMB_EDGE);
    let thumbnail = if small.color().has_alpha() {
        encode_png(&small)?
    } else {
        encode_jpeg(&small, THUMB_QUALITY)?
    };
    Ok(Processed {
        bytes: stored,
        kind: stored_kind,
        thumbnail,
    })
}

fn encode_png(picture: &DynamicImage) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    picture
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .map_err(|error| format!("could not write the picture: {error}"))?;
    Ok(out)
}

fn encode_jpeg(picture: &DynamicImage, quality: u8) -> Result<Vec<u8>, String> {
    let rgb = picture.to_rgb8();
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality)
        .write_image(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|error| format!("could not write the picture: {error}"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_model::media::sniff;
    use image::{Rgb, RgbImage, Rgba, RgbaImage};

    fn png(width: u32, height: u32) -> Vec<u8> {
        let image = RgbImage::from_pixel(width, height, Rgb([200, 30, 30]));
        let mut out = Vec::new();
        DynamicImage::ImageRgb8(image)
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .unwrap();
        out
    }

    fn dims(bytes: &[u8]) -> (u32, u32) {
        let picture = image::load_from_memory(bytes).unwrap();
        (picture.width(), picture.height())
    }

    #[test]
    fn a_small_picture_is_kept_and_gets_a_thumbnail() {
        let original = png(300, 200);
        let done = process(&original, Kind::Png).unwrap();
        assert_eq!(done.kind, Kind::Png);
        assert_eq!(dims(&done.bytes), (300, 200));
        assert_eq!(sniff(&done.bytes), Some(Kind::Png));
        assert_eq!(dims(&done.thumbnail), (256, 171));
        assert_eq!(sniff(&done.thumbnail), Some(Kind::Jpeg));
    }

    #[test]
    fn a_large_picture_is_shrunk_to_fit() {
        let done = process(&png(4096, 1024), Kind::Png).unwrap();
        assert_eq!(dims(&done.bytes), (2048, 512));
        assert!(done.thumbnail.len() < done.bytes.len() + 4096);
    }

    #[test]
    fn jpeg_comes_back_as_jpeg() {
        let rgb = RgbImage::from_pixel(64, 64, Rgb([10, 120, 200]));
        let mut jpeg = Vec::new();
        JpegEncoder::new(&mut jpeg)
            .write_image(rgb.as_raw(), 64, 64, image::ExtendedColorType::Rgb8)
            .unwrap();
        let done = process(&jpeg, Kind::Jpeg).unwrap();
        assert_eq!(done.kind, Kind::Jpeg);
        assert_eq!(dims(&done.bytes), (64, 64));
    }

    #[test]
    fn transparency_is_kept_in_the_thumbnail() {
        let rgba = RgbaImage::from_pixel(100, 100, Rgba([0, 0, 0, 0]));
        let mut out = Vec::new();
        DynamicImage::ImageRgba8(rgba)
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .unwrap();
        let done = process(&out, Kind::Png).unwrap();
        assert_eq!(sniff(&done.thumbnail), Some(Kind::Png));
    }

    #[test]
    fn something_that_only_looks_like_a_picture_is_refused() {
        let mut fake = b"\x89PNG\r\n\x1a\n".to_vec();
        fake.extend_from_slice(&[0u8; 64]);
        assert!(process(&fake, Kind::Png).is_err());
        assert!(process(b"%PDF-1.7", Kind::Pdf).is_err());
    }

    #[test]
    fn a_picture_that_declares_a_huge_size_is_refused() {
        // 20000 x 20000 is over the limit whatever the file's own size.
        let huge = RgbImage::from_pixel(20_000, 8, Rgb([0, 0, 0]));
        let mut out = Vec::new();
        DynamicImage::ImageRgb8(huge)
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .unwrap();
        assert!(process(&out, Kind::Png).is_err());
    }
}
