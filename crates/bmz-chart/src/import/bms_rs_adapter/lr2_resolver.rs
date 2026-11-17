//! LR2 互換の `#RANDOM` 解決。
//!
//! LR2 は分岐を入れ子にせず、直近の `#RANDOM` が選んだ値1つと、`#IF` の真偽1つだけを持つ。
//! - `#RANDOM n` は行ごとに `GetRand(n - 1)` を1回消費し、結果 + 1 を選択値にする。
//!   偽の `#IF` の中でも消費する。`#RANDOM` の直後は行を無効にし、後続の `#IF` / `#ENDIF` で再開する。
//! - `#IF v` は `v == 選択値` を真偽にし、`#ENDIF` で真に戻す。
//! - `#SWITCH` 系、`#SETRANDOM`、`#ELSE` / `#ELSEIF` は LR2 では分岐として働かない。
//!   これらの行は状態を変えずに落とし、中の行は `#IF` の真偽だけで残すか決める。
//!   行を残すと bms-rs が自身の乱数で分岐を解決してしまうため、残さない。

use super::*;

pub(super) struct Lr2RandomResolver {
    rng: Lr2Mt19937,
    random_value: i32,
    if_on: bool,
}

impl Lr2RandomResolver {
    pub(super) fn new(seed: u32) -> Self {
        Self { rng: Lr2Mt19937::new(seed), random_value: -1, if_on: true }
    }

    /// Applies line `line_number` and returns whether it stays in the resolved text.
    /// Control lines and lines of a false `#IF` do not; they become empty lines.
    pub(super) fn keep_line(
        &mut self,
        line: &str,
        line_number: usize,
        bms_random_choices: &mut Vec<i32>,
        warnings: &mut Vec<ImportWarning>,
    ) -> bool {
        match beatoraja_random_control_line(line) {
            Some(BeatorajaRandomControl::Random(args)) => {
                self.if_on = false;
                if let Some(max) = parse_lr2_control_int(args, line_number, "#RANDOM", warnings) {
                    if max <= 0 {
                        warnings.push(ImportWarning::ParserDiagnostic {
                            code: "RandomZeroClamped".to_string(),
                            message: format!(
                                "line {line_number} #RANDOM {max} selects 1 for LR2 compatibility"
                            ),
                        });
                    }
                    // `GetRand` of a negative max still consumes once and returns 0.
                    let selected = self.rng.get_rand(i64::from(max) - 1) as i32 + 1;
                    bms_random_choices.push(selected);
                    self.random_value = selected;
                }
            }
            Some(BeatorajaRandomControl::If(args)) => {
                if let Some(condition) = parse_lr2_control_int(args, line_number, "#IF", warnings) {
                    self.if_on = condition == self.random_value;
                }
            }
            Some(BeatorajaRandomControl::EndIf) => self.if_on = true,
            // The single selected value stays until the next `#RANDOM`.
            Some(BeatorajaRandomControl::EndRandom) => {}
            Some(_) => {
                warnings.push(ImportWarning::ParserDiagnostic {
                    code: "Lr2RandomIgnoredControl".to_string(),
                    message: format!(
                        "line {line_number} {} is ignored for LR2 compatibility",
                        line.trim()
                    ),
                });
            }
            None => {
                if let Some(command) = bms_rs_random_typo_control_line(line) {
                    warnings.push(ImportWarning::ParserDiagnostic {
                        code: "Lr2RandomIgnoredControl".to_string(),
                        message: format!(
                            "line {line_number} {command} is ignored for LR2 compatibility"
                        ),
                    });
                } else {
                    return self.if_on;
                }
            }
        }
        false
    }

    /// The sequence after every `#RANDOM`, which the RANDOM lane option continues.
    pub(super) fn finish(self) -> Lr2Mt19937 {
        self.rng
    }
}

fn parse_lr2_control_int(
    args: &str,
    line_number: usize,
    command: &str,
    warnings: &mut Vec<ImportWarning>,
) -> Option<i32> {
    match args.parse::<i32>() {
        Ok(value) => Some(value),
        Err(_) => {
            warnings.push(ImportWarning::ParserDiagnostic {
                code: "Lr2RandomInvalidArgument".to_string(),
                message: format!(
                    "line {line_number} {command} has invalid integer argument {args:?}; ignoring the line"
                ),
            });
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(text: &str, seed: u32) -> (String, Vec<i32>, Vec<String>, Lr2Mt19937) {
        let mut resolver = Lr2RandomResolver::new(seed);
        let mut choices = Vec::new();
        let mut warnings = Vec::new();
        let mut kept = Vec::new();
        for (index, line) in text.lines().enumerate() {
            if resolver.keep_line(line, index + 1, &mut choices, &mut warnings) {
                kept.push(line);
            }
        }
        let codes = warnings
            .iter()
            .map(|warning| match warning {
                ImportWarning::ParserDiagnostic { code, .. } => code.clone(),
                other => format!("{other:?}"),
            })
            .collect();
        (kept.join("\n"), choices, codes, resolver.finish())
    }

    #[test]
    fn random_inside_a_false_if_still_consumes() {
        // seed 391: GetRand(1) -> 0, GetRand(3) -> 0, so both select 1.
        let text = "#RANDOM 2\n#IF 2\n#RANDOM 4\n#00111:01\n#ENDIF\n#00112:01";
        let (kept, choices, warnings, mut rest) = resolve(text, 391);

        assert_eq!(kept, "#00112:01");
        assert_eq!(choices, [1, 1]);
        assert!(warnings.is_empty());
        // The rest continues after both draws: the vector for two earlier draws.
        assert_eq!(rest.shuffle_key_table(7), [0, 1, 2, 3, 4, 6, 5, 7]);
    }

    #[test]
    fn if_compares_with_the_single_latest_value() {
        // seed 2179: GetRand(2) -> 0, so #RANDOM 3 selects 1.
        let text = "#RANDOM 3\n#00114:01\n#IF 1\n#00111:01\n#RANDOM 1\n#00112:01\n#IF 1\n#00115:01\n#IF 2\n#00118:01\n#ENDIF\n#00113:01";
        let (kept, choices, _, mut rest) = resolve(text, 2179);

        assert_eq!(kept, "#00111:01\n#00115:01\n#00113:01");
        assert_eq!(choices, [1, 1]);
        let mut reference = Lr2Mt19937::new(2179);
        reference.get_rand(2);
        reference.get_rand(0);
        assert_eq!(rest.next_u32(), reference.next_u32());
    }

    #[test]
    fn if_before_first_random_compares_with_minus_one() {
        let text = "#IF 0\n#00111:01\n#ENDIF\n#IF -1\n#00112:01\n#ENDIF";
        let (kept, choices, _, _) = resolve(text, 2179);
        assert_eq!(kept, "#00112:01");
        assert!(choices.is_empty());
    }

    #[test]
    fn switch_lines_are_not_branches() {
        let text = "#SWITCH 2\n#CASE 1\n#00111:01\n#SKIP\n#CASE 2\n#00112:01\n#ENDSW";
        let (kept, choices, warnings, mut rest) = resolve(text, 2179);

        assert_eq!(kept, "#00111:01\n#00112:01");
        assert!(choices.is_empty());
        assert_eq!(warnings, ["Lr2RandomIgnoredControl"; 5]);
        // Nothing was drawn.
        assert_eq!(rest.shuffle_key_table(7), [0, 1, 5, 2, 6, 3, 7, 4]);
    }

    #[test]
    fn random_zero_draws_once_and_selects_one() {
        let text = "#RANDOM 0\n#IF 1\n#00111:01\n#ENDIF";
        let (kept, choices, warnings, mut rest) = resolve(text, 2179);

        assert_eq!(kept, "#00111:01");
        assert_eq!(choices, [1]);
        assert_eq!(warnings, ["RandomZeroClamped"]);
        let mut reference = Lr2Mt19937::new(2179);
        reference.next_u32();
        assert_eq!(rest.next_u32(), reference.next_u32());
    }
}
