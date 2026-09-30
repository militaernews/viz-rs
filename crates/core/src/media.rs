use crate::error::CoreError;
use image::{DynamicImage, ImageReader, Limits};
use std::io::Cursor;

const MAX_DECODE_DIMENSION: u32 = 16_384;
const MAX_DECODE_ALLOC: u64 = 512 * 1024 * 1024;

/// JPEG, PNG, GIF or WebP, judged by magic bytes only (never by a declared content type).
pub fn is_image(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFF, 0xD8, 0xFF])
        || bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(b"GIF87a")
        || bytes.starts_with(b"GIF89a")
        || (bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP")
}

/// MP4/MOV (ISO BMFF), Matroska/WebM or AVI, judged by magic bytes only.
pub fn is_video(bytes: &[u8]) -> bool {
    (bytes.len() >= 12 && &bytes[4..8] == b"ftyp" && !is_heif_brand(&bytes[8..12]))
        || bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3])
        || (bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"AVI ")
}

fn is_heif_brand(brand: &[u8]) -> bool {
    matches!(brand, b"heic" | b"heix" | b"mif1" | b"msf1" | b"avif" | b"avis")
}

/// Decodes an image after checking its magic bytes, with dimension/allocation limits so a
/// small upload can't expand into gigabytes of pixels.
pub fn decode_image(bytes: &[u8]) -> Result<DynamicImage, CoreError> {
    if !is_image(bytes) {
        return Err(CoreError::InvalidMedia("not a supported image format".into()));
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DECODE_DIMENSION);
    limits.max_image_height = Some(MAX_DECODE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);

    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| CoreError::InvalidMedia(e.to_string()))?;
    reader.limits(limits);
    reader.decode().map_err(|e| CoreError::InvalidMedia(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes() -> Vec<u8> {
        let mut out = Vec::new();
        DynamicImage::new_rgb8(4, 4).write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
        out
    }

    #[test]
    fn recognises_magic_bytes() {
        assert!(is_image(&png_bytes()));
        assert!(is_image(b"\xFF\xD8\xFF\xE0rest"));
        assert!(is_image(b"RIFF\0\0\0\0WEBPVP8 "));
        assert!(!is_image(b"RIFF\0\0\0\0AVI LIST"));
        assert!(!is_image(b"<svg></svg>"));

        assert!(is_video(b"\0\0\0\x20ftypisom\0\0\x02\0"));
        assert!(is_video(b"\x1A\x45\xDF\xA3\x01\0\0\0"));
        assert!(is_video(b"RIFF\0\0\0\0AVI LIST"));
        assert!(!is_video(b"\0\0\0\x18ftypheic\0\0\0\0"));
        assert!(!is_video(&png_bytes()));
        assert!(!is_video(b"short"));
    }

    #[test]
    fn decode_rejects_non_images() {
        assert!(decode_image(&png_bytes()).is_ok());
        assert!(matches!(decode_image(b"#!/bin/sh"), Err(CoreError::InvalidMedia(_))));
        let mut truncated = png_bytes();
        truncated.truncate(20);
        assert!(matches!(decode_image(&truncated), Err(CoreError::InvalidMedia(_))));
    }
}
