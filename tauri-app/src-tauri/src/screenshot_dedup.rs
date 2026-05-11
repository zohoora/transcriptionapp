//! Screenshot deduplication for multimodal LLM calls (multi-patient detect + SOAP).
//!
//! Rust port of the validated Python algorithm at `/tmp/vision_experiment/dedup.py`.
//! Used by `encounter_pipeline` + `continuous_mode_merge_back` + `commands::ollama` to
//! attach up to `MAX_SCREENSHOTS_PER_LLM_CALL` deduped chart screenshots to the multi-patient
//! detect and SOAP-generation LLM calls.
//!
//! ### Algorithm
//! 1. dHash every screenshot (8x8 → 64-bit signature) — captures gradient direction across
//!    the frame, robust to minor rendering differences.
//! 2. Single-linkage cluster by Hamming distance with a TWO-TIER threshold:
//!    - Body of session (first `1 - END_WINDOW_FRAC`): `DEDUP_THRESHOLD`
//!    - End window (last `END_WINDOW_FRAC`):           `DEDUP_THRESHOLD * END_THRESHOLD_FACTOR`
//!    The stricter end-window threshold means fewer captures collapse together in the
//!    prescription/plan-writing window, so more reps survive there.
//! 3. For each cluster, pick one representative:
//!    - If the cluster touches the end window  → LATEST member (most likely to show the
//!      finalized prescription / plan).
//!    - Otherwise                              → MEDOID (member with smallest Hamming-sum
//!      to other members — the most "representative" view in that cluster).
//! 4. Always force-keep the final screenshot as a safety net for the plan/prescription
//!    boundary, in case its cluster's rep was an earlier member.
//! 5. If the final list exceeds `max_keep`, evenly downsample (including endpoints) so
//!    we still send a representative spread.
//!
//! ### Why no region-aware (banner-region) hashing?
//! Future users may use different EMRs with different layouts; banner-region assumptions
//! don't generalize. Cluster+medoid+end-bias is EMR-agnostic.
//!
//! ### Why end-bias?
//! Final prescriptions, dosing changes, and the plan section are typically displayed in
//! the closing minutes of an encounter. Losing those screenshots costs more downstream
//! than losing redundant mid-encounter views of the same chart section.

use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use image::imageops::FilterType;
use tracing::warn;

use crate::llm_client::{ContentPart, ImageUrlContent};

/// dHash Hamming-distance threshold for clustering screenshots in the body of the session.
const DEDUP_THRESHOLD: u32 = 5;
/// Last fraction of the session treated as the "end window" (where stricter clustering applies).
const END_WINDOW_FRAC: f64 = 0.30;
/// End-window threshold = `DEDUP_THRESHOLD * END_THRESHOLD_FACTOR` (rounded, clamped to ≥1).
const END_THRESHOLD_FACTOR: f64 = 0.6;

/// Maximum number of screenshots attached to any single multimodal LLM call.
///
/// Cap rather than target: dedup typically produces fewer than this on real
/// corpus sessions (Marion 8, Linda+Rashida 6, Janice 25 → capped to 8).
/// When dedup exceeds the cap, an evenly-spaced downsample preserves temporal
/// spread (including the last frame).
pub const MAX_SCREENSHOTS_PER_LLM_CALL: usize = 8;

// ────────────────────────────────────────────────────────────────────────────
// dHash + Hamming
// ────────────────────────────────────────────────────────────────────────────

/// 8x8 difference hash → 64-bit signature.
///
/// Decodes the JPEG, converts to luma8 (grayscale), resizes to 9x8 with Lanczos3,
/// then encodes each row as 8 bits: `left > right ? 1 : 0` for adjacent pixel pairs.
pub fn dhash(image_path: &Path) -> Result<u64, String> {
    let img = image::open(image_path).map_err(|e| format!("dhash open {:?}: {}", image_path, e))?;
    let gray = img.to_luma8();
    let resized = image::imageops::resize(&gray, 9, 8, FilterType::Lanczos3);
    let mut bits: u64 = 0;
    for row in 0u32..8 {
        for col in 0u32..8 {
            let left = resized.get_pixel(col, row).0[0];
            let right = resized.get_pixel(col + 1, row).0[0];
            bits = (bits << 1) | if left > right { 1 } else { 0 };
        }
    }
    Ok(bits)
}

/// Bit-level Hamming distance between two dHash signatures.
#[inline]
pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

// ────────────────────────────────────────────────────────────────────────────
// Dedup
// ────────────────────────────────────────────────────────────────────────────

/// Cluster + medoid dedup with stricter clustering near end-of-session.
///
/// `paths` MUST be chronologically ordered (sorted by filename works — capture
/// filenames are `{NNN_zero_padded}_{timestamp}.jpg`).
///
/// Returns up to `max_keep` paths in chronological order. Caller-owned alloc.
pub fn dedup_screenshots(paths: &[PathBuf], max_keep: usize) -> Vec<PathBuf> {
    dedup_screenshots_with(
        paths,
        DEDUP_THRESHOLD,
        END_WINDOW_FRAC,
        END_THRESHOLD_FACTOR,
        max_keep,
    )
}

/// Parameterized dedup (mirrors the Python `dedup_screenshots_detailed` knobs).
/// Exposed primarily for tests / experiments; production should call
/// [`dedup_screenshots`] which uses the calibrated defaults.
pub fn dedup_screenshots_with(
    paths: &[PathBuf],
    threshold: u32,
    end_window_frac: f64,
    end_threshold_factor: f64,
    max_keep: usize,
) -> Vec<PathBuf> {
    if paths.is_empty() {
        return Vec::new();
    }
    if paths.len() == 1 {
        return vec![paths[0].clone()];
    }

    let n = paths.len();
    let end_start = ((n as f64) * (1.0 - end_window_frac)) as usize;
    let body_t = threshold;
    let end_t = ((threshold as f64) * end_threshold_factor).round().max(1.0) as u32;
    let thresh_for = |i: usize| -> u32 { if i >= end_start { end_t } else { body_t } };

    // Hash every screenshot. Skip paths that fail to hash (corrupt file, missing,
    // permission denied, etc.) rather than aborting — caller still gets the rest.
    #[derive(Clone)]
    struct Item {
        idx: usize,
        path: PathBuf,
        h: u64,
    }
    let mut items: Vec<Item> = Vec::with_capacity(n);
    for (i, p) in paths.iter().enumerate() {
        match dhash(p) {
            Ok(h) => items.push(Item { idx: i, path: p.clone(), h }),
            Err(e) => warn!("screenshot_dedup: skipping {:?} ({})", p, e),
        }
    }
    if items.is_empty() {
        return Vec::new();
    }

    // Greedy single-linkage clustering. Use min(my_threshold, their_threshold) as
    // the merge cutoff so an end-window screenshot can't be lazily absorbed into a
    // permissive body cluster — it requires the stricter similarity to merge.
    let mut clusters: Vec<Vec<Item>> = Vec::new();
    'item: for it in items.iter() {
        let my_t = thresh_for(it.idx);
        for c in clusters.iter_mut() {
            for m in c.iter() {
                if hamming(it.h, m.h) <= my_t.min(thresh_for(m.idx)) {
                    c.push(it.clone());
                    continue 'item;
                }
            }
        }
        clusters.push(vec![it.clone()]);
    }

    // Pick one representative per cluster.
    let mut chosen: Vec<Item> = Vec::with_capacity(clusters.len());
    for c in &clusters {
        if c.len() == 1 {
            chosen.push(c[0].clone());
            continue;
        }
        let touches_end = c.iter().any(|m| m.idx >= end_start);
        let rep = if touches_end {
            // End-window cluster: prefer LATEST (closest to plan/Rx writing).
            c.iter().max_by_key(|m| m.idx).expect("non-empty")
        } else {
            // Body cluster: MEDOID (member with smallest summed Hamming to others).
            c.iter()
                .min_by_key(|m| {
                    c.iter().filter(|x| x.idx != m.idx).map(|x| hamming(m.h, x.h)).sum::<u32>()
                })
                .expect("non-empty")
        };
        chosen.push(rep.clone());
    }

    // Force-keep the absolute last screenshot (safety net for the plan boundary).
    let last_idx = items.last().expect("non-empty").idx;
    if !chosen.iter().any(|m| m.idx == last_idx) {
        chosen.push(items.last().expect("non-empty").clone());
    }

    chosen.sort_by_key(|m| m.idx);
    let mut kept: Vec<PathBuf> = chosen.into_iter().map(|m| m.path).collect();

    // Apply max_keep cap with evenly-spaced downsample (including endpoints).
    if max_keep > 0 && kept.len() > max_keep {
        let total = kept.len();
        if max_keep == 1 {
            kept = vec![kept[total - 1].clone()];
        } else {
            kept = (0..max_keep)
                .map(|i| kept[i * (total - 1) / (max_keep - 1)].clone())
                .collect();
        }
    }
    kept
}

// ────────────────────────────────────────────────────────────────────────────
// Disk helpers
// ────────────────────────────────────────────────────────────────────────────

/// Resolve a session's archive dir, list its screenshots, and run the calibrated
/// dedup in one call. Returns an empty `Vec` when the session has no archive
/// dir (transient I/O error) or no screenshots on disk. The result is always
/// safe to pass as `Some(&deduped)` to the multimodal LLM helpers — they treat
/// an empty slice as a signal to fall back to text-only.
pub fn load_deduped_screenshots_for_session(
    session_id: &str,
    date: &chrono::DateTime<chrono::Utc>,
) -> Vec<PathBuf> {
    let Ok(session_dir) = crate::local_archive::get_session_archive_dir(session_id, date) else {
        return Vec::new();
    };
    let all = list_session_screenshots(&session_dir);
    if all.is_empty() {
        return Vec::new();
    }
    dedup_screenshots(&all, MAX_SCREENSHOTS_PER_LLM_CALL)
}

/// List `session_dir/screenshots/*.jpg` in chronological order (by filename).
///
/// Returns an empty `Vec` when the directory doesn't exist, can't be read, or
/// has no JPEGs. Filename pattern is `{NNN}_{YYYY-MM-DDTHHMM}.jpg`, so a
/// lexicographic sort is chronological.
pub fn list_session_screenshots(session_dir: &Path) -> Vec<PathBuf> {
    let ss_dir = session_dir.join("screenshots");
    let read_dir = match std::fs::read_dir(&ss_dir) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };
    let mut paths: Vec<PathBuf> = read_dir
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|x| x.to_str())
                .map(|x| x.eq_ignore_ascii_case("jpg") || x.eq_ignore_ascii_case("jpeg"))
                .unwrap_or(false)
        })
        .collect();
    paths.sort();
    paths
}

/// Read a JPEG and return a multimodal `ContentPart::ImageUrl` with a base64
/// data URL. Returns `None` on I/O failure (caller skips that path silently).
pub fn load_jpeg_as_content_part(path: &Path) -> Option<ContentPart> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            warn!("screenshot_dedup: failed to read {:?} ({})", path, e);
            return None;
        }
    };
    let b64 = BASE64_STANDARD.encode(&bytes);
    Some(ContentPart::ImageUrl {
        image_url: ImageUrlContent {
            url: format!("data:image/jpeg;base64,{}", b64),
        },
    })
}

// ────────────────────────────────────────────────────────────────────────────
// Tests
// ────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Luma};

    /// Encode an 8-bit grayscale buffer as JPEG bytes for an in-memory fixture.
    fn synthetic_jpeg(width: u32, height: u32, fill: impl Fn(u32, u32) -> u8) -> Vec<u8> {
        let img: ImageBuffer<Luma<u8>, Vec<u8>> = ImageBuffer::from_fn(width, height, |x, y| Luma([fill(x, y)]));
        let mut bytes: Vec<u8> = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 80);
        encoder
            .encode(&img.into_raw(), width, height, image::ExtendedColorType::L8)
            .expect("jpeg encode");
        bytes
    }

    fn write_jpeg(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, bytes).expect("write jpeg");
        p
    }

    #[test]
    fn hamming_basic() {
        assert_eq!(hamming(0, 0), 0);
        assert_eq!(hamming(0b1010, 0b0101), 4);
        assert_eq!(hamming(u64::MAX, 0), 64);
    }

    #[test]
    fn dhash_is_deterministic() {
        let tmp = tempfile::tempdir().unwrap();
        let bytes = synthetic_jpeg(40, 30, |x, y| ((x.wrapping_add(y)) * 7) as u8);
        let p = write_jpeg(tmp.path(), "a.jpg", &bytes);
        let h1 = dhash(&p).unwrap();
        let h2 = dhash(&p).unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn dedup_empty_input() {
        assert!(dedup_screenshots(&[], MAX_SCREENSHOTS_PER_LLM_CALL).is_empty());
    }

    #[test]
    fn dedup_single_screenshot() {
        let tmp = tempfile::tempdir().unwrap();
        let bytes = synthetic_jpeg(20, 20, |x, _| x as u8 * 12);
        let p = write_jpeg(tmp.path(), "001_2026-01-01T0000.jpg", &bytes);
        let out = dedup_screenshots(&[p.clone()], MAX_SCREENSHOTS_PER_LLM_CALL);
        assert_eq!(out, vec![p]);
    }

    #[test]
    fn dedup_collapses_identical_then_keeps_singletons() {
        let tmp = tempfile::tempdir().unwrap();
        // 4 identical body screenshots + 2 distinct end-window screenshots.
        let same = synthetic_jpeg(64, 48, |x, y| ((x * 13 + y * 7) % 251) as u8);
        let p0 = write_jpeg(tmp.path(), "000_2026-01-01T0900.jpg", &same);
        let p1 = write_jpeg(tmp.path(), "001_2026-01-01T0901.jpg", &same);
        let p2 = write_jpeg(tmp.path(), "002_2026-01-01T0902.jpg", &same);
        let p3 = write_jpeg(tmp.path(), "003_2026-01-01T0903.jpg", &same);
        let p4 = write_jpeg(
            tmp.path(),
            "004_2026-01-01T0904.jpg",
            &synthetic_jpeg(64, 48, |x, y| ((x * 31 + y * 17 + 50) % 251) as u8),
        );
        let p5 = write_jpeg(
            tmp.path(),
            "005_2026-01-01T0905.jpg",
            &synthetic_jpeg(64, 48, |x, y| ((x * 199 + y * 41 + 200) % 251) as u8),
        );
        let paths = vec![p0.clone(), p1.clone(), p2.clone(), p3.clone(), p4.clone(), p5.clone()];
        let out = dedup_screenshots(&paths, MAX_SCREENSHOTS_PER_LLM_CALL);
        // 4 identicals collapse to 1, plus 2 distinct end-window pictures → 3 total.
        // The last screenshot (p5) MUST be present.
        assert!(out.len() <= 4, "expected ≤4 reps, got {} ({:?})", out.len(), out);
        assert!(out.contains(&p5), "must force-keep final screenshot");
    }

    #[test]
    fn dedup_max_keep_cap() {
        let tmp = tempfile::tempdir().unwrap();
        // 20 visually-distinct screenshots → dedup keeps most → cap kicks in.
        let mut paths = Vec::new();
        for i in 0..20u32 {
            let bytes = synthetic_jpeg(48, 40, move |x, y| ((x * (i + 1) * 9 + y * 11 + i * 23) % 251) as u8);
            paths.push(write_jpeg(tmp.path(), &format!("{:03}_2026-01-01T09{:02}.jpg", i, i), &bytes));
        }
        let out = dedup_screenshots(&paths, 8);
        assert!(out.len() <= 8, "cap violated: {} > 8", out.len());
        // Final frame preserved via downsample endpoint.
        assert_eq!(out.last(), paths.last(), "downsample must keep the final frame");
    }

    #[test]
    fn list_session_screenshots_missing_dir_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(list_session_screenshots(tmp.path()).is_empty());
    }

    #[test]
    fn list_session_screenshots_returns_chronological() {
        let tmp = tempfile::tempdir().unwrap();
        let ss = tmp.path().join("screenshots");
        std::fs::create_dir(&ss).unwrap();
        let bytes = synthetic_jpeg(8, 8, |_, _| 128);
        let _p2 = write_jpeg(&ss, "002_2026-01-01T0902.jpg", &bytes);
        let _p0 = write_jpeg(&ss, "000_2026-01-01T0900.jpg", &bytes);
        let _p1 = write_jpeg(&ss, "001_2026-01-01T0901.jpg", &bytes);
        // unrelated file should be skipped
        std::fs::write(ss.join("metadata.json"), b"{}").unwrap();
        let out = list_session_screenshots(tmp.path());
        assert_eq!(out.len(), 3);
        let names: Vec<String> = out.iter().map(|p| p.file_name().unwrap().to_string_lossy().to_string()).collect();
        assert_eq!(names[0], "000_2026-01-01T0900.jpg");
        assert_eq!(names[1], "001_2026-01-01T0901.jpg");
        assert_eq!(names[2], "002_2026-01-01T0902.jpg");
    }

    #[test]
    fn load_jpeg_as_content_part_returns_data_url() {
        let tmp = tempfile::tempdir().unwrap();
        let bytes = synthetic_jpeg(12, 10, |x, _| x as u8 * 20);
        let p = write_jpeg(tmp.path(), "x.jpg", &bytes);
        let part = load_jpeg_as_content_part(&p).expect("Some");
        match part {
            ContentPart::ImageUrl { image_url } => {
                assert!(image_url.url.starts_with("data:image/jpeg;base64,"));
                assert!(image_url.url.len() > "data:image/jpeg;base64,".len());
            }
            ContentPart::Text { .. } => panic!("expected ImageUrl variant"),
        }
    }

    #[test]
    fn load_jpeg_as_content_part_missing_file_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("nope.jpg");
        assert!(load_jpeg_as_content_part(&missing).is_none());
    }
}
