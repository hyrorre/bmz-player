use super::*;

pub(super) fn build_metadata(bms: &Bms) -> IntermediateMetadata {
    let initial_bpm = bms
        .bpm
        .bpm
        .as_ref()
        .and_then(|sv| sv.value().as_ref().ok().map(|v| v.get()))
        .unwrap_or(130.0);
    let total = bms.judge.total.as_ref().and_then(|sv| sv.value().as_ref().ok().map(|v| v.get()));
    let judge_rank = bms.judge.rank.map(judge_level_to_int);
    let judge_rank_spec =
        judge_rank.map(|value| JudgeRankSpec { value, kind: JudgeRankKind::BmsRank });

    IntermediateMetadata {
        conditional: None,
        title: bms.music_info.title.clone().unwrap_or_default(),
        subtitle: bms.music_info.subtitle.clone().unwrap_or_default(),
        artist: bms.music_info.artist.clone().unwrap_or_default(),
        subartist: bms.music_info.sub_artist.clone().unwrap_or_default(),
        genre: bms.music_info.genre.clone().unwrap_or_default(),
        play_level: bms.metadata.play_level.map(|v| v.to_string()).unwrap_or_default(),
        difficulty_name: bms.metadata.difficulty.map(|v| v.to_string()).unwrap_or_default(),
        judge_rank,
        judge_rank_spec,
        initial_bpm,
        total,
        total_is_bmson_percent: false,
        stage_file: bms
            .sprite
            .stage_file
            .as_ref()
            .map(|p| path_to_string(p.as_path()))
            .unwrap_or_default(),
        banner_file: bms
            .sprite
            .banner
            .as_ref()
            .map(|p| path_to_string(p.as_path()))
            .unwrap_or_default(),
        backbmp_file: bms
            .sprite
            .back_bmp
            .as_ref()
            .map(|p| path_to_string(p.as_path()))
            .unwrap_or_default(),
        preview_file: bms
            .music_info
            .preview_music
            .as_ref()
            .map(|p| path_to_string(p.as_path()))
            .unwrap_or_default(),
        volwav_percent: bms.volume.volume.relative_percent,
        long_note_mode: map_ln_mode(bms.repr.ln_mode),
        long_note_mode_defined: bms_has_explicit_ln_mode(bms),
        has_bga: false,
        has_bms_random: false,
        source_url: String::new(),
        append_url: String::new(),
        bms_headers: BTreeMap::new(),
        key_mode: KeyMode::default(),
        base62_obj_ids: bms_uses_base62_obj_ids(bms),
        suppress_bar_lines: false,
    }
}

pub(super) fn extract_bms_headers_from_text(text: &str) -> BTreeMap<String, String> {
    let mut headers = BTreeMap::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('#') {
            continue;
        }
        let body = trimmed.strip_prefix('#').unwrap_or(trimmed).trim_start();
        if body.is_empty() || is_bms_channel_command(body) {
            continue;
        }
        let (name, value) = split_bms_header_command(body);
        if name.is_empty() {
            continue;
        }
        let name = name.to_ascii_uppercase();
        if !value.is_empty() || !headers.contains_key(&name) {
            headers.insert(name, value);
        }
    }
    headers
}

pub(super) fn apply_raw_judge_rank_headers(
    intermediate: &mut IntermediateChart,
    bms_headers: &BTreeMap<String, String>,
) {
    let Some(value) = bms_headers.get("DEFEXRANK").and_then(|value| parse_header_i32(value)) else {
        return;
    };
    intermediate.metadata.judge_rank = Some(value);
    intermediate.metadata.judge_rank_spec =
        Some(JudgeRankSpec { value, kind: JudgeRankKind::DefExRank });
}

pub(super) fn parse_header_i32(value: &str) -> Option<i32> {
    value.split_whitespace().next()?.parse().ok()
}

pub(super) fn split_bms_header_command(body: &str) -> (String, String) {
    if let Some((name, value)) = body.split_once(char::is_whitespace) {
        (name.to_string(), value.trim().to_string())
    } else {
        (body.to_string(), String::new())
    }
}

pub(super) fn is_bms_channel_command(body: &str) -> bool {
    let Some((head, tail)) = body.split_once(':') else {
        return false;
    };
    let bytes = head.as_bytes();
    bytes.len() == 5
        && bytes[..3].iter().all(u8::is_ascii_digit)
        && bytes[3..].iter().all(u8::is_ascii_alphanumeric)
        && !tail.is_empty()
}

pub(super) fn append_url_from_headers(headers: &BTreeMap<String, String>) -> String {
    for key in ["URL-WAV", "URLWAV", "URL_WAV"] {
        if let Some(url) = headers.get(key)
            && !url.is_empty()
        {
            return url.clone();
        }
    }
    String::new()
}

pub(super) fn source_text_has_bms_random(text: &str) -> bool {
    text.lines().any(|line| {
        matches!(
            beatoraja_random_control_line(line),
            Some(
                BeatorajaRandomControl::Random(_)
                    | BeatorajaRandomControl::SetRandom(_)
                    | BeatorajaRandomControl::Switch(_)
                    | BeatorajaRandomControl::SetSwitch(_)
            )
        )
    })
}

pub(super) fn bms_uses_base62_obj_ids(bms: &Bms) -> bool {
    if bms.repr.case_sensitive_obj_id {
        return true;
    }
    // bms-rs は `#BASE 62` 処理時に RefCell だけ更新し `repr.case_sensitive_obj_id` を
    // 立てないことがあるため、記録済みヘッダ行も見る。
    bms.repr.raw_command_lines.iter().any(|line| line.eq_ignore_ascii_case("#BASE 62"))
}

pub(super) fn bms_has_explicit_ln_mode(bms: &Bms) -> bool {
    bms.repr.raw_command_lines.iter().any(|line| {
        let trimmed = line.trim_start();
        trimmed.get(..7).is_some_and(|prefix| prefix.eq_ignore_ascii_case("#LNMODE"))
    })
}

pub(super) fn map_ln_mode(mode: LnMode) -> LongNoteMode {
    match mode {
        LnMode::Ln => LongNoteMode::Ln,
        LnMode::Cn => LongNoteMode::Cn,
        LnMode::Hcn => LongNoteMode::Hcn,
    }
}

pub(super) fn path_to_string(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

pub(super) fn judge_level_to_int(level: JudgeLevel) -> i32 {
    match level {
        JudgeLevel::VeryHard => 0,
        JudgeLevel::Hard => 1,
        JudgeLevel::Normal => 2,
        JudgeLevel::Easy => 3,
        JudgeLevel::OtherInt(v) => v.clamp(i32::MIN as i64, i32::MAX as i64) as i32,
    }
}
