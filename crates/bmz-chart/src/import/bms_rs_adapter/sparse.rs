use super::*;

pub(super) fn extract_sparse_bms_message_lines(
    text: &str,
    warnings: &mut Vec<ImportWarning>,
) -> (String, Vec<SparseBmsMessage>) {
    let mut rewritten = String::with_capacity(text.len());
    let mut sparse_messages = Vec::new();

    for (line_index, line) in text.lines().enumerate() {
        let line_number = line_index + 1;
        if let Some(message) =
            extract_sparse_bms_message_line(line, line_number, sparse_messages.len())
        {
            warnings.push(ImportWarning::ParserDiagnostic {
                code: "SparseBmsMessage".to_string(),
                message: format!(
                    "line {} #{}{} has {} slots and {} non-zero objects; importing sparsely",
                    message.line_number,
                    message.measure,
                    message.channel,
                    message.object_count,
                    message.objects.len()
                ),
            });
            rewritten.push('#');
            rewritten.push_str(SPARSE_BMS_MARKER_HEADER);
            rewritten.push(' ');
            rewritten.push_str(&message.id.to_string());
            sparse_messages.push(message);
        } else {
            rewritten.push_str(line);
        }
        rewritten.push('\n');
    }

    (rewritten, sparse_messages)
}

pub(super) fn extract_sparse_bms_message_line(
    line: &str,
    line_number: usize,
    sparse_id: usize,
) -> Option<SparseBmsMessage> {
    let trimmed = line.trim();
    let body = trimmed.strip_prefix('#')?;
    let colon = body.find(':')?;
    let head = &body[..colon];
    if head.len() < 5 || !head.is_ascii() {
        return None;
    }
    let measure_text = &head[..head.len() - 2];
    let channel = &head[head.len() - 2..];
    if channel.eq_ignore_ascii_case("02") {
        return None;
    }
    let payload = body[colon + 1..].trim();
    if payload.len() % 2 != 0 {
        return None;
    }
    let object_count = payload.len() / 2;
    if object_count <= SPARSE_BMS_MESSAGE_OBJECT_THRESHOLD {
        return None;
    }
    let measure = measure_text.parse::<u64>().ok()?;
    let mut objects = Vec::new();
    for (index, chunk) in payload.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        if chunk != b"00" {
            let id = std::str::from_utf8(chunk).ok()?.to_string();
            objects.push(SparseBmsObject { index: index as u64, id });
        }
    }
    Some(SparseBmsMessage {
        id: sparse_id,
        line_number,
        measure,
        channel: channel.to_ascii_uppercase(),
        object_count: object_count as u64,
        objects,
    })
}

pub(super) fn extract_bga_message_lines(text: &str) -> Vec<BgaMessage> {
    text.lines()
        .enumerate()
        .filter_map(|(line_index, line)| extract_bga_message_line(line, line_index + 1))
        .collect()
}

pub(super) fn extract_bga_message_line(line: &str, line_number: usize) -> Option<BgaMessage> {
    let trimmed = line.trim();
    let body = trimmed.strip_prefix('#')?;
    let colon = body.find(':')?;
    let head = &body[..colon];
    if head.len() < 5 || !head.is_ascii() {
        return None;
    }
    let measure_text = &head[..head.len() - 2];
    let channel = &head[head.len() - 2..];
    let kind = bga_kind_from_channel(channel)?;
    let payload = body[colon + 1..].trim();
    if payload.len() % 2 != 0 {
        return None;
    }
    let object_count = payload.len() / 2;
    if object_count == 0 {
        return None;
    }
    let measure = measure_text.parse::<u64>().ok()?;
    let mut objects = Vec::new();
    for (index, chunk) in payload.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        if chunk != b"00" {
            let id = std::str::from_utf8(chunk).ok()?.to_string();
            objects.push(SparseBmsObject { index: index as u64, id });
        }
    }
    Some(BgaMessage { line_number, measure, kind, object_count: object_count as u64, objects })
}

pub(super) fn bga_kind_from_channel(channel: &str) -> Option<IntermediateBgaKind> {
    match read_channel(&channel.to_ascii_uppercase())? {
        Channel::BgaBase => Some(IntermediateBgaKind::Base),
        Channel::BgaPoor => Some(IntermediateBgaKind::Poor),
        Channel::BgaLayer => Some(IntermediateBgaKind::Layer),
        Channel::BgaLayer2 => Some(IntermediateBgaKind::Layer2),
        _ => None,
    }
}

pub(super) fn bga_messages_to_intermediate_objects(
    messages: &[BgaMessage],
    base62_obj_ids: bool,
    warnings: &mut Vec<ImportWarning>,
) -> Vec<IntermediateObject> {
    let mut objects = Vec::new();
    for message in messages {
        for object in &message.objects {
            let Some(time) = ObjTime::new(message.measure, object.index, message.object_count)
            else {
                continue;
            };
            let Ok(obj_id) = ObjId::try_from(&object.id, base62_obj_ids) else {
                warnings.push(ImportWarning::ParserDiagnostic {
                    code: "InvalidBgaObjectId".to_string(),
                    message: format!(
                        "line {} BGA channel has invalid object id {:?}",
                        message.line_number, object.id
                    ),
                });
                continue;
            };
            if obj_id.as_u16() == 0 {
                continue;
            }
            objects.push(IntermediateObject {
                measure: track_of(time),
                position_num: time.numerator() as u32,
                position_den: time.denominator().get() as u32,
                kind: IntermediateObjectKind::Bga { bmp_key: obj_id.as_u16(), kind: message.kind },
            });
        }
    }
    objects
}

pub(super) fn inject_sparse_bms_messages<T: KeyLayoutMapper>(
    bms: &mut Bms,
    sparse_messages: &[SparseBmsMessage],
    warnings: &mut Vec<ImportWarning>,
) {
    if sparse_messages.is_empty() {
        return;
    }

    let active_sparse_ids: Vec<usize> =
        bms.repr.raw_command_lines.iter().filter_map(|line| sparse_marker_id(line)).collect();
    for sparse_id in active_sparse_ids {
        if let Some(message) = sparse_messages.get(sparse_id) {
            inject_sparse_bms_message::<T>(bms, message, warnings);
        }
    }
    bms.repr.raw_command_lines.retain(|line| sparse_marker_id(line).is_none());

    for randomized in &mut bms.randomized {
        for branch in randomized.branches_mut() {
            inject_sparse_bms_messages::<T>(branch.sub_mut(), sparse_messages, warnings);
        }
    }
}

pub(super) fn sparse_marker_id(line: &str) -> Option<usize> {
    let line = line.trim();
    let body = line.strip_prefix('#')?;
    let args = body.strip_prefix(SPARSE_BMS_MARKER_HEADER)?;
    args.trim().parse().ok()
}

pub(super) fn inject_sparse_bms_message<T: KeyLayoutMapper>(
    bms: &mut Bms,
    message: &SparseBmsMessage,
    warnings: &mut Vec<ImportWarning>,
) {
    let Some(channel) = read_channel(&message.channel) else {
        warnings.push(ImportWarning::ParserDiagnostic {
            code: "SparseBmsMessageWarning".to_string(),
            message: format!(
                "line {} uses unsupported sparse channel {}",
                message.line_number, message.channel
            ),
        });
        return;
    };

    for object in &message.objects {
        let Some(time) = ObjTime::new(message.measure, object.index, message.object_count) else {
            continue;
        };
        let Ok(obj_id) = ObjId::try_from(&object.id, bms_uses_base62_obj_ids(bms)) else {
            continue;
        };
        if obj_id.as_u16() == 0 {
            continue;
        }

        match channel {
            Channel::Bgm => {
                bms.wav.notes.push_note(WavObj {
                    offset: time,
                    channel_id: NoteChannelId::bgm(),
                    wav_id: obj_id,
                });
            }
            Channel::Note { channel_id } if T::from_channel_id(channel_id).is_some() => {
                bms.wav.notes.push_note(WavObj { offset: time, channel_id, wav_id: obj_id });
            }
            Channel::BpmChangeU8 => {
                bms.bpm.bpm_changes_u8.insert(time, obj_id.as_u16().min(u8::MAX as u16) as u8);
            }
            Channel::BpmChange => {
                if let Some(bpm) =
                    bms.bpm.bpm_defs.get(&obj_id).and_then(|sv| sv.value().as_ref().ok()).cloned()
                {
                    bms.bpm.bpm_changes.insert(time, BpmChangeObj { time, bpm });
                } else {
                    warnings.push(ImportWarning::MissingBpmDefinition { key: obj_id.as_u16() });
                }
            }
            Channel::Stop => {
                if let Some(duration) =
                    bms.stop.stop_defs.get(&obj_id).and_then(|sv| sv.value().as_ref().ok()).cloned()
                {
                    bms.stop.stops.insert(time, StopObj { time, duration });
                } else {
                    warnings.push(ImportWarning::MissingStopDefinition { key: obj_id.as_u16() });
                }
            }
            Channel::Scroll => {
                if let Some(factor) = bms
                    .scroll
                    .scroll_defs
                    .get(&obj_id)
                    .and_then(|sv| sv.value().as_ref().ok())
                    .cloned()
                {
                    bms.scroll
                        .scrolling_factor_changes
                        .insert(time, ScrollingFactorObj { time, factor });
                }
            }
            Channel::Speed => {
                if let Some(factor) = bms
                    .speed
                    .speed_defs
                    .get(&obj_id)
                    .and_then(|sv| sv.value().as_ref().ok())
                    .cloned()
                {
                    bms.speed.speed_factor_changes.insert(time, SpeedObj { time, factor });
                }
            }
            Channel::BgaBase | Channel::BgaPoor | Channel::BgaLayer | Channel::BgaLayer2 => {
                if let Some(layer) = BgaLayer::from_channel(channel) {
                    bms.bmp.bga_changes.insert(time, BgaObj { time, id: obj_id, layer });
                }
            }
            Channel::BgaBaseOpacity
            | Channel::BgaPoorOpacity
            | Channel::BgaLayerOpacity
            | Channel::BgaLayer2Opacity => {
                if let Some(layer) = BgaLayer::from_channel(channel) {
                    bms.bmp.bga_opacity_changes.entry(layer).or_default().insert(
                        time,
                        BgaOpacityObj {
                            time,
                            layer,
                            opacity: obj_id.as_u16().min(u8::MAX as u16) as u8,
                        },
                    );
                }
            }
            Channel::BgaBaseArgb
            | Channel::BgaPoorArgb
            | Channel::BgaLayerArgb
            | Channel::BgaLayer2Argb => {
                if let (Some(layer), Some(argb)) =
                    (BgaLayer::from_channel(channel), bms.bmp.argb_defs.get(&obj_id).copied())
                {
                    bms.bmp
                        .bga_argb_changes
                        .entry(layer)
                        .or_default()
                        .insert(time, BgaArgbObj { time, layer, argb });
                }
            }
            Channel::BgaKeybound => {
                if let Some(event) = bms.bmp.swbga_events.get(&obj_id).cloned() {
                    bms.bmp.bga_keybound_events.insert(time, BgaKeyboundObj { time, event });
                }
            }
            Channel::BgmVolume => {
                bms.volume.bgm_volume_changes.insert(
                    time,
                    BgmVolumeObj { time, volume: obj_id.as_u16().min(u8::MAX as u16) as u8 },
                );
            }
            Channel::KeyVolume => {
                bms.volume.key_volume_changes.insert(
                    time,
                    KeyVolumeObj { time, volume: obj_id.as_u16().min(u8::MAX as u16) as u8 },
                );
            }
            Channel::Text => {
                if let Some(text) = bms.text.texts.get(&obj_id).cloned() {
                    bms.text.text_events.insert(time, TextObj { time, text });
                }
            }
            Channel::Judge => {
                if let Some(judge_level) =
                    bms.judge.exrank_defs.get(&obj_id).map(|def| def.judge_level)
                {
                    bms.judge.judge_events.insert(time, JudgeObj { time, judge_level });
                }
            }
            Channel::SectionLen | Channel::Seek | Channel::OptionChange => {}
            _ => {}
        }
    }
}

pub(super) fn message_channel_bytes(line: &str) -> Option<[u8; 2]> {
    let line = line.trim();
    let body = line.strip_prefix('#')?;
    let (head, _) = split_bms_channel_command(body)?;
    let channel_str = &head[head.len() - 2..];
    let bytes = channel_str.as_bytes();
    Some([bytes[0], bytes[1]])
}

/// PMS Standard: P2 K2–K5 (= PMS K6–K9) の各 note kind。
pub(super) fn pms_standard_upper_channel(channel: [u8; 2]) -> bool {
    let first = channel[0].to_ascii_uppercase();
    matches!(first, b'2' | b'3' | b'5' | b'6' | b'D' | b'E') && matches!(channel[1], b'2'..=b'5')
}

/// PMS BME-type: P1 ch 16–19 (= PMS K6–K9) の各 note kind。
pub(super) fn pms_bme_upper_channel(channel: [u8; 2]) -> bool {
    let first = channel[0].to_ascii_uppercase();
    matches!(first, b'1' | b'3' | b'5' | b'6' | b'D' | b'E') && matches!(channel[1], b'6'..=b'9')
}

/// Standard PMS uses P2 channels 22-25 for keys 6-9. When a chart also
/// contains BME-type P1 channels 16-19, beatoraja ignores those conflicting
/// objects instead of adding them to the standard layout.
pub(super) fn strip_pms_bme_upper_channels(source: &str) -> String {
    source
        .split_inclusive('\n')
        .filter(|line| {
            message_channel_bytes(line).is_none_or(|channel| !pms_bme_upper_channel(channel))
        })
        .collect()
}
