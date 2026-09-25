//! 起動時フォント探索の診断用。引数には同梱フォントのディレクトリを渡す。
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    for pass in 1..=2 {
        for coverage in bmz_font::ALL_FONT_COVERAGES {
            let start = Instant::now();
            let resolved = bmz_font::resolve_font_for_coverage(coverage, &roots);
            let elapsed = start.elapsed();
            println!(
                "pass={pass} coverage={coverage:?} resolve_ms={:.3} source={}",
                elapsed.as_secs_f64() * 1000.0,
                resolved
                    .as_ref()
                    .map(bmz_font::resolved_font_source)
                    .unwrap_or_else(|| "unavailable".into())
            );
        }
        let start = Instant::now();
        let fonts = bmz_font::resolve_font_fallbacks(bmz_font::FontCoverage::Japanese, &roots);
        println!(
            "pass={pass} fallback_ms={:.3} faces={}",
            start.elapsed().as_secs_f64() * 1000.0,
            fonts.len()
        );
    }
}
