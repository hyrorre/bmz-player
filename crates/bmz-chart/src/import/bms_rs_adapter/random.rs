use super::*;

#[derive(Debug, Clone, Copy)]
pub(super) enum BeatorajaRandomControl<'a> {
    Random(&'a str),
    SetRandom(&'a str),
    If(&'a str),
    Else,
    ElseIf,
    EndIf,
    EndRandom,
    Switch(&'a str),
    SetSwitch(&'a str),
    Case(&'a str),
    Skip,
    Def,
    EndSwitch,
}

#[derive(Debug, Clone, Copy)]
struct SwitchControlState {
    selected: u64,
    matched: bool,
    branch_active: bool,
    skipped: bool,
}

/// beatoraja 互換の `#RANDOM` / `#SWITCH` 解決。区切り文字を正規化したテキストを
/// 1行ずつ [`Self::keep_line`] に渡し、分岐を解決したテキストに残る行を決める。
pub(super) struct BeatorajaRandomResolver<'a> {
    random_source: &'a BmsRandomSource,
    rng: JavaRandom,
    random_choice_index: usize,
    switch_choice_index: usize,
    random_stack: Vec<i32>,
    skip_stack: Vec<bool>,
    switch_stack: Vec<SwitchControlState>,
}

impl<'a> BeatorajaRandomResolver<'a> {
    pub(super) fn new(random_source: &'a BmsRandomSource) -> Self {
        Self {
            random_source,
            rng: JavaRandom::new(random_source_seed(random_source) as i64),
            random_choice_index: 0,
            switch_choice_index: 0,
            random_stack: Vec::new(),
            skip_stack: Vec::new(),
            switch_stack: Vec::new(),
        }
    }

    /// Applies line `line_number` and returns whether it stays in the resolved text.
    /// Control lines and lines of unselected branches do not; they become empty lines.
    pub(super) fn keep_line(
        &mut self,
        line: &str,
        line_number: usize,
        bms_random_choices: &mut Vec<i32>,
        bms_switch_choices: &mut Vec<u64>,
        warnings: &mut Vec<ImportWarning>,
    ) -> bool {
        let Self {
            random_source,
            rng,
            random_choice_index,
            switch_choice_index,
            random_stack,
            skip_stack,
            switch_stack,
        } = self;

        match beatoraja_random_control_line(line) {
            Some(BeatorajaRandomControl::Random(args)) => {
                if let Some(max) =
                    parse_beatoraja_control_int(args, line_number, "#RANDOM", warnings)
                {
                    let max = if max <= 0 {
                        warnings.push(ImportWarning::ParserDiagnostic {
                            code: "RandomZeroClamped".to_string(),
                            message: format!(
                                "line {line_number} #RANDOM {max} is treated as #RANDOM 1 for beatoraja compatibility"
                            ),
                        });
                        1
                    } else {
                        max
                    };
                    let selected = match random_source {
                        BmsRandomSource::Seed(_) => rng.next_int_bound(max) + 1,
                        BmsRandomSource::Choices { random, .. } => {
                            let current_index = *random_choice_index;
                            *random_choice_index += 1;
                            match random.get(current_index).copied() {
                                None => {
                                    warnings.push(ImportWarning::ParserDiagnostic {
                                        code: "BmsRandomChoiceMissing".to_string(),
                                        message: format!(
                                            "line {line_number} #RANDOM {max} has no recorded choice at index {current_index}; using deterministic fallback"
                                        ),
                                    });
                                    rng.next_int_bound(max) + 1
                                }
                                Some(choice) if !(1..=max).contains(&choice) => {
                                    let clamped = choice.clamp(1, max);
                                    warnings.push(ImportWarning::ParserDiagnostic {
                                        code: "BmsRandomChoiceOutOfRange".to_string(),
                                        message: format!(
                                            "line {line_number} #RANDOM {max} cannot use recorded choice {choice} at index {current_index}; clamping to {clamped}"
                                        ),
                                    });
                                    clamped
                                }
                                Some(choice) => choice,
                            }
                        }
                    };
                    bms_random_choices.push(selected);
                    random_stack.push(selected);
                }
            }
            Some(BeatorajaRandomControl::SetRandom(args)) => {
                if let Some(selected) =
                    parse_beatoraja_control_int(args, line_number, "#SETRANDOM", warnings)
                {
                    random_stack.push(selected);
                }
            }
            Some(BeatorajaRandomControl::If(args)) => {
                if let Some(&selected) = random_stack.last() {
                    if let Some(condition) =
                        parse_beatoraja_control_int(args, line_number, "#IF", warnings)
                    {
                        skip_stack.push(selected != condition);
                    }
                } else {
                    warnings.push(ImportWarning::ParserDiagnostic {
                        code: "BeatorajaRandomIfWithoutRandom".to_string(),
                        message: format!(
                            "line {line_number} #IF has no active #RANDOM; continuing like beatoraja"
                        ),
                    });
                }
            }
            Some(BeatorajaRandomControl::Else | BeatorajaRandomControl::ElseIf) => {
                // beatoraja (jbms-parser BMSDecoder) は予約語として RANDOM / IF /
                // ENDIF / ENDRANDOM しか扱わず、#ELSE / #ELSEIF 行そのものを
                // 無視する。つまり直前の #IF の skip 状態がそのまま継続する
                // (#IF 一致時は #ELSE 側のブロックも取り込まれる)。BMZ も同じ
                // 実行結果になるよう、行を落とすだけで skip 状態は変更しない。
                warnings.push(ImportWarning::ParserDiagnostic {
                    code: "BeatorajaRandomUnsupportedElse".to_string(),
                    message: format!(
                        "line {line_number} random #ELSE/#ELSEIF is ignored for beatoraja compatibility"
                    ),
                });
            }
            Some(BeatorajaRandomControl::EndIf) => {
                if skip_stack.pop().is_none() {
                    warnings.push(ImportWarning::ParserDiagnostic {
                        code: "BeatorajaRandomEndifWithoutIf".to_string(),
                        message: format!(
                            "line {line_number} #ENDIF has no active #IF; continuing like beatoraja"
                        ),
                    });
                }
            }
            Some(BeatorajaRandomControl::EndRandom) => {
                if random_stack.pop().is_none() {
                    warnings.push(ImportWarning::ParserDiagnostic {
                        code: "BeatorajaRandomEndrandomWithoutRandom".to_string(),
                        message: format!(
                            "line {line_number} #ENDRANDOM has no active #RANDOM; continuing like beatoraja"
                        ),
                    });
                }
            }
            Some(BeatorajaRandomControl::Switch(args)) => {
                let mut max = parse_beatoraja_control_u64(args, line_number, "#SWITCH", warnings)
                    .unwrap_or(1);
                if max == 0 {
                    warnings.push(ImportWarning::ParserDiagnostic {
                        code: "SwitchZeroClamped".to_string(),
                        message: format!("line {line_number} #SWITCH 0 is treated as #SWITCH 1"),
                    });
                    max = 1;
                }
                let selected = match random_source {
                    BmsRandomSource::Seed(_) => select_switch_value(rng, max),
                    BmsRandomSource::Choices { switches, .. } => {
                        let current_index = *switch_choice_index;
                        *switch_choice_index += 1;
                        match switches.get(current_index).copied() {
                            None => {
                                warnings.push(ImportWarning::ParserDiagnostic {
                                    code: "BmsSwitchChoiceMissing".to_string(),
                                    message: format!(
                                        "line {line_number} #SWITCH {max} has no recorded choice at index {current_index}; using deterministic fallback"
                                    ),
                                });
                                select_switch_value(rng, max)
                            }
                            Some(choice) if !(1..=max).contains(&choice) => {
                                let clamped = choice.clamp(1, max);
                                warnings.push(ImportWarning::ParserDiagnostic {
                                    code: "BmsSwitchChoiceOutOfRange".to_string(),
                                    message: format!(
                                        "line {line_number} #SWITCH {max} cannot use recorded choice {choice} at index {current_index}; clamping to {clamped}"
                                    ),
                                });
                                clamped
                            }
                            Some(choice) => choice,
                        }
                    }
                };
                bms_switch_choices.push(selected);
                switch_stack.push(SwitchControlState {
                    selected,
                    matched: false,
                    branch_active: false,
                    skipped: false,
                });
            }
            Some(BeatorajaRandomControl::SetSwitch(args)) => {
                let selected =
                    parse_beatoraja_control_u64(args, line_number, "#SETSWITCH", warnings)
                        .unwrap_or(0);
                switch_stack.push(SwitchControlState {
                    selected,
                    matched: false,
                    branch_active: false,
                    skipped: false,
                });
            }
            Some(BeatorajaRandomControl::Case(args)) => {
                let condition = parse_beatoraja_control_u64(args, line_number, "#CASE", warnings);
                if let Some(state) = switch_stack.last_mut() {
                    if state.skipped {
                        state.branch_active = false;
                    } else if state.branch_active {
                        // `#SKIP` がない CASE は次の CASE へ fallthrough する。
                    } else if !state.matched {
                        state.branch_active = condition == Some(state.selected);
                        state.matched = state.branch_active;
                    }
                } else {
                    warnings.push(ImportWarning::ParserDiagnostic {
                        code: "BmsSwitchCaseWithoutSwitch".to_string(),
                        message: format!(
                            "line {line_number} #CASE has no active #SWITCH; continuing safely"
                        ),
                    });
                }
            }
            Some(BeatorajaRandomControl::Skip) => {
                if let Some(state) = switch_stack.last_mut() {
                    if state.branch_active {
                        state.skipped = true;
                        state.branch_active = false;
                    }
                } else {
                    warnings.push(ImportWarning::ParserDiagnostic {
                        code: "BmsSwitchSkipWithoutSwitch".to_string(),
                        message: format!(
                            "line {line_number} #SKIP has no active #SWITCH; continuing safely"
                        ),
                    });
                }
            }
            Some(BeatorajaRandomControl::Def) => {
                if let Some(state) = switch_stack.last_mut() {
                    if state.skipped {
                        state.branch_active = false;
                    } else if !state.branch_active && !state.matched {
                        state.branch_active = true;
                        state.matched = true;
                    }
                } else {
                    warnings.push(ImportWarning::ParserDiagnostic {
                        code: "BmsSwitchDefWithoutSwitch".to_string(),
                        message: format!(
                            "line {line_number} #DEF has no active #SWITCH; continuing safely"
                        ),
                    });
                }
            }
            Some(BeatorajaRandomControl::EndSwitch) => {
                if switch_stack.pop().is_none() {
                    warnings.push(ImportWarning::ParserDiagnostic {
                        code: "BmsSwitchEndWithoutSwitch".to_string(),
                        message: format!(
                            "line {line_number} #ENDSW has no active #SWITCH; continuing safely"
                        ),
                    });
                }
            }
            None => {
                if let Some(command) = bms_rs_random_typo_control_line(line) {
                    warnings.push(ImportWarning::ParserDiagnostic {
                        code: "BeatorajaRandomIgnoredTypoControl".to_string(),
                        message: format!(
                            "line {line_number} {command} is ignored for beatoraja compatibility"
                        ),
                    });
                } else {
                    return !skip_stack.last().copied().unwrap_or(false)
                        && switch_stack.iter().all(|state| state.branch_active);
                }
            }
        }
        false
    }

    /// Reports recorded choices that no `#RANDOM` / `#SWITCH` used, after the last line.
    pub(super) fn finish(self, warnings: &mut Vec<ImportWarning>) {
        let Self { random_source, random_choice_index, switch_choice_index, .. } = self;
        if let BmsRandomSource::Choices { random, switches } = random_source {
            if random_choice_index < random.len() {
                warnings.push(ImportWarning::ParserDiagnostic {
                    code: "BmsRandomChoiceExtra".to_string(),
                    message: format!(
                        "{} recorded BMS #RANDOM choice(s) were unused",
                        random.len() - random_choice_index
                    ),
                });
            }
            if switch_choice_index < switches.len() {
                warnings.push(ImportWarning::ParserDiagnostic {
                    code: "BmsSwitchChoiceExtra".to_string(),
                    message: format!(
                        "{} recorded BMS #SWITCH choice(s) were unused",
                        switches.len() - switch_choice_index
                    ),
                });
            }
        }
    }
}

pub(super) fn random_source_seed(random_source: &BmsRandomSource) -> u64 {
    match random_source {
        BmsRandomSource::Seed(seed) => seed.unwrap_or(0),
        BmsRandomSource::Choices { .. } => 0,
    }
}

fn select_switch_value(rng: &mut JavaRandom, max: u64) -> u64 {
    if max <= i32::MAX as u64 {
        return (rng.next_int_bound(max as i32) + 1) as u64;
    }
    bms_rs::bms::rng::Rng::generate(rng, 1u64.into()..=max.into()).try_into().unwrap_or(max)
}

/// Whether `line` starts a `#RANDOM` / `#SWITCH` block, as `has_bms_random` records it.
pub(super) fn declares_bms_random(line: &str) -> bool {
    matches!(
        beatoraja_random_control_line(line),
        Some(
            BeatorajaRandomControl::Random(_)
                | BeatorajaRandomControl::SetRandom(_)
                | BeatorajaRandomControl::Switch(_)
                | BeatorajaRandomControl::SetSwitch(_)
        )
    )
}

pub(super) fn beatoraja_random_control_line(line: &str) -> Option<BeatorajaRandomControl<'_>> {
    let body = line.trim_start().strip_prefix('#')?.trim_start();
    // Every control command starts with one of these letters; most lines are channel
    // messages or definitions and stop here.
    if !matches!(
        body.as_bytes().first()?.to_ascii_uppercase(),
        b'C' | b'D' | b'E' | b'I' | b'R' | b'S'
    ) {
        return None;
    }
    if is_control_command(body, "ENDRANDOM") {
        return Some(BeatorajaRandomControl::EndRandom);
    }
    if is_control_command(body, "ENDIF") {
        return Some(BeatorajaRandomControl::EndIf);
    }
    if is_control_command(body, "ELSEIF") {
        return Some(BeatorajaRandomControl::ElseIf);
    }
    if is_control_command(body, "ELSE") {
        return Some(BeatorajaRandomControl::Else);
    }
    if let Some(args) = control_command_args(body, "SETRANDOM") {
        return Some(BeatorajaRandomControl::SetRandom(args));
    }
    if let Some(args) = control_command_args(body, "RANDOM") {
        return Some(BeatorajaRandomControl::Random(args));
    }
    if let Some(args) = control_command_args(body, "IF") {
        return Some(BeatorajaRandomControl::If(args));
    }
    if is_control_command(body, "ENDSWITCH") || is_control_command(body, "ENDSW") {
        return Some(BeatorajaRandomControl::EndSwitch);
    }
    if let Some(args) = control_command_args(body, "SETSWITCH") {
        return Some(BeatorajaRandomControl::SetSwitch(args));
    }
    if let Some(args) = control_command_args(body, "SWITCH") {
        return Some(BeatorajaRandomControl::Switch(args));
    }
    if let Some(args) = control_command_args(body, "CASE") {
        return Some(BeatorajaRandomControl::Case(args));
    }
    if is_control_command(body, "SKIP") {
        return Some(BeatorajaRandomControl::Skip);
    }
    if is_control_command(body, "DEF") {
        return Some(BeatorajaRandomControl::Def);
    }
    None
}

pub(super) fn bms_rs_random_typo_control_line(line: &str) -> Option<&'static str> {
    let body = line.trim_start().strip_prefix('#')?.trim();
    if body.eq_ignore_ascii_case("END IF") {
        return Some("#END IF");
    }
    if body.eq_ignore_ascii_case("END RANDOM") {
        return Some("#END RANDOM");
    }
    None
}

fn is_control_command(body: &str, command: &str) -> bool {
    let Some(rest) = strip_control_command_prefix(body, command) else {
        return false;
    };
    rest.is_empty() || rest.chars().next().is_some_and(char::is_whitespace)
}

fn control_command_args<'a>(body: &'a str, command: &str) -> Option<&'a str> {
    let rest = strip_control_command_prefix(body, command)?;
    if rest.is_empty() {
        return Some(rest);
    }
    if rest.chars().next().is_some_and(char::is_whitespace) {
        return Some(rest.trim());
    }
    direct_control_number(rest.trim())
}

fn strip_control_command_prefix<'a>(body: &'a str, command: &str) -> Option<&'a str> {
    body.get(..command.len())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(command))
        .then(|| &body[command.len()..])
}

fn direct_control_number(value: &str) -> Option<&str> {
    let value = value.strip_prefix('[').and_then(|value| value.strip_suffix(']')).unwrap_or(value);
    let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
    (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())).then_some(value)
}

const EMPTY_METADATA_COMMANDS: [&str; 19] = [
    "PLAYER",
    "GENRE",
    "TITLE",
    "SUBTITLE",
    "ARTIST",
    "SUBARTIST",
    "BPM",
    "PLAYLEVEL",
    "DIFFICULTY",
    "RANK",
    "DEFEXRANK",
    "TOTAL",
    "STAGEFILE",
    "BANNER",
    "BACKBMP",
    "PREVIEW",
    "VOLWAV",
    "LNTYPE",
    "LNMODE",
];

pub(super) fn is_empty_metadata_command(line: &str) -> bool {
    let Some(body) = line.trim().strip_prefix('#') else {
        return false;
    };
    // `trim` leaves no whitespace at the end, so the value is empty exactly when the
    // body is the bare command name.
    let body = body.trim_start();
    EMPTY_METADATA_COMMANDS.iter().any(|command| body.eq_ignore_ascii_case(command))
}

/// A `#LNOBJ` line of the random-resolved text. Its object ID is read once `#BASE` is known.
#[derive(Debug, Clone)]
pub(super) struct LnobjLine {
    pub(super) line_number: usize,
    pub(super) args: String,
}

pub(super) fn lnobj_wav_key(
    lines: &[LnobjLine],
    base62_obj_ids: bool,
    warnings: &mut Vec<ImportWarning>,
) -> Option<u16> {
    let mut lnobj_wav_key = None;
    for LnobjLine { line_number, args } in lines {
        let Some(token) = args.split_whitespace().next() else {
            warnings.push(ImportWarning::ParserDiagnostic {
                code: "InvalidLnobj".to_string(),
                message: format!("line {line_number} #LNOBJ has no object id"),
            });
            continue;
        };
        match ObjId::try_from(token, base62_obj_ids) {
            Ok(obj_id) => lnobj_wav_key = Some(obj_id.as_u16()),
            Err(err) => warnings.push(ImportWarning::ParserDiagnostic {
                code: "InvalidLnobj".to_string(),
                message: format!(
                    "line {line_number} #LNOBJ has invalid object id {token:?}: {err}"
                ),
            }),
        }
    }
    lnobj_wav_key
}

/// The arguments of `#LNOBJ`, which must be followed by whitespace or the line end.
pub(super) fn lnobj_command_args(line: &str) -> Option<&str> {
    let body = line.trim_start().strip_prefix('#')?.trim_start();
    let rest = strip_control_command_prefix(body, "LNOBJ")?;
    (rest.is_empty() || rest.starts_with(char::is_whitespace)).then(|| rest.trim())
}

pub(super) fn parse_beatoraja_control_int(
    args: &str,
    line_number: usize,
    command: &str,
    warnings: &mut Vec<ImportWarning>,
) -> Option<i32> {
    match args.parse::<i32>() {
        Ok(value) => Some(value),
        Err(_) => {
            warnings.push(ImportWarning::ParserDiagnostic {
                code: "BeatorajaRandomInvalidArgument".to_string(),
                message: format!(
                    "line {line_number} {command} has invalid integer argument {args:?}; continuing like beatoraja"
                ),
            });
            None
        }
    }
}

pub(super) fn parse_beatoraja_control_u64(
    args: &str,
    line_number: usize,
    command: &str,
    warnings: &mut Vec<ImportWarning>,
) -> Option<u64> {
    match args.parse::<u64>() {
        Ok(value) => Some(value),
        Err(_) => {
            warnings.push(ImportWarning::ParserDiagnostic {
                code: "BmsSwitchInvalidArgument".to_string(),
                message: format!(
                    "line {line_number} {command} has invalid u64 argument {args:?}; continuing safely"
                ),
            });
            None
        }
    }
}
