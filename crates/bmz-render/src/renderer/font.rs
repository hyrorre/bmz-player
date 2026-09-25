use super::*;

#[cfg(test)]
pub(super) fn load_default_font() -> Option<FontArc> {
    load_default_font_fallbacks(bmz_font::FontCoverage::Japanese, &[])
        .primary()
        .map(|face| face.font.clone())
}

pub(super) fn load_default_font_fallbacks(
    preferred: bmz_font::FontCoverage,
    font_roots: &[PathBuf],
) -> FontFallbackChain {
    let started_at = std::time::Instant::now();
    let mut resolved = bmz_font::resolve_font_fallbacks(preferred, font_roots);
    if let Some(general) = bmz_font::resolve_system_font(false)
        && !resolved.iter().any(|(_, font)| font == &general)
    {
        resolved.push((preferred, general));
    }

    let mut faces = Vec::new();
    for (index, (coverage, resolved)) in resolved.iter().enumerate() {
        if let Some(font) = load_font_from_resolved(resolved) {
            faces.push(FontFallbackFace {
                cache_id: format!("{DEFAULT_TEXT_FONT_ID}:{coverage:?}:{index}"),
                font,
            });
        }
    }

    if faces.is_empty() {
        tracing::warn!("no default render font found; text draw commands will be skipped");
    } else if !faces
        .iter()
        .any(|face| preferred.glyph_probes().iter().all(|ch| face.font.glyph_id(*ch).0 != 0))
    {
        tracing::warn!(
            ?preferred,
            "no font matching preferred CJK coverage; default text will use other fallback faces"
        );
    }

    tracing::info!(
        ?preferred,
        font_count = faces.len(),
        font_load_ms = started_at.elapsed().as_millis(),
        "default render font fallbacks loaded"
    );
    FontFallbackChain { faces }
}

pub(super) fn load_font_from_resolved(resolved: &bmz_font::ResolvedFont) -> Option<FontArc> {
    let source = bmz_font::resolved_font_source(resolved);
    let bytes = bmz_font::read_resolved_font_bytes(resolved).ok()?;
    match FontVec::try_from_vec_and_index(bytes, resolved.font_index) {
        Ok(font) => Some(FontArc::from(font)),
        Err(error) => {
            tracing::warn!(%error, source, "failed to load default render font");
            None
        }
    }
}

/// egui など外部 UI 向けに、日本語表示が可能なフォントファイルの生バイト列を返す。
///
/// OS フォント DB から CJK 対応 face を font-kit 経由で解決し、
/// collection index 付きでファイル全体を返す。
pub fn load_japanese_font_bytes() -> Option<Vec<u8>> {
    load_font_bytes_for_coverage(bmz_font::FontCoverage::Japanese)
}

/// egui など外部 UI に渡せる OS フォントデータ。
///
/// TTC/OTC では `font_index` を `egui::FontData::index` などへ引き継ぐ必要がある。
#[derive(Debug, Clone)]
pub struct SystemFontData {
    pub bytes: Vec<u8>,
    pub font_index: u32,
}

/// egui など外部 UI 向けに、指定 coverage を満たす OS フォントの生バイト列を返す。
pub fn load_font_bytes_for_coverage(coverage: bmz_font::FontCoverage) -> Option<Vec<u8>> {
    load_system_font_data_for_coverage(coverage).map(|data| data.bytes)
}

/// 指定 coverage を満たす OS フォントを collection index 付きで返す。
pub fn load_system_font_data_for_coverage(
    coverage: bmz_font::FontCoverage,
) -> Option<SystemFontData> {
    let resolved = bmz_font::resolve_system_font_for_coverage(coverage)?;
    let bytes = bmz_font::read_resolved_font_bytes(&resolved).ok()?;
    if bmz_font::font_supports_coverage(&bytes, resolved.font_index, coverage) {
        Some(SystemFontData { bytes, font_index: resolved.font_index })
    } else {
        tracing::warn!(?coverage, "no matching font found for egui; text may render as tofu");
        None
    }
}

/// 優先 coverage を先頭にして、利用可能な全 CJK fallback font を返す。
///
/// `font_roots` の同梱フォントを OS フォントより優先する。egui のフォント定義と
/// ゲーム/スキン描画の fallback 順を一致させるために使用する。
pub fn load_cjk_font_fallback_data(
    preferred: bmz_font::FontCoverage,
    font_roots: &[PathBuf],
) -> Vec<(bmz_font::FontCoverage, SystemFontData)> {
    bmz_font::resolve_font_fallbacks(preferred, font_roots)
        .into_iter()
        .filter_map(|(coverage, resolved)| {
            let bytes = bmz_font::read_resolved_font_bytes(&resolved).ok()?;
            Some((coverage, SystemFontData { bytes, font_index: resolved.font_index }))
        })
        .collect()
}

pub(super) fn block_on<T>(future: impl Future<Output = T>) -> T {
    let waker = noop_waker();
    let mut context = TaskContext::from_waker(&waker);
    let mut future = Box::pin(future);

    loop {
        match Pin::new(&mut future).poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => thread::yield_now(),
        }
    }
}

pub(super) fn noop_waker() -> Waker {
    unsafe fn clone(_: *const ()) -> RawWaker {
        noop_raw_waker()
    }
    unsafe fn wake(_: *const ()) {}
    unsafe fn wake_by_ref(_: *const ()) {}
    unsafe fn drop(_: *const ()) {}

    fn noop_raw_waker() -> RawWaker {
        RawWaker::new(std::ptr::null(), &RawWakerVTable::new(clone, wake, wake_by_ref, drop))
    }

    // SAFETY: The vtable functions do not dereference the null data pointer.
    unsafe { Waker::from_raw(noop_raw_waker()) }
}
