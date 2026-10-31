//! Similar photos: the same photo resized, re-saved or in another format. Each photo gets a
//! 64-bit difference hash (dHash): the photo shrunk to 9 × 8 grey pixels, one bit for each
//! pair of neighbours. Photos whose hashes differ in a few bits look the same.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use image::{DynamicImage, ImageFormat, ImageReader};
use rayon::prelude::*;

use super::cache::HashCache;
use super::{Candidate, DupeGroup, DupeProgress};

/// The formats that the decoder reads. HEIC and RAW files are not supported.
const PHOTO_EXT: &[&str] = &["jpg", "jpeg", "png", "gif", "bmp", "webp", "tif", "tiff"];
/// Hashes that differ in at most this many bits count as the same photo.
const MAX_DISTANCE: u32 = 6;
/// Buckets bigger than this (for example many plain white images) are not compared pair by pair.
const MAX_BUCKET: usize = 3000;

pub fn is_photo(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| PHOTO_EXT.iter().any(|e| ext.eq_ignore_ascii_case(e)))
}

fn decode(path: &str) -> Option<DynamicImage> {
    ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()
}

/// The difference hash of a picture.
pub fn dhash(img: &DynamicImage) -> u64 {
    let small = img.thumbnail_exact(9, 8).to_luma8();
    let mut bits = 0u64;
    for y in 0..8 {
        for x in 0..8 {
            bits <<= 1;
            if small.get_pixel(x, y)[0] < small.get_pixel(x + 1, y)[0] {
                bits |= 1;
            }
        }
    }
    bits
}

/// The photo hash, from the cache or from the file.
fn cached_photo(c: &Candidate, p: &DupeProgress, cache: &Mutex<HashCache>) -> Option<u64> {
    let hit = cache
        .lock()
        .ok()
        .and_then(|mut k| k.get(&c.path, c.size, c.modified))
        .and_then(|e| e.photo);
    let h = match hit {
        Some(h) => h,
        None => {
            let h = dhash(&decode(&c.path)?);
            if let Ok(mut k) = cache.lock() {
                k.update(&c.path, c.size, c.modified, |e| e.photo = Some(h));
            }
            h
        }
    };
    p.hashed_bytes.fetch_add(c.size, Ordering::Relaxed);
    Some(h)
}

fn find_root(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// Groups of similar photos, biggest first in each group. Two photos with hashes within
/// `MAX_DISTANCE` bits share at least one of the 8 bytes of their hashes, so only photos
/// in the same byte bucket are compared.
pub(super) fn similar(
    list: Vec<Candidate>,
    p: &DupeProgress,
    cache: &Mutex<HashCache>,
) -> Vec<DupeGroup> {
    let list: Vec<Candidate> = list.into_iter().filter(|c| is_photo(c.name())).collect();
    p.total_bytes
        .store(list.iter().map(|c| c.size).sum(), Ordering::Relaxed);
    let hashed: Vec<(Candidate, u64)> = list
        .into_par_iter()
        .filter_map(|c| {
            if p.is_cancelled() {
                return None;
            }
            let h = cached_photo(&c, p, cache)?;
            Some((c, h))
        })
        .collect();
    if p.is_cancelled() {
        return Vec::new();
    }

    let mut buckets: HashMap<(u8, u8), Vec<usize>> = HashMap::new();
    for (i, (_, h)) in hashed.iter().enumerate() {
        for b in 0..8u8 {
            buckets
                .entry((b, (h >> (b * 8)) as u8))
                .or_default()
                .push(i);
        }
    }
    let mut parent: Vec<usize> = (0..hashed.len()).collect();
    for members in buckets.values() {
        if members.len() < 2 || members.len() > MAX_BUCKET {
            continue;
        }
        for (x, &i) in members.iter().enumerate() {
            for &j in &members[x + 1..] {
                if (hashed[i].1 ^ hashed[j].1).count_ones() <= MAX_DISTANCE {
                    let (a, b) = (find_root(&mut parent, i), find_root(&mut parent, j));
                    if a != b {
                        parent[a] = b;
                    }
                }
            }
        }
    }

    let mut by_root: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..hashed.len() {
        let r = find_root(&mut parent, i);
        by_root.entry(r).or_default().push(i);
    }
    let mut slots: Vec<Option<(Candidate, u64)>> = hashed.into_iter().map(Some).collect();
    let mut groups: Vec<DupeGroup> = by_root
        .into_values()
        .filter(|v| v.len() > 1)
        .map(|v| {
            let mut members: Vec<(Candidate, u64)> =
                v.into_iter().filter_map(|i| slots[i].take()).collect();
            members.sort_by_key(|(c, _)| std::cmp::Reverse(c.size));
            p.groups.fetch_add(1, Ordering::Relaxed);
            DupeGroup {
                size: members[0].0.size,
                hash: format!("{:016x}", members[0].1),
                kind: "similar",
                items: 1,
                files: members.into_iter().map(|(c, _)| c.into_file(0)).collect(),
            }
        })
        .collect();
    let extra = |g: &DupeGroup| g.files.iter().map(|f| f.size).sum::<u64>() - g.size;
    groups.sort_by_key(|g| std::cmp::Reverse(extra(g)));
    groups
}

/// A small PNG preview of a photo as a `data:` URL, or None when it cannot be read.
pub fn thumbnail(path: &str) -> Option<String> {
    let img = decode(path)?.thumbnail(128, 128);
    let mut png = Vec::new();
    img.write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .ok()?;
    Some(format!("data:image/png;base64,{}", base64(&png)))
}

fn base64(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    /// A test picture: a soft gradient with a dark block, so that it has a real shape.
    fn picture(w: u32, h: u32, shift: u8) -> DynamicImage {
        let img = RgbImage::from_fn(w, h, |x, y| {
            let v = ((x * 255 / w) as u8).saturating_add(shift);
            if x > w / 3 && x < w / 2 && y > h / 4 && y < h / 2 {
                Rgb([20, 20, 20])
            } else {
                Rgb([v, v / 2, 255 - v])
            }
        });
        DynamicImage::ImageRgb8(img)
    }

    #[test]
    fn resized_and_brighter_photos_match() {
        let a = dhash(&picture(800, 600, 0));
        let b = dhash(&picture(400, 300, 0));
        let c = dhash(&picture(800, 600, 12));
        assert!((a ^ b).count_ones() <= MAX_DISTANCE, "resized");
        assert!((a ^ c).count_ones() <= MAX_DISTANCE, "brighter");
        let flipped = dhash(&picture(800, 600, 0).fliph());
        assert!(
            (a ^ flipped).count_ones() > MAX_DISTANCE,
            "a mirror image is another photo"
        );
    }

    #[test]
    fn photo_names() {
        assert!(is_photo("IMG_1.JPG"));
        assert!(is_photo("a.webp"));
        assert!(!is_photo("a.heic"));
        assert!(!is_photo("jpg"));
    }

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
