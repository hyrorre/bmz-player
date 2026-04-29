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
    UnsupportedCommand { command: String },
    UnsupportedChannel { channel: u16 },
    MissingWavDefinition { key: u16 },
    MissingSoundFile { path: PathBuf },
    MissingBpmDefinition { key: u16 },
    MissingStopDefinition { key: u16 },
    SuspiciousMeasureLength { measure: u32 },
    LnobjWithoutStart { lane: Lane },
    UnterminatedLongNote { lane: Lane },
    ConflictingLongNoteSyntax { lane: Lane },
    DuplicateDefinition { name: String },
}
