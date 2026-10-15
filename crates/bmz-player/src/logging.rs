use std::{
    backtrace::{Backtrace, BacktraceStatus},
    collections::VecDeque,
    fmt::{self, Write as _},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    sync::{Arc, Mutex},
};

use anyhow::{Context as _, Result};
use serde::Deserialize;
use tracing::{Event, Level, Subscriber, field::Visit};
use tracing_appender::{
    non_blocking::{ErrorCounter, NonBlocking, NonBlockingBuilder, WorkerGuard},
    rolling::{RollingFileAppender, Rotation},
};
use tracing_subscriber::{
    EnvFilter,
    layer::{Context, Layer, SubscriberExt},
    registry::LookupSpan,
    util::SubscriberInitExt,
};

use crate::{config::app_config::LogLevel as ConfigLogLevel, paths::AppPaths};

/// デバッグ表示に保持するログの最大件数。
pub const DEFAULT_LOG_CAPACITY: usize = 1_000;
const MAX_LOG_TARGET_CHARS: usize = 128;
const MAX_LOG_MESSAGE_CHARS: usize = 4_096;
const FILE_LOG_BUFFERED_LINES: usize = 8_192;
const FILE_LOG_RETENTION: usize = 10;
const FILE_LOG_PREFIX: &str = "bmz-player";
const FILE_LOG_SUFFIX: &str = "log";

/// 起動時に副作用なしで読み取る `[logging]` の有効値。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupLoggingConfig {
    /// `None` は設定にlevelがなく、既定のinfoを使うことを表す。
    pub configured_level: Option<ConfigLogLevel>,
    pub file_logging: bool,
}

impl Default for StartupLoggingConfig {
    fn default() -> Self {
        Self { configured_level: None, file_logging: true }
    }
}

#[derive(Debug, Default, Deserialize)]
struct StartupConfigDocument {
    logging: Option<StartupLoggingSection>,
}

#[derive(Debug, Default, Deserialize)]
struct StartupLoggingSection {
    level: Option<ConfigLogLevel>,
    file_logging: Option<bool>,
}

/// 通常のconfig migrationや保存を行わず、既存ファイルの`[logging]`だけを読む。
pub fn load_startup_logging_config(path: &Path) -> Result<StartupLoggingConfig> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(StartupLoggingConfig::default());
        }
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", path.display()));
        }
    };
    parse_startup_logging_config(&text)
        .with_context(|| format!("failed to parse startup logging config from {}", path.display()))
}

fn parse_startup_logging_config(text: &str) -> Result<StartupLoggingConfig> {
    let document: StartupConfigDocument = toml::from_str(text)?;
    let logging = document.logging.unwrap_or_default();
    Ok(StartupLoggingConfig {
        configured_level: logging.level,
        file_logging: logging.file_logging.unwrap_or(true),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterSource {
    RustLog,
    Config,
    Default,
}

impl FilterSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RustLog => "RUST_LOG",
            Self::Config => "config",
            Self::Default => "default",
        }
    }
}

#[derive(Debug)]
pub struct FilterSelection {
    filter: EnvFilter,
    pub display: String,
    pub source: FilterSource,
    pub invalid_rust_log: Option<String>,
}

/// configのログレベルを`EnvFilter` directiveへ変換する。
pub const fn log_level_directive(level: ConfigLogLevel) -> &'static str {
    match level {
        ConfigLogLevel::Trace => "trace",
        ConfigLogLevel::Debug => "debug",
        ConfigLogLevel::Info => "info",
        ConfigLogLevel::Warn => "warn",
        ConfigLogLevel::Error => "error",
    }
}

/// 環境変数を書き換えずに、`RUST_LOG` > config > infoの優先順位を決める。
pub fn select_filter(
    rust_log: Option<&str>,
    configured_level: Option<ConfigLogLevel>,
) -> FilterSelection {
    let mut invalid_rust_log = None;
    if let Some(candidate) = rust_log {
        let candidate = candidate.trim();
        if !candidate.is_empty() {
            match EnvFilter::try_new(candidate) {
                Ok(filter) => {
                    return FilterSelection {
                        filter,
                        display: candidate.to_string(),
                        source: FilterSource::RustLog,
                        invalid_rust_log: None,
                    };
                }
                Err(error) => invalid_rust_log = Some(error.to_string()),
            }
        } else {
            invalid_rust_log = Some("value is empty".to_string());
        }
    }

    let (level, source) = configured_level
        .map(|level| (level, FilterSource::Config))
        .unwrap_or((ConfigLogLevel::Info, FilterSource::Default));
    let directive = log_level_directive(level);
    FilterSelection {
        filter: EnvFilter::new(directive),
        display: directive.to_string(),
        source,
        invalid_rust_log,
    }
}

struct FileLoggingOutput {
    writer: NonBlocking,
    guard: WorkerGuard,
    error_counter: ErrorCounter,
}

struct FileLoggingPreparation {
    output: Option<FileLoggingOutput>,
    error: Option<String>,
}

fn prepare_file_logging(enabled: bool, logs_dir: &Path) -> FileLoggingPreparation {
    if !enabled {
        return FileLoggingPreparation { output: None, error: None };
    }

    match create_file_logging(logs_dir) {
        Ok(output) => FileLoggingPreparation { output: Some(output), error: None },
        Err(error) => FileLoggingPreparation { output: None, error: Some(format!("{error:#}")) },
    }
}

fn create_file_logging(logs_dir: &Path) -> Result<FileLoggingOutput> {
    std::fs::create_dir_all(logs_dir)
        .with_context(|| format!("failed to create logs directory {}", logs_dir.display()))?;
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(FILE_LOG_PREFIX)
        .filename_suffix(FILE_LOG_SUFFIX)
        .max_log_files(FILE_LOG_RETENTION)
        .build(logs_dir)
        .with_context(|| format!("failed to create rolling log in {}", logs_dir.display()))?;

    // 描画・入力・音声threadへfile I/Oのbackpressureを掛けない。キュー飽和時は
    // 診断性よりリアルタイム性を優先して新規行をdropし、件数を終了時に記録する。
    let (writer, guard) = NonBlockingBuilder::default()
        .buffered_lines_limit(FILE_LOG_BUFFERED_LINES)
        .lossy(true)
        .thread_name("bmz-file-logger")
        .finish(appender);
    let error_counter = writer.error_counter();
    Ok(FileLoggingOutput { writer, guard, error_counter })
}

/// subscriberとnon-blocking file workerを最上位で所有する初期化結果。
pub struct LoggingRuntime {
    pub log_buffer: LogBuffer,
    pub file_guard: Option<WorkerGuard>,
    pub filter: String,
    pub filter_source: FilterSource,
    pub file_logging_enabled: bool,
    pub logs_dir: PathBuf,
    file_error_counter: Option<ErrorCounter>,
}

impl LoggingRuntime {
    pub fn dropped_file_lines(&self) -> usize {
        self.file_error_counter.as_ref().map_or(0, ErrorCounter::dropped_lines)
    }
}

/// stderr・GUI buffer・任意の永続file layerを同じfilterで初期化する。
pub fn initialize_logging(
    app_paths: &AppPaths,
    startup: StartupLoggingConfig,
    rust_log: Option<&str>,
) -> Result<LoggingRuntime> {
    let selection = select_filter(rust_log, startup.configured_level);
    if let Some(error) = &selection.invalid_rust_log {
        crate::stdio::stderr_line(format_args!(
            "Warning: invalid RUST_LOG ({error}); falling back to startup logging level"
        ));
    }

    let preparation = prepare_file_logging(startup.file_logging, &app_paths.logs_dir);
    if let Some(error) = &preparation.error {
        crate::stdio::stderr_line(format_args!("Warning: file logging is disabled: {error}"));
    }

    let log_buffer = LogBuffer::default();
    let (file_writer, file_guard, file_error_counter) = match preparation.output {
        Some(output) => (Some(output.writer), Some(output.guard), Some(output.error_counter)),
        None => (None, None, None),
    };
    let file_logging_enabled = file_guard.is_some();
    let file_layer = file_writer.map(|writer| {
        tracing_subscriber::fmt::layer()
            .with_writer(writer)
            .with_ansi(false)
            .with_target(true)
            .with_thread_names(true)
            .with_thread_ids(true)
    });

    tracing_subscriber::registry()
        .with(selection.filter)
        .with(tracing_subscriber::fmt::layer().with_writer(crate::stdio::SafeStderr))
        .with(LogBufferLayer::new(log_buffer.clone()))
        .with(file_layer)
        .try_init()
        .context("failed to initialize tracing subscriber")?;

    if let Some(error) = &selection.invalid_rust_log {
        tracing::warn!(%error, "invalid RUST_LOG; using startup logging level");
    }
    if let Some(error) = &preparation.error {
        tracing::warn!(logs_dir = %app_paths.logs_dir.display(), %error, "file logging is disabled");
    }

    Ok(LoggingRuntime {
        log_buffer,
        file_guard,
        filter: selection.display,
        filter_source: selection.source,
        file_logging_enabled,
        logs_dir: app_paths.logs_dir.clone(),
        file_error_counter,
    })
}

pub fn log_session_start(runtime: &LoggingRuntime) {
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        os = std::env::consts::OS,
        architecture = std::env::consts::ARCH,
        build = if cfg!(debug_assertions) { "debug" } else { "release" },
        process_id = std::process::id(),
        filter = %runtime.filter,
        filter_source = runtime.filter_source.as_str(),
        file_logging = runtime.file_logging_enabled,
        logs_dir = %runtime.logs_dir.display(),
        "BMZ Player session started"
    );
}

pub fn log_session_end(runtime: &LoggingRuntime, succeeded: bool) {
    tracing::info!(
        status = if succeeded { "success" } else { "error" },
        file_logging = runtime.file_logging_enabled,
        dropped_file_lines = runtime.dropped_file_lines(),
        "BMZ Player session ended"
    );
}

/// 既存hookをchainしつつ、通常panicをtracingとstderrへbest-effortで残す。
pub fn install_panic_hook() {
    static IN_PANIC_HOOK: AtomicBool = AtomicBool::new(false);

    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if IN_PANIC_HOOK.swap(true, Ordering::AcqRel) {
            crate::stdio::stderr_line(format_args!("panic while reporting another panic"));
            return;
        }

        struct Reset<'a>(&'a AtomicBool);
        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }
        let reset = Reset(&IN_PANIC_HOOK);

        let payload = panic_payload(info.payload());
        let location = info
            .location()
            .map(|location| {
                format!("{}:{}:{}", location.file(), location.line(), location.column())
            })
            .unwrap_or_else(|| "unknown".to_string());
        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("<unnamed>").to_string();
        let thread_id = format!("{:?}", thread.id());
        let backtrace = Backtrace::capture();

        crate::stdio::stderr_line(format_args!(
            "panic: {payload}; location={location}; thread={thread_name} {thread_id}"
        ));
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if backtrace.status() == BacktraceStatus::Captured {
                tracing::error!(
                    target: "bmz_player::panic",
                    panic_payload = %payload,
                    %location,
                    %thread_name,
                    %thread_id,
                    backtrace = %backtrace,
                    "panic captured"
                );
            } else {
                tracing::error!(
                    target: "bmz_player::panic",
                    panic_payload = %payload,
                    %location,
                    %thread_name,
                    %thread_id,
                    "panic captured"
                );
            }
        }));
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| previous(info)));
        drop(reset);
    }));
}

fn panic_payload(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub const ALL: [Self; 5] = [Self::Trace, Self::Debug, Self::Info, Self::Warn, Self::Error];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }

    fn from_tracing(level: &Level) -> Self {
        match *level {
            Level::TRACE => Self::Trace,
            Level::DEBUG => Self::Debug,
            Level::WARN => Self::Warn,
            Level::ERROR => Self::Error,
            Level::INFO => Self::Info,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub level: LogLevel,
    pub target: String,
    pub message: String,
}

#[derive(Debug)]
struct LogBufferState {
    entries: VecDeque<LogEntry>,
    capacity: usize,
}

/// tracing イベントを UI から読める bounded バッファへ保持する共有ハンドル。
#[derive(Clone, Debug)]
pub struct LogBuffer {
    state: Arc<Mutex<LogBufferState>>,
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self::new(DEFAULT_LOG_CAPACITY)
    }
}

impl LogBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            state: Arc::new(Mutex::new(LogBufferState {
                entries: VecDeque::with_capacity(capacity),
                capacity: capacity.max(1),
            })),
        }
    }

    pub fn snapshot(&self) -> Vec<LogEntry> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entries
            .iter()
            .cloned()
            .collect()
    }

    pub fn clear(&self) {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).entries.clear();
    }

    fn push(&self, mut entry: LogEntry) {
        entry.target = truncate_chars(entry.target, MAX_LOG_TARGET_CHARS);
        entry.message = truncate_chars(entry.message, MAX_LOG_MESSAGE_CHARS);
        // panic hookから再入した場合に、panic元が同じbuffer lockを保持していても
        // deadlockしない。通常時の短い競合でもGUI表示分だけをdropし、stderr/fileは維持する。
        let mut state = match self.state.try_lock() {
            Ok(state) => state,
            Err(std::sync::TryLockError::Poisoned(error)) => error.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return,
        };
        if state.entries.len() >= state.capacity {
            state.entries.pop_front();
        }
        state.entries.push_back(entry);
    }
}

/// 既存のコンソール出力と同じ tracing イベントを `LogBuffer` へ転送する Layer。
pub struct LogBufferLayer {
    buffer: LogBuffer,
}

impl LogBufferLayer {
    pub fn new(buffer: LogBuffer) -> Self {
        Self { buffer }
    }
}

impl<S> Layer<S> for LogBufferLayer
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut visitor = EventVisitor::default();
        event.record(&mut visitor);
        self.buffer.push(LogEntry {
            level: LogLevel::from_tracing(event.metadata().level()),
            target: event.metadata().target().to_string(),
            message: visitor.finish(),
        });
    }
}

#[derive(Default)]
struct EventVisitor {
    message: Option<String>,
    fields: Vec<(String, String)>,
}

impl EventVisitor {
    fn record_value(&mut self, field: &tracing::field::Field, value: String) {
        if field.name() == "message" {
            self.message = Some(value);
        } else {
            self.fields.push((field.name().to_string(), value));
        }
    }

    fn finish(self) -> String {
        let mut message = self.message.unwrap_or_default();
        for (name, value) in self.fields {
            if !message.is_empty() {
                message.push(' ');
            }
            let _ = write!(message, "{name}={value}");
        }
        message
    }
}

impl Visit for EventVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.record_value(field, value.to_string());
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        self.record_value(field, format!("{value:?}"));
    }
}

fn truncate_chars(value: String, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value;
    }
    let mut truncated = value.chars().take(max_chars).collect::<String>();
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_test_dir(label: &str) -> crate::test_dir::TestDir {
        crate::test_dir::TestDir::new(label)
    }

    #[test]
    fn startup_logging_reads_debug_and_disabled_file_output() {
        let config = parse_startup_logging_config(
            r#"
                version = 1
                unknown = "ignored"

                [logging]
                level = "debug"
                file_logging = false
                future_field = true
            "#,
        )
        .unwrap();

        assert_eq!(config.configured_level, Some(ConfigLogLevel::Debug));
        assert!(!config.file_logging);
    }

    #[test]
    fn startup_logging_defaults_when_section_is_missing() {
        let config = parse_startup_logging_config("version = 1").unwrap();

        assert_eq!(config, StartupLoggingConfig::default());
    }

    #[test]
    fn config_log_levels_map_to_filter_directives() {
        assert_eq!(log_level_directive(ConfigLogLevel::Trace), "trace");
        assert_eq!(log_level_directive(ConfigLogLevel::Debug), "debug");
        assert_eq!(log_level_directive(ConfigLogLevel::Info), "info");
        assert_eq!(log_level_directive(ConfigLogLevel::Warn), "warn");
        assert_eq!(log_level_directive(ConfigLogLevel::Error), "error");
    }

    #[test]
    fn valid_rust_log_takes_priority_over_config() {
        let selection =
            select_filter(Some("bmz_player=trace,wgpu=warn"), Some(ConfigLogLevel::Error));

        assert_eq!(selection.source, FilterSource::RustLog);
        assert_eq!(selection.display, "bmz_player=trace,wgpu=warn");
        assert!(selection.invalid_rust_log.is_none());
    }

    #[test]
    fn config_filter_is_used_without_rust_log() {
        let selection = select_filter(None, Some(ConfigLogLevel::Debug));

        assert_eq!(selection.source, FilterSource::Config);
        assert_eq!(selection.display, "debug");
    }

    #[test]
    fn invalid_rust_log_falls_back_to_config() {
        let selection = select_filter(Some("[invalid"), Some(ConfigLogLevel::Warn));

        assert_eq!(selection.source, FilterSource::Config);
        assert_eq!(selection.display, "warn");
        assert!(selection.invalid_rust_log.is_some());
    }

    #[test]
    fn info_filter_is_used_without_rust_log_or_config() {
        let selection = select_filter(None, None);

        assert_eq!(selection.source, FilterSource::Default);
        assert_eq!(selection.display, "info");
    }

    #[test]
    fn file_logging_flushes_plain_text_event_on_guard_drop() {
        let logs_dir = unique_test_dir("bmz-file-log");
        let output = create_file_logging(&logs_dir).unwrap();
        let FileLoggingOutput { writer, guard, .. } = output;
        let subscriber = tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .with_writer(writer)
                .with_ansi(false)
                .with_target(true)
                .with_thread_names(true)
                .with_thread_ids(true),
        );

        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "bmz_player::file_test", answer = 42, "persistent event");
        });
        drop(guard);

        let files = std::fs::read_dir(&logs_dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(files.len(), 1);
        let file_name = files[0].file_name().unwrap().to_string_lossy();
        assert!(file_name.starts_with("bmz-player."));
        assert!(file_name.ends_with(".log"));
        let text = std::fs::read_to_string(&files[0]).unwrap();
        assert!(text.contains("persistent event"));
        assert!(text.contains("INFO"));
        assert!(text.contains("bmz_player::file_test"));
        assert!(text.contains("answer=42"));
        assert!(!text.contains('\u{1b}'));
    }

    #[test]
    fn disabled_file_logging_does_not_create_directory_or_file() {
        let logs_dir = unique_test_dir("bmz-file-log-disabled");

        let preparation = prepare_file_logging(false, &logs_dir);

        assert!(preparation.output.is_none());
        assert!(preparation.error.is_none());
        assert!(!logs_dir.exists());
    }

    #[test]
    fn file_logging_initialization_failure_is_non_panicking() {
        let root = unique_test_dir("bmz-file-log-failure");
        std::fs::create_dir_all(&root).unwrap();
        let logs_dir = root.join("not-a-directory");
        std::fs::write(&logs_dir, b"block directory creation").unwrap();

        let preparation = std::panic::catch_unwind(|| prepare_file_logging(true, &logs_dir))
            .expect("file logger fallback must not panic");

        assert!(preparation.output.is_none());
        assert!(preparation.error.is_some());
    }

    #[test]
    fn rolling_file_logging_retains_at_most_ten_files() {
        let logs_dir = unique_test_dir("bmz-file-log-retention");
        std::fs::create_dir_all(&logs_dir).unwrap();
        for day in 1..=12 {
            std::fs::write(logs_dir.join(format!("bmz-player.2000-01-{day:02}.log")), b"old")
                .unwrap();
        }

        let output = create_file_logging(&logs_dir).unwrap();
        drop(output.writer);
        drop(output.guard);

        let retained = std::fs::read_dir(&logs_dir).unwrap().count();
        assert!(retained <= FILE_LOG_RETENTION, "retained {retained} files");
    }

    #[test]
    fn log_buffer_keeps_newest_entries_within_capacity() {
        let buffer = LogBuffer::new(2);
        for index in 0..3 {
            buffer.push(LogEntry {
                level: LogLevel::Info,
                target: "test".to_string(),
                message: index.to_string(),
            });
        }

        let entries = buffer.snapshot();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].message, "1");
        assert_eq!(entries[1].message, "2");
    }

    #[test]
    fn log_entry_text_contains_message_and_fields() {
        let mut visitor =
            EventVisitor { message: Some("started".to_string()), ..EventVisitor::default() };
        visitor.fields.push(("chart_id".to_string(), "42".to_string()));

        assert_eq!(visitor.finish(), "started chart_id=42");
    }

    #[test]
    fn log_buffer_layer_collects_tracing_events() {
        let buffer = LogBuffer::new(4);
        let subscriber = tracing_subscriber::registry().with(LogBufferLayer::new(buffer.clone()));

        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(chart_id = 42_u64, "slow frame");
        });

        let entry = buffer.snapshot().pop().expect("tracing event must be collected");
        assert_eq!(entry.level, LogLevel::Warn);
        assert!(entry.message.contains("slow frame"));
        assert!(entry.message.contains("chart_id=42"));
    }

    #[test]
    fn log_buffer_truncates_large_values() {
        let buffer = LogBuffer::new(1);
        buffer.push(LogEntry {
            level: LogLevel::Error,
            target: "t".repeat(MAX_LOG_TARGET_CHARS + 1),
            message: "m".repeat(MAX_LOG_MESSAGE_CHARS + 1),
        });

        let entry = buffer.snapshot().pop().expect("entry must exist");
        assert_eq!(entry.target.chars().count(), MAX_LOG_TARGET_CHARS + 1);
        assert_eq!(entry.message.chars().count(), MAX_LOG_MESSAGE_CHARS + 1);
        assert!(entry.target.ends_with('…'));
        assert!(entry.message.ends_with('…'));
    }
}
