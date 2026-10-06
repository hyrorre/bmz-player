use super::{ArchiveControl, ArchiveEntry, ArchiveLimits};
use anyhow::{Result, ensure};
use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};
use unicode_normalization::UnicodeNormalization;

pub(super) fn decode_name(bytes: &[u8], unicode: bool) -> Result<String> {
    if unicode {
        return Ok(std::str::from_utf8(bytes)?.to_owned());
    }
    if let Ok(name) = std::str::from_utf8(bytes) {
        return Ok(name.to_owned());
    }
    let (name, _, errors) = encoding_rs::SHIFT_JIS.decode(bytes);
    ensure!(!errors, "archive filename is neither UTF-8 nor CP932");
    Ok(name.into_owned())
}

pub(super) fn normalize_name(name: &str, directory: bool) -> Result<String> {
    let name = name.replace('\\', "/");
    let name = if directory { name.trim_end_matches('/') } else { &name };
    let name = name.strip_prefix("./").unwrap_or(name);
    ensure!(!name.is_empty() && name.len() <= 4096, "invalid archive path length");
    ensure!(
        !name
            .chars()
            .any(|c| c.is_control() || matches!(c, ':' | '<' | '>' | '"' | '|' | '?' | '*')),
        "invalid archive path character"
    );
    let parts: Vec<_> = name.split('/').collect();
    ensure!(parts.len() <= 64, "archive path is too deep");
    for part in parts {
        ensure!(!part.is_empty() && part != "." && part != "..", "archive path escapes its root");
        ensure!(!part.ends_with([' ', '.']), "ambiguous archive path suffix");
        let base = part.split('.').next().unwrap().to_ascii_uppercase();
        ensure!(
            !matches!(
                base.as_str(),
                "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$" | "CONIN$" | "CONOUT$"
            ) && !((base.starts_with("COM") || base.starts_with("LPT"))
                && matches!(
                    base.get(3..),
                    Some("1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³")
                )),
            "reserved archive path"
        );
    }
    Ok(name.to_owned())
}

pub(super) fn validate_entries(entries: &[ArchiveEntry], limits: &ArchiveLimits) -> Result<()> {
    ensure!(entries.len() <= limits.entries, "archive entry count exceeds limit");
    let mut names = HashMap::new();
    let mut components = HashMap::new();
    let mut total = 0u64;
    for entry in entries {
        ensure!(!entry.is_directory || entry.size == 0, "archive directory carries file data");
        ensure!(entry.size <= limits.file_bytes, "archive file bytes exceed limit");
        if entry.is_chart() {
            ensure!(entry.size <= limits.chart_bytes, "archive chart bytes exceed limit");
        }
        total = total
            .checked_add(entry.size)
            .ok_or_else(|| anyhow::anyhow!("archive size overflow"))?;
        ensure!(total <= limits.extracted_bytes, "archive extracted bytes exceed limit");
        ensure!(
            names.insert(collision_key(&entry.name), entry.is_directory).is_none(),
            "duplicate or case-colliding archive path: {}",
            entry.name
        );
        let mut prefix = entry.name.as_str();
        loop {
            if let Some(previous) = components.insert(collision_key(prefix), prefix) {
                ensure!(previous == prefix, "archive directory spelling collision");
            }
            let Some((parent, _)) = prefix.rsplit_once('/') else {
                break;
            };
            prefix = parent;
        }
    }
    for entry in entries {
        let mut parent = entry.name.as_str();
        while let Some((prefix, _)) = parent.rsplit_once('/') {
            ensure!(
                names.get(&collision_key(prefix)) != Some(&false),
                "archive file/directory collision"
            );
            parent = prefix;
        }
    }
    Ok(())
}

fn collision_key(name: &str) -> String {
    name.nfc().collect::<String>().to_lowercase()
}

pub(super) fn regular_mode(mode: u32, directory: bool) -> Result<()> {
    let kind = mode & 0o170000;
    ensure!(
        kind == 0 || kind == if directory { 0o040000 } else { 0o100000 },
        "archive links or special files are unsupported"
    );
    Ok(())
}

pub(super) struct Meters {
    total: u64,
    pub complete: bool,
}
impl Default for Meters {
    fn default() -> Self {
        Self { total: 0, complete: true }
    }
}

pub(super) struct MeteredWriter {
    writer: Box<dyn Write>,
    expected: u64,
    written: u64,
    control: ArchiveControl,
    meters: Arc<Mutex<Meters>>,
}
impl MeteredWriter {
    pub fn new(
        writer: Box<dyn Write>,
        entry: &ArchiveEntry,
        control: ArchiveControl,
        meters: Arc<Mutex<Meters>>,
    ) -> Self {
        Self { writer, expected: entry.size, written: 0, control, meters }
    }
}
impl Write for MeteredWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.control.check().map_err(std::io::Error::other)?;
        let mut meters = self.meters.lock().unwrap();
        if bytes.len() as u64 > self.expected.saturating_sub(self.written)
            || bytes.len() as u64 > self.control.limits.extracted_bytes.saturating_sub(meters.total)
        {
            return Err(std::io::Error::other("archive expanded beyond declared size or limit"));
        }
        let n = self.writer.write(bytes)?;
        self.written += n as u64;
        meters.total += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}
impl Drop for MeteredWriter {
    fn drop(&mut self) {
        if self.written != self.expected {
            self.meters.lock().unwrap().complete = false;
        }
    }
}
