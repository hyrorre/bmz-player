use std::{
    collections::VecDeque,
    fmt::{self, Write as _},
    sync::{Arc, Mutex},
};

use tracing::{Event, Level, Subscriber, field::Visit};
use tracing_subscriber::{
    layer::{Context, Layer},
    registry::LookupSpan,
};

/// デバッグ表示に保持するログの最大件数。
pub const DEFAULT_LOG_CAPACITY: usize = 1_000;
const MAX_LOG_TARGET_CHARS: usize = 128;
const MAX_LOG_MESSAGE_CHARS: usize = 4_096;

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
        self.state.lock().expect("log buffer mutex poisoned").entries.iter().cloned().collect()
    }

    pub fn clear(&self) {
        self.state.lock().expect("log buffer mutex poisoned").entries.clear();
    }

    fn push(&self, mut entry: LogEntry) {
        entry.target = truncate_chars(entry.target, MAX_LOG_TARGET_CHARS);
        entry.message = truncate_chars(entry.message, MAX_LOG_MESSAGE_CHARS);
        let mut state = self.state.lock().expect("log buffer mutex poisoned");
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
    use tracing_subscriber::prelude::*;

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
        let mut visitor = EventVisitor::default();
        visitor.message = Some("started".to_string());
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
