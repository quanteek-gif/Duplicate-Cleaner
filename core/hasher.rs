//! Hashing: streaming SHA-256, quick first-block hashes, perceptual image hashes.

use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::utils::error::{AppError, AppResult};
use crate::utils::file_utils::open_file;

/// Streaming block size for SHA-256 (never loads whole files into RAM).
pub const HASH_BLOCK_SIZE: usize = 1024 * 1024; // 1 MiB

/// Bytes read for the quick pre-filter hash.
pub const QUICK_HASH_BYTES: u64 = 64 * 1024; // 64 KiB

/// Compute the SHA-256 of a whole file by streaming it in 1 MiB blocks.
///
/// Returns the lower-case hex digest and the number of bytes read.
pub fn hash_file(path: &Path) -> AppResult<(String, u64)> {
    let mut file = open_file(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; HASH_BLOCK_SIZE];
    let mut total: u64 = 0;

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        total += read as u64;
    }

    Ok((hex(&hasher.finalize()), total))
}

/// Result of the cheap pre-filter hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickHash {
    /// 64-bit fingerprint of the first [64 KiB] of the file.
    pub fingerprint: u64,
    /// Full SHA-256 hex digest when the file is smaller than the quick-hash
    /// window (the whole file was read, so no second pass is needed).
    pub full_sha256: Option<String>,
}

/// Hash the first [`QUICK_HASH_BYTES`] of a file. Used to discard files with
/// equal size but different beginnings before paying for a full SHA-256.
///
/// Files that fit into the window are hashed completely and returned with
/// `full_sha256` set, so the engine never reads them twice.
pub fn quick_hash(path: &Path) -> AppResult<QuickHash> {
    let mut file = open_file(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; QUICK_HASH_BYTES as usize];
    let mut read_total = 0usize;
    let mut complete = false;
    while read_total < buffer.len() {
        let read = file.read(&mut buffer[read_total..])?;
        if read == 0 {
            complete = true;
            break;
        }
        read_total += read;
    }
    hasher.update(&buffer[..read_total]);
    let digest = hasher.finalize();
    Ok(QuickHash {
        fingerprint: streaming_fold(&digest),
        full_sha256: if complete { Some(hex(&digest)) } else { None },
    })
}

fn streaming_fold(digest: &[u8]) -> u64 {
    let mut value = 0u64;
    for (index, byte) in digest.iter().take(8).enumerate() {
        value |= (*byte as u64) << (index * 8);
    }
    value
}

/// Verify that a file still has the expected SHA-256 digest.
pub fn verify_hash(path: &Path, expected_hex: &str) -> AppResult<String> {
    let (actual, _) = hash_file(path)?;
    if actual.eq_ignore_ascii_case(expected_hex) {
        Ok(actual)
    } else {
        Err(AppError::other(format!(
            "file content changed since the scan: expected {expected_hex}, got {actual}"
        )))
    }
}

fn hex(bytes: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(TABLE[(byte >> 4) as usize] as char);
        out.push(TABLE[(byte & 0x0f) as usize] as char);
    }
    out
}

// ---------------------------------------------------------------------------
// Perceptual hashing (pHash) for photos
// ---------------------------------------------------------------------------

const PHASH_SIZE: usize = 32;
const PHASH_LOW_FREQ: usize = 8;
/// Long edge of the cheap pre-resize (see `perceptual_hash`).
const PHASH_PRE_SIZE: u32 = 256;

/// Compute a 64-bit perceptual hash (DCT-based pHash) of an image.
///
/// The image is decoded, converted to 32×32 grayscale, DCT-transformed and the
/// top-left 8×8 low-frequency coefficients (DC excluded) are thresholded by
/// their median. Robust to resizing, re-compression and slight crops.
pub fn perceptual_hash(path: &Path) -> AppResult<u64> {
    let decoded = image::ImageReader::open(path)?
        .with_guessed_format()?
        .decode()?;

    // Downscale in two steps: large photos first to 256 px (cheap, avoids
    // aliasing), then to the 32×32 analysis grid.
    let luma = decoded.to_luma8();
    // `thumbnail(w, h)` with two equal values *stretches* the picture into a
    // square — a 4:3 photo and a 3:2 photo get warped by different amounts, and
    // a small photo (below 256 px) is not warped at all. Because the final
    // 32×32 grid is square either way, that inconsistency makes identical
    // content land on different coefficients depending on the source size.
    // Scale by the long edge instead so every image is warped identically.
    let small = {
        let (width, height) = (luma.width(), luma.height());
        let longest = width.max(height);
        if longest > PHASH_PRE_SIZE {
            let scale = PHASH_PRE_SIZE as f64 / longest as f64;
            let target_w = ((width as f64 * scale).round() as u32).max(1);
            let target_h = ((height as f64 * scale).round() as u32).max(1);
            image::imageops::thumbnail(&luma, target_w, target_h)
        } else {
            luma
        }
    };
    let resized = image::imageops::resize(
        &small,
        PHASH_SIZE as u32,
        PHASH_SIZE as u32,
        image::imageops::FilterType::Lanczos3,
    );

    let mut pixels = [0f64; PHASH_SIZE * PHASH_SIZE];
    for (index, pixel) in resized.pixels().enumerate() {
        pixels[index] = pixel.0[0] as f64;
    }

    let dct = separable_dct(&pixels);
    let mut low = Vec::with_capacity(PHASH_LOW_FREQ * PHASH_LOW_FREQ);
    for row in 0..PHASH_LOW_FREQ {
        for column in 0..PHASH_LOW_FREQ {
            if row == 0 && column == 0 {
                continue; // DC component carries only brightness
            }
            low.push(dct[row * PHASH_SIZE + column]);
        }
    }

    let mut sorted = low.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = if sorted.is_empty() {
        0.0
    } else {
        sorted[sorted.len() / 2]
    };

    let mut hash: u64 = 0;
    for (index, coefficient) in low.iter().enumerate() {
        if *coefficient > median {
            hash |= 1u64 << index;
        }
    }
    Ok(hash)
}

/// Separable 2-D DCT-II over a square 32×32 block (rows then columns).
fn separable_dct(pixels: &[f64; PHASH_SIZE * PHASH_SIZE]) -> [f64; PHASH_SIZE * PHASH_SIZE] {
    let mut temp = [0f64; PHASH_SIZE * PHASH_SIZE];
    let mut out = [0f64; PHASH_SIZE * PHASH_SIZE];

    for row in 0..PHASH_SIZE {
        let start = row * PHASH_SIZE;
        dct_1d(
            &pixels[start..start + PHASH_SIZE],
            &mut temp[start..start + PHASH_SIZE],
        );
    }
    let mut column_in = [0f64; PHASH_SIZE];
    let mut column_out = [0f64; PHASH_SIZE];
    for column in 0..PHASH_SIZE {
        for row in 0..PHASH_SIZE {
            column_in[row] = temp[row * PHASH_SIZE + column];
        }
        dct_1d(&column_in, &mut column_out);
        for row in 0..PHASH_SIZE {
            out[row * PHASH_SIZE + column] = column_out[row];
        }
    }
    out
}

fn dct_1d(input: &[f64], output: &mut [f64]) {
    let n = input.len() as f64;
    for (k, slot) in output.iter_mut().enumerate() {
        let scale = if k == 0 {
            (1.0 / n).sqrt()
        } else {
            (2.0 / n).sqrt()
        };
        let mut sum = 0.0;
        for (i, value) in input.iter().enumerate() {
            sum += value
                * ((std::f64::consts::PI * (k as f64) * (2.0 * i as f64 + 1.0)) / (2.0 * n)).cos();
        }
        *slot = scale * sum;
    }
}

/// Hamming distance between two 64-bit hashes.
pub fn hamming_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

pub fn phash_to_hex(hash: u64) -> String {
    format!("{hash:016x}")
}

pub fn phash_from_hex(text: &str) -> Option<u64> {
    u64::from_str_radix(text.trim(), 16).ok()
}

// ---------------------------------------------------------------------------
// Thumbnails
// ---------------------------------------------------------------------------

/// Create a base64 `data:` URI with a downscaled JPEG thumbnail.
pub fn thumbnail_data_uri(path: &Path, max_dim: u32) -> AppResult<String> {
    use base64::Engine as _;
    use image::codecs::jpeg::JpegEncoder;

    let image = image::ImageReader::open(path)?
        .with_guessed_format()?
        .decode()?;
    let thumb = image.thumbnail(max_dim, max_dim);

    let mut buffer: Vec<u8> = Vec::new();
    let rgb = thumb.to_rgb8();
    let mut encoder = JpegEncoder::new_with_quality(&mut buffer, 80);
    encoder.encode_image(&image::DynamicImage::ImageRgb8(rgb))?;

    let encoded = base64::engine::general_purpose::STANDARD.encode(&buffer);
    Ok(format!("data:image/jpeg;base64,{encoded}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp(content: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("temp");
        let path = dir.path().join("blob.bin");
        let mut file = std::fs::File::create(&path).expect("create");
        file.write_all(content).expect("write");
        (dir, path)
    }

    /// A photo-like test image: overlapping blobs on a gradient. A smooth
    /// gradient alone is degenerate for a DCT (almost all coefficients are
    /// exactly zero), so the fixtures carry real low-frequency structure.
    fn photo_like_rgb(width: u32, height: u32, seed: u32) -> image::RgbImage {
        let mut image = image::RgbImage::from_fn(width, height, |x, y| {
            let fx = x as f32 / width as f32;
            let fy = y as f32 / height as f32;
            image::Rgb([
                (40.0 + fx * 90.0) as u8,
                (60.0 + fy * 70.0) as u8,
                (110.0 + (fx + fy) * 40.0) as u8,
            ])
        });
        // Deterministic blobs: no RNG dependency, same picture every run.
        let mut state = seed;
        let mut next = move || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 16) % 1000
        };
        for _ in 0..24 {
            let cx = (next() as f32 / 1000.0 * width as f32) as i32;
            let cy = (next() as f32 / 1000.0 * height as f32) as i32;
            let radius = 12 + (next() % 60) as i32;
            let (r, g, b) = (
                (next() % 220 + 30) as u8,
                (next() % 220 + 30) as u8,
                (next() % 220 + 30) as u8,
            );
            for y in (cy - radius).max(0)..(cy + radius).min(height as i32) {
                for x in (cx - radius).max(0)..(cx + radius).min(width as i32) {
                    let dx = x - cx;
                    let dy = y - cy;
                    if dx * dx + dy * dy <= radius * radius {
                        image.put_pixel(x as u32, y as u32, image::Rgb([r, g, b]));
                    }
                }
            }
        }
        image
    }

    fn save_png(image: &image::RgbImage, dir: &std::path::Path, name: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        image.save(&path).expect("save image");
        path
    }

    /// Half-size copies are the most common duplicate in a photo library; they
    /// must land on the same bits, otherwise the feature is useless.
    #[test]
    fn perceptual_hash_is_independent_of_source_resolution() {
        let dir = tempfile::tempdir().expect("temp");
        let full = photo_like_rgb(640, 480, 11);
        let half = image::imageops::resize(&full, 320, 240, image::imageops::FilterType::Lanczos3);
        let quarter =
            image::imageops::resize(&full, 160, 120, image::imageops::FilterType::Lanczos3);

        let h_full = perceptual_hash(&save_png(&full, dir.path(), "full.png")).expect("hash full");
        let h_half = perceptual_hash(&save_png(&half, dir.path(), "half.png")).expect("hash half");
        let h_quarter =
            perceptual_hash(&save_png(&quarter, dir.path(), "quarter.png")).expect("hash quarter");

        assert!(
            (h_full ^ h_half).count_ones() <= 4,
            "half-size copy drifted {} bits",
            (h_full ^ h_half).count_ones()
        );
        assert!(
            (h_full ^ h_quarter).count_ones() <= 6,
            "quarter-size copy drifted {} bits",
            (h_full ^ h_quarter).count_ones()
        );
    }

    #[test]
    fn perceptual_hash_survives_jpeg_recompression() {
        let dir = tempfile::tempdir().expect("temp");
        let photo = photo_like_rgb(512, 384, 29);
        let png = save_png(&photo, dir.path(), "photo.png");
        let jpg = dir.path().join("photo.jpg");
        photo
            .save_with_format(&jpg, image::ImageFormat::Jpeg)
            .expect("save jpeg");

        let sum = |a: u64, b: u64| (a ^ b).count_ones();
        let a = perceptual_hash(&png).expect("png hash");
        let b = perceptual_hash(&jpg).expect("jpg hash");
        assert!(sum(a, b) <= 6, "recompression moved {} bits", sum(a, b));
    }

    /// Different pictures must not collapse into one group, even after the
    /// edits above move a few bits.
    #[test]
    fn different_photos_stay_far_apart() {
        let dir = tempfile::tempdir().expect("temp");
        let a = perceptual_hash(&save_png(&photo_like_rgb(640, 480, 5), dir.path(), "a.png"))
            .expect("a");
        let b = perceptual_hash(&save_png(
            &photo_like_rgb(640, 480, 77),
            dir.path(),
            "b.png",
        ))
        .expect("b");
        let c = perceptual_hash(&save_png(
            &photo_like_rgb(640, 480, 913),
            dir.path(),
            "c.png",
        ))
        .expect("c");

        let pairs = [("a-b", a, b), ("a-c", a, c), ("b-c", b, c)];
        for (label, x, y) in pairs {
            let distance = (x ^ y).count_ones();
            assert!(
                distance > 12,
                "{label} differ by only {distance} bits - distinct photos would be grouped"
            );
        }
    }

    #[test]
    fn sha256_matches_known_vectors() {
        let (_dir, path) = write_temp(b"abc");
        let (hash, size) = hash_file(&path).expect("hash");
        assert_eq!(size, 3);
        assert_eq!(
            hash,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sha256_empty_file() {
        let (_dir, path) = write_temp(b"");
        let (hash, size) = hash_file(&path).expect("hash");
        assert_eq!(size, 0);
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn identical_content_has_same_hash() {
        let (_d1, p1) = write_temp(&vec![7u8; 2 * 1024 * 1024]);
        let (_d2, p2) = write_temp(&vec![7u8; 2 * 1024 * 1024]);
        assert_eq!(hash_file(&p1).unwrap().0, hash_file(&p2).unwrap().0);
    }

    #[test]
    fn different_content_differs() {
        let (_d1, p1) = write_temp(b"hello world");
        let (_d2, p2) = write_temp(b"hello worle");
        assert_ne!(hash_file(&p1).unwrap().0, hash_file(&p2).unwrap().0);
    }

    #[test]
    fn verify_hash_detects_changes() {
        let (_dir, path) = write_temp(b"original");
        let (hash, _) = hash_file(&path).unwrap();
        assert!(verify_hash(&path, &hash).is_ok());

        std::fs::write(&path, b"changed!").unwrap();
        assert!(verify_hash(&path, &hash).is_err());
    }

    #[test]
    fn quick_hash_is_stable_and_complete_for_small_files() {
        let (_dir, path) = write_temp(&vec![42u8; 200_000]);
        let a = quick_hash(&path).unwrap();
        let b = quick_hash(&path).unwrap();
        assert_eq!(a.fingerprint, b.fingerprint);
        assert!(a.full_sha256.is_none(), "large files need a full pass");

        let (_dir2, small) = write_temp(b"small content");
        let quick = quick_hash(&small).unwrap();
        assert_eq!(quick.full_sha256, Some(hash_file(&small).unwrap().0));
    }

    #[test]
    fn hamming_distance_basics() {
        assert_eq!(hamming_distance(0b1011, 0b0010), 2);
        assert_eq!(hamming_distance(u64::MAX, 0), 64);
    }

    fn save_test_image(path: &Path, pattern: impl Fn(u32, u32) -> u8) {
        let mut img = image::GrayImage::new(64, 64);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            pixel.0[0] = pattern(x, y);
        }
        img.save(path).expect("save image");
    }

    /// Smooth "photo-like" picture: a bright top half, dark bottom half and
    /// a soft vertical gradient — robust to resizing, like real photos.
    fn photo_like(x: u32, y: u32) -> u8 {
        let brightness = if y < 32 { 220 } else { 35 };
        let gradient = (x as f32 / 64.0 * 40.0) as u32;
        (brightness - gradient as i32).clamp(0, 255) as u8
    }

    #[test]
    fn phash_matches_itself_and_differs_for_other_images() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.png");
        let b = dir.path().join("b.png");
        save_test_image(&a, photo_like);
        save_test_image(&b, |x, y| ((x * 3 + y) % 255) as u8);

        let ha = perceptual_hash(&a).unwrap();
        let hb = perceptual_hash(&b).unwrap();
        assert_eq!(hamming_distance(ha, ha), 0);
        assert!(hamming_distance(ha, hb) > 0);

        // A resized copy of the same picture keeps a (nearly) identical hash.
        let resized_path = dir.path().join("a_small.png");
        let img = image::ImageReader::open(&a).unwrap().decode().unwrap();
        img.resize_exact(24, 24, image::imageops::FilterType::Triangle)
            .save(&resized_path)
            .unwrap();
        let hr = perceptual_hash(&resized_path).unwrap();
        assert!(
            hamming_distance(ha, hr) <= 6,
            "distance {}",
            hamming_distance(ha, hr)
        );
    }

    #[test]
    fn thumbnail_generates_data_uri() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo.png");
        save_test_image(&path, |x, y| ((x * y) % 255) as u8);
        let uri = thumbnail_data_uri(&path, 96).unwrap();
        assert!(uri.starts_with("data:image/jpeg;base64,"));
        assert!(uri.len() > 100);
    }

    #[test]
    fn phash_hex_roundtrip() {
        let value = 0xdead_beef_1234_5678u64;
        assert_eq!(phash_from_hex(&phash_to_hex(value)), Some(value));
        assert_eq!(phash_from_hex("not-hex"), None);
    }
}
