use std::path::PathBuf;

use bmz_core::lane::Lane;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("failed to read chart file: {path}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to decode chart text: {path}")]
    Decode { path: PathBuf },

    #[error("failed to parse chart: {path}: {message}")]
    Parse { path: PathBuf, message: String },

    #[error("invalid chart structure: {message}")]
    InvalidChart { message: String },

    #[error("unsupported chart mode: {mode}")]
    UnsupportedMode { mode: String },

    #[error("invalid timing data: {message}")]
    InvalidTiming { message: String },

    #[error("invalid long note data: {message}")]
    InvalidLongNote { message: String },
}

#[derive(Debug, Clone)]
pub enum ImportWarning {
    EncodingFallback,
    TextReplacementOccurred,
    /// bms-rs から返される [`bms_rs::bms::BmsWarning`] を表示用にラップしたもの。
    /// `code` は安定した分類タグ (e.g. `ParseSyntaxError`, `PlayingTotalUndefined`)、
    /// `message` は人間向け詳細。`library_db` の `chart_import_warnings` テーブルへ
    /// `code` がそのまま入る。
    ParserDiagnostic {
        code: String,
        message: String,
    },
    /// 内部使用: 将来別 parser を呼ぶ場合用に残してある汎用フォールバック。
    UnsupportedCommand {
        command: String,
    },
    UnsupportedChannel {
        channel: u16,
    },
    /// PMS 18K 等、現状未対応のプレイヤー側ノート。
    UnsupportedPmsPlayerSide {
        side: u8,
    },
    MissingWavDefinition {
        key: u16,
    },
    MissingSoundFile {
        path: PathBuf,
    },
    MissingBmpDefinition {
        key: u16,
    },
    MissingBmpFile {
        path: PathBuf,
    },
    MissingBpmDefinition {
        key: u16,
    },
    MissingStopDefinition {
        key: u16,
    },
    LnobjWithoutStart {
        lane: Lane,
    },
    UnterminatedLongNote {
        lane: Lane,
    },
}
