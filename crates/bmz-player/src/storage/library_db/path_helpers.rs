use super::*;

pub(super) fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Filesystem path key persisted in library.db.
///
/// Windows accepts both `\` and `/` as separators and `canonicalize()` can add
/// an extended-length prefix. Select navigation stores ordinary `/` paths, so
/// a partial rescan can otherwise rediscover a file under a different key.
pub(super) fn path_key(path: &Path) -> String {
    to_folder_key(&path_to_string(path))
}

pub(super) fn chart_file_path_candidates(path: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |value: String| {
        if !value.is_empty() && !out.contains(&value) {
            out.push(value);
        }
    };
    push(path_key(path));
    if let Ok(canonical) =
        crate::chart_source::ChartLocator::parse(path).and_then(|locator| locator.canonicalize())
    {
        push(path_key(&canonical.to_path_buf()));
    }
    out
}

/// `charts.folder_path` はスラッシュ `/` を正準とする。
/// Windows のバックスラッシュ区切りをスラッシュに変換し、extended-length
/// path prefix を取り除く。
pub(super) fn to_folder_key(path: &str) -> String {
    normalize_library_path(path)
}

/// Escapes SQL LIKE wildcards so user input is matched literally.
/// Pair with `LIKE ? ESCAPE '\\'`.
pub(super) fn escape_like(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

pub(super) fn folder_path(path: &Path) -> String {
    path.parent().map(path_key).unwrap_or_default()
}

pub(super) fn warning_details(warning: &ImportWarning) -> (String, String) {
    match warning {
        ImportWarning::EncodingFallback => {
            ("EncodingFallback".into(), "decoded chart as Shift_JIS".to_string())
        }
        ImportWarning::TextReplacementOccurred => {
            ("TextReplacementOccurred".into(), "text decoder replaced invalid bytes".to_string())
        }
        ImportWarning::ParserDiagnostic { code, message } => {
            // bms-rs から細分化済みの code をそのまま `chart_import_warnings.code` に保存する。
            (code.clone(), message.clone())
        }
        ImportWarning::UnsupportedCommand { command } => {
            ("UnsupportedCommand".into(), format!("unsupported command: {command}"))
        }
        ImportWarning::UnsupportedChannel { channel } => {
            ("UnsupportedChannel".into(), format!("unsupported channel: {channel}"))
        }
        ImportWarning::UnsupportedPmsPlayerSide { side } => {
            ("UnsupportedPmsPlayerSide".into(), format!("unsupported PMS player side: {side}"))
        }
        ImportWarning::MissingWavDefinition { key } => {
            ("MissingWavDefinition".into(), format!("missing WAV definition: {key}"))
        }
        ImportWarning::MissingSoundFile { path } => {
            ("MissingSoundFile".into(), format!("missing sound file: {}", path_to_string(path)))
        }
        ImportWarning::MissingBmpDefinition { key } => {
            ("MissingBmpDefinition".into(), format!("missing BMP definition: {key}"))
        }
        ImportWarning::MissingBmpFile { path } => {
            ("MissingBmpFile".into(), format!("missing BMP file: {}", path_to_string(path)))
        }
        ImportWarning::MissingBpmDefinition { key } => {
            ("MissingBpmDefinition".into(), format!("missing BPM definition: {key}"))
        }
        ImportWarning::MissingStopDefinition { key } => {
            ("MissingStopDefinition".into(), format!("missing STOP definition: {key}"))
        }
        ImportWarning::LnobjWithoutStart { lane } => {
            ("LnobjWithoutStart".into(), format!("LNOBJ without start on lane {lane:?}"))
        }
        ImportWarning::UnterminatedLongNote { lane } => {
            ("UnterminatedLongNote".into(), format!("unterminated long note on lane {lane:?}"))
        }
    }
}
