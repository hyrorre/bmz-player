//! Per-stage timing of BMS import on real charts, single-threaded.
//!
//! ```text
//! BMZ_IMPORT_BENCH_DIR=G:/BMS/EVENTS BMZ_IMPORT_BENCH_LIMIT=400 \
//!   cargo test -p bmz-chart --release --lib -- --ignored --nocapture import_stage_timing
//! ```
//!
//! The stages mirror `import_with_layout` and `import_chart_bytes_with_assets` with
//! [`AssetLookup::Declared`](crate::import::AssetLookup::Declared), as a library scan
//! runs them, for `.bms` / `.bme` / `.bml` files; keep them in step with that pipeline.

use std::time::{Duration, Instant};

use super::*;

#[derive(Default)]
struct Stages(Vec<(&'static str, Duration)>);

impl Stages {
    fn add(&mut self, name: &'static str, elapsed: Duration) {
        match self.0.iter_mut().find(|(stage, _)| *stage == name) {
            Some((_, total)) => *total += elapsed,
            None => self.0.push((name, elapsed)),
        }
    }
}

fn chart_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    chart_files_with(dir, &["bms", "bme", "bml"], out);
}

fn chart_files_with(dir: &Path, extensions: &[&str], out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            chart_files_with(&path, extensions, out);
        } else if path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| {
            extensions.iter().any(|candidate| ext.eq_ignore_ascii_case(candidate))
        }) {
            out.push(path);
        }
    }
}

#[test]
#[ignore = "needs real charts in BMZ_IMPORT_BENCH_DIR"]
fn import_stage_timing() {
    let Some(dir) = std::env::var_os("BMZ_IMPORT_BENCH_DIR") else {
        eprintln!("skipping: BMZ_IMPORT_BENCH_DIR is not set");
        return;
    };
    let limit = std::env::var("BMZ_IMPORT_BENCH_LIMIT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(400_usize);
    let mut files = Vec::new();
    chart_files(Path::new(&dir), &mut files);
    files.sort();
    files.truncate(limit);
    assert!(!files.is_empty(), "no charts below {}", Path::new(&dir).display());

    let mut stages = Stages::default();
    let mut whole = Duration::ZERO;
    let mut failed = 0_usize;
    macro_rules! timed {
        ($name:literal, $expr:expr) => {{
            let started = Instant::now();
            let value = $expr;
            stages.add($name, started.elapsed());
            value
        }};
    }
    for path in &files {
        let bytes = timed!("read", std::fs::read(path).unwrap());
        let random_source = BmsRandomSource::Seed(None);

        // The whole import as the scan runs it, for a cross-check of the stage sum.
        let started = Instant::now();
        let full = crate::import::import_chart_bytes_with_assets(
            path,
            &bytes,
            random_source.clone(),
            crate::import::AssetLookup::Declared,
        );
        whole += started.elapsed();
        if full.is_err() {
            failed += 1;
            continue;
        }

        let mut warnings = Vec::new();
        let mut random_choices = Vec::new();
        let mut switch_choices = Vec::new();
        let identity = timed!("hash md5+sha256", compute_chart_identity(&bytes));
        let raw_text = timed!("decode text", decode_bms_text(&bytes, &mut warnings));
        let source = timed!(
            "prepare source text",
            prepare_bms_source(
                &raw_text,
                false,
                &random_source,
                &mut random_choices,
                &mut switch_choices,
                &mut warnings,
            )
        );
        timed!("drop decoded text", drop(raw_text));
        let BmsOutput { bms, warnings: bms_warnings } = timed!(
            "bms-rs parse_bms",
            parse_bms::<KeyLayoutBeat, _, _, _>(
                &source.parse_text,
                default_config_with_rng(JavaRandom::new(random_source_seed(&random_source) as i64))
                    .key_mapper::<KeyLayoutBeat>(),
            )
        );
        timed!("map bms-rs warnings", {
            for warning in bms_warnings {
                if let Some(warning) = map_bms_warning(&warning) {
                    warnings.push(warning);
                }
            }
        });
        let mut bms = bms.unwrap();
        timed!(
            "inject sparse messages",
            inject_sparse_bms_messages::<KeyLayoutBeat>(
                &mut bms,
                &source.sparse_messages,
                &mut warnings,
            )
        );
        let base62 = timed!("detect #BASE 62", bms_uses_base62_obj_ids(&bms));
        let mut extra_objects = timed!(
            "BGA objects",
            bga_messages_to_intermediate_objects(&source.bga_messages, base62, &mut warnings)
        );
        bms.bmp.bga_changes.clear();
        let source_texts = timed!(
            "text objects",
            source_text_objects(&source.source_texts, base62, &mut warnings)
        );
        extra_objects.extend(source_texts.objects);
        bms.text.text_events.clear();
        let mut intermediate = timed!(
            "build intermediate",
            build_intermediate_from_bms_with_extra_objects::<KeyLayoutBeat>(
                &bms,
                ChartKeyLayout::beat(),
                &extra_objects,
                &mut warnings,
            )
            .unwrap()
        );
        timed!("headers and #LNOBJ", {
            restore_default_poor_bga(&mut intermediate, bms.bmp.poor_bmp.as_deref());
            intermediate.lnobj_wav_key = lnobj_wav_key(&source.lnobj_lines, base62, &mut warnings);
            let headers = source.bms_headers;
            intermediate.metadata.has_bms_random = source.has_bms_random;
            intermediate.metadata.miss_text = source_texts.miss_text;
            apply_raw_judge_rank_headers(&mut intermediate, &headers);
            intermediate.metadata.source_url = bms
                .metadata
                .url
                .clone()
                .filter(|url| !url.is_empty())
                .or_else(|| headers.get("URL").cloned())
                .unwrap_or_default();
            intermediate.metadata.append_url = append_url_from_headers(&headers);
            intermediate.metadata.bms_headers = headers;
        });
        timed!("drop parse text", drop(source.parse_text));
        intermediate.identity = identity;
        let _ = timed!(
            "normalize_chart",
            crate::import::normalize::normalize_chart_with_assets(
                path,
                intermediate,
                &mut warnings,
                crate::import::AssetLookup::Declared,
            )
        );
        drop(timed!("drop bms model", bms));
    }

    let staged: Duration = stages.0.iter().map(|(_, elapsed)| *elapsed).sum();
    let measured = files.len() - failed;
    stages.0.sort_by_key(|(_, elapsed)| std::cmp::Reverse(*elapsed));
    eprintln!(
        "{measured} charts ({failed} failed and skipped); whole import {:.1} ms/chart, stages {:.1} ms/chart",
        whole.as_secs_f64() * 1000.0 / measured as f64,
        staged.as_secs_f64() * 1000.0 / measured as f64,
    );
    for (name, elapsed) in &stages.0 {
        eprintln!(
            "{name:>24}: {:8.3} ms/chart {:5.1}%",
            elapsed.as_secs_f64() * 1000.0 / measured as f64,
            elapsed.as_secs_f64() * 100.0 / staged.as_secs_f64(),
        );
    }
}

/// Writes one SHA-256 digest of the whole import result (chart, warnings and RANDOM
/// choices, or the error) per chart and random source, for comparing the output of
/// two builds byte for byte. `.pms` files are included; `.bmson` files are not, since
/// they do not go through the BMS text passes. With `BMZ_IMPORT_DIGEST_MATCH`
/// set, the full Debug text of the charts whose path contains it is written instead.
///
/// ```text
/// BMZ_IMPORT_BENCH_DIR=G:/BMS/EVENTS BMZ_IMPORT_DIGEST_OUT=before.txt \
///   cargo test -p bmz-chart --release --lib -- --ignored --nocapture import_output_digest
/// ```
#[test]
#[ignore = "needs real charts in BMZ_IMPORT_BENCH_DIR"]
fn import_output_digest() {
    use std::fmt::Write as _;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use sha2::{Digest, Sha256};

    let (Some(dir), Some(out)) =
        (std::env::var_os("BMZ_IMPORT_BENCH_DIR"), std::env::var_os("BMZ_IMPORT_DIGEST_OUT"))
    else {
        eprintln!("skipping: BMZ_IMPORT_BENCH_DIR or BMZ_IMPORT_DIGEST_OUT is not set");
        return;
    };
    let full_match = std::env::var("BMZ_IMPORT_DIGEST_MATCH").ok();
    let dir = Path::new(&dir);
    let mut files = Vec::new();
    chart_files_with(dir, &["bms", "bme", "bml", "pms"], &mut files);
    files.sort();
    if let Some(needle) = &full_match {
        files.retain(|path| path.to_string_lossy().contains(needle.as_str()));
    }
    let random_sources = [
        ("seed-none", BmsRandomSource::Seed(None)),
        ("seed-1", BmsRandomSource::Seed(Some(1))),
        ("seed-3735928559", BmsRandomSource::Seed(Some(0xDEAD_BEEF))),
        ("choices", BmsRandomSource::Choices { random: vec![2, 1, 3], switches: vec![2, 1] }),
    ];

    // `import_chart_bytes_with_assets` for BMS / PMS, except that the definitions bms-rs
    // keeps in HashMaps are put in key order: their order, and so the sound IDs, would
    // otherwise change from run to run.
    let import = |path: &Path,
                  bytes: &[u8],
                  random_source: &BmsRandomSource|
     -> Result<crate::import::ImportResult, ImportError> {
        let mut warnings = Vec::new();
        let mut bms_random_choices = Vec::new();
        let mut bms_switch_choices = Vec::new();
        let format = crate::import::chart_file_format(path);
        let import_bytes = match format {
            crate::import::ChartFileFormat::Pms => import_pms_bytes_to_intermediate,
            _ => import_bms_bytes_to_intermediate,
        };
        let mut intermediate = import_bytes(
            path,
            bytes,
            random_source,
            &mut bms_random_choices,
            &mut bms_switch_choices,
            &mut warnings,
        )?;
        intermediate.resources.wavs.sort_by_key(|wav| wav.key);
        intermediate.resources.swbga_defs.sort_by_key(|def| def.id);
        let mut chart = crate::import::normalize::normalize_chart_with_assets(
            path,
            intermediate,
            &mut warnings,
            crate::import::AssetLookup::Declared,
        )?;
        chart.metadata.source_format = format.source_format();
        Ok(crate::import::ImportResult { chart, warnings, bms_random_choices, bms_switch_choices })
    };
    let describe = |path: &Path, random_source: &BmsRandomSource| -> String {
        let bytes = std::fs::read(path).unwrap();
        match import(path, &bytes, random_source) {
            Ok(mut result) => {
                // The only HashMap in the result; its Debug order is not stable.
                let bga_asset_by_bmp_key: BTreeMap<_, _> =
                    std::mem::take(&mut result.chart.bga_asset_by_bmp_key).into_iter().collect();
                format!("{result:#?}\nbga_asset_by_bmp_key: {bga_asset_by_bmp_key:#?}")
            }
            Err(err) => format!("error: {err:?}"),
        }
    };

    let lines: Vec<std::sync::Mutex<String>> =
        files.iter().map(|_| std::sync::Mutex::new(String::new())).collect();
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..std::thread::available_parallelism().map_or(4, |count| count.get()) {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(path) = files.get(index) else {
                        break;
                    };
                    let relative = path.strip_prefix(dir).unwrap_or(path).display().to_string();
                    let mut line = String::new();
                    for (label, random_source) in &random_sources {
                        let text = describe(path, random_source);
                        if full_match.is_some() {
                            writeln!(line, "== {relative}\t{label}\n{text}").unwrap();
                        } else {
                            let digest = Sha256::digest(text.as_bytes());
                            write!(line, "{relative}\t{label}\t").unwrap();
                            for byte in digest {
                                write!(line, "{byte:02x}").unwrap();
                            }
                            line.push('\n');
                        }
                    }
                    *lines[index].lock().unwrap() = line;
                }
            });
        }
    });
    let output: String = lines.into_iter().map(|line| line.into_inner().unwrap()).collect();
    std::fs::write(&out, output).unwrap();
    eprintln!(
        "{} charts x {} random sources written to {}",
        files.len(),
        random_sources.len(),
        Path::new(&out).display()
    );
}

/// Wall time of whole imports spread over N threads, from pre-read bytes and from
/// files. Ideal scaling divides the 1-thread time by N; contention shows up as less.
///
/// ```text
/// BMZ_IMPORT_BENCH_DIR=G:/BMS/EVENTS BMZ_IMPORT_BENCH_LIMIT=1200 \
///   cargo test -p bmz-chart --release --lib -- --ignored --nocapture import_thread_scaling
/// ```
#[test]
#[ignore = "needs real charts in BMZ_IMPORT_BENCH_DIR"]
fn import_thread_scaling() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let Some(dir) = std::env::var_os("BMZ_IMPORT_BENCH_DIR") else {
        eprintln!("skipping: BMZ_IMPORT_BENCH_DIR is not set");
        return;
    };
    let limit = std::env::var("BMZ_IMPORT_BENCH_LIMIT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1200_usize);
    let mut files = Vec::new();
    chart_files(Path::new(&dir), &mut files);
    files.sort();
    files.truncate(limit);
    let sources: Vec<(std::path::PathBuf, Vec<u8>)> =
        files.iter().map(|path| (path.clone(), std::fs::read(path).unwrap())).collect();

    let run = |threads: usize, from_files: bool| {
        let next = AtomicUsize::new(0);
        let started = Instant::now();
        std::thread::scope(|scope| {
            for _ in 0..threads {
                scope.spawn(|| {
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some((path, bytes)) = sources.get(index) else {
                            break;
                        };
                        let lookup = crate::import::AssetLookup::Declared;
                        let result = if from_files {
                            crate::import::import_chart_with_assets(
                                path,
                                BmsRandomSource::Seed(None),
                                lookup,
                            )
                        } else {
                            crate::import::import_chart_bytes_with_assets(
                                path,
                                bytes,
                                BmsRandomSource::Seed(None),
                                lookup,
                            )
                        };
                        std::hint::black_box(result.ok());
                    }
                });
            }
        });
        started.elapsed()
    };
    let single = run(1, false);
    eprintln!("{} charts; 1 thread {:.2} s", sources.len(), single.as_secs_f64());
    for threads in [2, 4, 8, 12, 16, 24] {
        let bytes = run(threads, false);
        let files = run(threads, true);
        eprintln!(
            "{threads:>2} threads: bytes {:.2} s (x{:.1}), files {:.2} s (x{:.1})",
            bytes.as_secs_f64(),
            single.as_secs_f64() / bytes.as_secs_f64(),
            files.as_secs_f64(),
            single.as_secs_f64() / files.as_secs_f64(),
        );
    }
}
