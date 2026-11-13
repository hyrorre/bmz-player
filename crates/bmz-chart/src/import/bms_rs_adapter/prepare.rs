//! One line scan of the decoded BMS text before bms-rs parses it.
//!
//! Each line goes through these steps in order, and the scan also collects what
//! BMZ reads from the source itself instead of from bms-rs:
//!
//! 1. Raw headers and whether the chart has `#RANDOM` / `#SWITCH`, from the original
//!    line.
//! 2. Standard PMS drops BME-type upper channels, which removes the line and does not
//!    count it in line numbers.
//! 3. beatoraja separators are normalized ([`normalize_beatoraja_separator_line`]).
//! 4. `#RANDOM` / `#SWITCH` are resolved. `#LNOBJ` is read from the resolved line.
//! 5. Empty metadata commands and `#LNOBJ` are removed.
//! 6. BGA channels, `#TEXTxx` and channel #99 are collected, and sparse messages are
//!    replaced by markers.
//!
//! Removed lines stay as empty lines so that line numbers in warnings match the source.
//! These steps were once separate whole-text passes. Each one wrote its lines back with
//! `\n` and the next read them with `str::lines`, which drops one more trailing `\r` per
//! step, so [`strip_cr`] between the steps keeps lines ending in `\r\r\n` as they were.

use super::*;

/// The decoded text prepared for bms-rs, and what BMZ reads from it directly.
#[derive(Debug, Default)]
pub(super) struct PreparedBmsSource {
    /// The text bms-rs parses.
    pub(super) parse_text: String,
    pub(super) has_bms_random: bool,
    /// Headers of the original text, before `#RANDOM` resolution.
    pub(super) bms_headers: BTreeMap<String, String>,
    pub(super) bga_messages: Vec<BgaMessage>,
    pub(super) source_texts: SourceTexts,
    pub(super) sparse_messages: Vec<SparseBmsMessage>,
    pub(super) lnobj_lines: Vec<LnobjLine>,
}

/// Prepares `raw_text` for bms-rs. `pms_standard` drops the BME-type upper channels
/// that the standard PMS layout ignores.
pub(super) fn prepare_bms_source(
    raw_text: &str,
    pms_standard: bool,
    random_source: &BmsRandomSource,
    bms_random_choices: &mut Vec<i32>,
    bms_switch_choices: &mut Vec<u64>,
    warnings: &mut Vec<ImportWarning>,
) -> PreparedBmsSource {
    let mut prepared = PreparedBmsSource {
        parse_text: String::with_capacity(raw_text.len() + 1),
        ..PreparedBmsSource::default()
    };
    let mut random = BeatorajaRandomResolver::new(random_source);
    // Sparse message warnings follow every `#RANDOM` warning.
    let mut sparse_warnings = Vec::new();
    let mut line_number = 0;

    for raw_line in raw_text.lines() {
        insert_bms_header_line(&mut prepared.bms_headers, raw_line);
        prepared.has_bms_random = prepared.has_bms_random || declares_bms_random(raw_line);
        if pms_standard && is_pms_bme_upper_channel_line(raw_line) {
            continue;
        }
        line_number += 1;

        let normalized = normalize_beatoraja_separator_line(raw_line);
        let line = strip_cr(&normalized);
        let line = if random.keep_line(
            line,
            line_number,
            bms_random_choices,
            bms_switch_choices,
            warnings,
        ) {
            strip_cr(line)
        } else {
            ""
        };
        if let Some(args) = lnobj_command_args(line) {
            prepared.lnobj_lines.push(LnobjLine { line_number, args: args.to_string() });
        }
        let line = if is_empty_metadata_command(line) { "" } else { strip_cr(line) };
        let line = if lnobj_command_args(line).is_some() { "" } else { strip_cr(line) };

        let is_text_definition = prepared.source_texts.push_definition_line(line);
        let message = ObjectMessageLine::parse(line);
        if let Some(message) = &message {
            if message.channel == "99" {
                if !is_text_definition {
                    prepared.source_texts.push_message(message);
                }
            } else if let Some(kind) = bga_kind_from_channel(message.channel)
                && let Some((measure, object_count, objects)) = message.objects()
            {
                prepared.bga_messages.push(BgaMessage {
                    line_number,
                    measure,
                    kind,
                    object_count,
                    objects,
                });
            }
        }
        let sparse_id = prepared.sparse_messages.len();
        match message.and_then(|message| message.sparse_message(line_number, sparse_id)) {
            Some(sparse) => {
                sparse_warnings.push(sparse_message_warning(&sparse));
                push_sparse_marker(&mut prepared.parse_text, sparse.id);
                prepared.sparse_messages.push(sparse);
            }
            None => prepared.parse_text.push_str(line),
        }
        prepared.parse_text.push('\n');
    }

    random.finish(warnings);
    warnings.extend(sparse_warnings);
    prepared
}

/// Resolve static branches before Kaleid compilation; choices are consumed once.
pub(super) fn resolve_kaleid_source(
    raw_text: &str,
    pms_standard: bool,
    random_source: &BmsRandomSource,
    bms_random_choices: &mut Vec<i32>,
    bms_switch_choices: &mut Vec<u64>,
    warnings: &mut Vec<ImportWarning>,
) -> String {
    let mut text = String::with_capacity(raw_text.len());
    let mut random = BeatorajaRandomResolver::new(random_source);
    let mut line_number = 0;
    for raw_line in raw_text.lines() {
        if pms_standard && is_pms_bme_upper_channel_line(raw_line) {
            continue;
        }
        line_number += 1;
        let normalized = normalize_beatoraja_separator_line(raw_line);
        let line = strip_cr(&normalized);
        if random.keep_line(line, line_number, bms_random_choices, bms_switch_choices, warnings) {
            text.push_str(strip_cr(line));
        }
        text.push('\n');
    }
    random.finish(warnings);
    text
}

/// What `str::lines` leaves of `line + "\n"`.
fn strip_cr(line: &str) -> &str {
    line.strip_suffix('\r').unwrap_or(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prepare(
        text: &str,
        pms_standard: bool,
        random_source: &BmsRandomSource,
    ) -> (PreparedBmsSource, Vec<i32>, Vec<String>) {
        let mut random_choices = Vec::new();
        let mut warnings = Vec::new();
        let prepared = prepare_bms_source(
            text,
            pms_standard,
            random_source,
            &mut random_choices,
            &mut Vec::new(),
            &mut warnings,
        );
        let codes = warnings
            .iter()
            .map(|warning| match warning {
                ImportWarning::ParserDiagnostic { code, .. } => code.clone(),
                other => format!("{other:?}"),
            })
            .collect();
        (prepared, random_choices, codes)
    }

    #[test]
    fn removed_lines_stay_empty_and_keep_line_numbers() {
        let sparse = format!("#00211:{}01", "00".repeat(SPARSE_BMS_MESSAGE_OBJECT_THRESHOLD));
        let text = [
            "#TITLE",
            "#RANDOM 2",
            "#IF 1",
            "#00111:01",
            "#ENDIF",
            "#ENDRANDOM",
            "#LNOBJ ZZ",
            "#00104:0100",
            "#00112 0101",
            &sparse,
            "#TEXT01 hi",
            "#00199:01",
        ]
        .join("\n");
        let random_source = BmsRandomSource::Choices { random: vec![1, 2], switches: Vec::new() };

        let (prepared, random_choices, warnings) = prepare(&text, false, &random_source);

        assert_eq!(
            prepared.parse_text,
            "\n\n\n#00111:01\n\n\n\n#00104:0100\n#00112:0101\n#BMZSPARSE 0\n#TEXT01 hi\n#00199:01\n"
        );
        assert!(prepared.has_bms_random);
        assert_eq!(random_choices, [1]);
        // Warnings about unused choices come after the last line, before sparse messages.
        assert_eq!(warnings, ["BmsRandomChoiceExtra", "SparseBmsMessage"]);
        assert_eq!(prepared.sparse_messages[0].line_number, 10);
        assert_eq!(prepared.bga_messages.len(), 1);
        assert_eq!(prepared.bga_messages[0].line_number, 8);
        assert_eq!(
            prepared.lnobj_lines.iter().map(|line| line.line_number).collect::<Vec<_>>(),
            [7]
        );
        assert_eq!(prepared.bms_headers.get("TITLE").map(String::as_str), Some(""));
        assert_eq!(prepared.bms_headers.get("TEXT01").map(String::as_str), Some("hi"));
        assert!(!prepared.bms_headers.contains_key("00111"));
    }

    #[test]
    fn each_step_drops_one_trailing_carriage_return() {
        let text = "#00111:01\r\r\r\r\r\r\n#TITLE\r\r\n#00112 01\r\r\n#LNOBJ 01\r\r\r\n";

        let (prepared, _, _) = prepare(text, false, &BmsRandomSource::Seed(None));

        // `str::lines` and the four steps after it each dropped one `\r`.
        assert_eq!(prepared.parse_text, "#00111:01\r\n\n#00112:01\n\n");
        assert_eq!(prepared.lnobj_lines[0].args, "01");
    }

    #[test]
    fn standard_pms_drops_bme_upper_channels_without_counting_lines() {
        let text = "#00116:01\n#00111:0100\n#00104:01\n";

        let (prepared, _, _) = prepare(text, true, &BmsRandomSource::Seed(None));

        assert_eq!(prepared.parse_text, "#00111:0100\n#00104:01\n");
        assert_eq!(prepared.bga_messages[0].line_number, 2);
    }
}
