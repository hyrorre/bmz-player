use anyhow::{Context, Result, bail, ensure};
use bmz_core::lane::KeyMode;
use serde::{Deserialize, Serialize};

use crate::config::profile_config::KeyModeConversionConfig;
use crate::ir::types::IrScoreSubmission;
use crate::ln_policy::LnScorePolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Game {
    #[serde(rename = "bms-7k")]
    Bms7k,
    #[serde(rename = "bms-14k")]
    Bms14k,
    #[serde(rename = "pms-controller")]
    PmsController,
    #[serde(rename = "pms-keyboard")]
    PmsKeyboard,
}

impl Game {
    pub fn for_play(mode: KeyMode, rule: &str, device: &str) -> Result<Self> {
        match (mode, rule, device) {
            (KeyMode::K7, "Lr2Oraja", "keyboard" | "controller") => Ok(Self::Bms7k),
            (KeyMode::K14, "Lr2Oraja", "keyboard" | "controller") => Ok(Self::Bms14k),
            (KeyMode::K9, "Beatoraja", "controller") => Ok(Self::PmsController),
            (KeyMode::K9, "Beatoraja", "keyboard") => Ok(Self::PmsKeyboard),
            _ => bail!(
                "Bokutachi: use native 7K/14K with LR2oraja or native 9K with beatoraja; a saved input device is required"
            ),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bms7k => "bms-7k",
            Self::Bms14k => "bms-14k",
            Self::PmsController => "pms-controller",
            Self::PmsKeyboard => "pms-keyboard",
        }
    }

    pub fn is_pms(self) -> bool {
        matches!(self, Self::PmsController | Self::PmsKeyboard)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScoringScope {
    Native,
    /// Verified at result capture against the display-only 2P lane mask and
    /// scored_note_count_excluding_lanes, never inferred from SessionMode.
    DisplayBattle7k,
}

/// Present only in new Bokutachi jobs. No defaults: legacy or damaged jobs must
/// not invent provenance. Account is additionally bound by the outbox row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayContext {
    pub version: u32,
    pub origin: String,
    pub new_normal_single: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub course_stage: Option<CourseStageContext>,
    pub normal_score_and_effective_assist_passed: bool,
    pub source_key_mode: KeyMode,
    pub primary_key_mode: KeyMode,
    pub conversion: KeyModeConversionConfig,
    pub scoring_scope: ScoringScope,
    pub game: Game,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CourseStageContext {
    pub source_clear: bmz_core::clear::ClearType,
    pub completed: bool,
}

pub(crate) fn context(payload: &IrScoreSubmission) -> Result<PlayContext> {
    let context: PlayContext = serde_json::from_value(
        payload
            .play_options
            .get("bokutachi")
            .context("Bokutachi: missing saved play context")?
            .clone(),
    )
    .context("Bokutachi: invalid saved play context")?;
    ensure!(matches!(context.version, 1 | 2), "Bokutachi: unsupported play context version");
    ensure!(
        context.normal_score_and_effective_assist_passed,
        "Bokutachi: a normally scored play without effective assists is required"
    );
    ensure!(
        context.conversion == KeyModeConversionConfig::Off
            && context.source_key_mode == context.primary_key_mode,
        "Bokutachi: key mode conversions are not supported"
    );
    ensure!(
        context.scoring_scope != ScoringScope::DisplayBattle7k
            || context.primary_key_mode == KeyMode::K7,
        "Bokutachi: invalid display battle scoring scope"
    );
    ensure!(
        payload.rule.ln_policy == LnScorePolicy::ForceLn,
        "Bokutachi: the saved score must use LN only"
    );
    ensure!(
        !payload.chart.features.random,
        "Bokutachi: charts with original RANDOM/SWITCH branches are not supported"
    );
    let stage_flag = payload.play_options.get("course_stage");
    match &context.course_stage {
        Some(stage) => {
            ensure!(
                context.version == 2
                    && !context.new_normal_single
                    && stage_flag == Some(&serde_json::Value::Bool(true)),
                "Bokutachi: inconsistent course stage provenance"
            );
            ensure!(
                stage.source_clear != bmz_core::clear::ClearType::NoPlay
                    && payload.result.notes > 0,
                "Bokutachi: no valid played course stage result"
            );
            ensure!(
                payload.result.clear
                    == crate::storage::play_result::course_stage_clear_type(stage.source_clear)
                        .as_str(),
                "Bokutachi: course stage lamp differs from its saved result"
            );
        }
        None => ensure!(
            context.new_normal_single
                && stage_flag.is_none_or(|v| v == &serde_json::Value::Bool(false)),
            "Bokutachi: missing saved play provenance"
        ),
    }
    ensure!(
        option(payload, "source_kind")? == "local",
        "Bokutachi: past score uploads are not supported"
    );
    ensure!(
        matches!(option(payload, "applied_double_option")?, "off" | "flip"),
        "Bokutachi: BATTLE and BATTLE AS chart options are not supported"
    );
    let game = Game::for_play(
        context.primary_key_mode,
        &payload.rule.rule_mode,
        option(payload, "device_type")?,
    )?;
    ensure!(game == context.game, "Bokutachi: saved game and input device disagree");
    ensure!(
        payload.chart.sha256.len() == 64
            && payload.chart.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
        "Bokutachi: invalid original SHA256"
    );
    ensure!(
        u64::from(payload.result.ex_score) <= u64::from(payload.result.notes) * 2,
        "Bokutachi: EX score exceeds the full scoring note count"
    );
    Ok(context)
}

fn option<'a>(payload: &'a IrScoreSubmission, key: &str) -> Result<&'a str> {
    payload
        .play_options
        .get(key)
        .and_then(|v| v.as_str())
        .with_context(|| format!("Bokutachi: missing or invalid {key}"))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChartMatch {
    pub match_type: String,
    pub identifier: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub difficulty: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ImportRequest {
    meta: Meta,
    scores: [Score; 1],
}
#[derive(Debug, Serialize, Deserialize)]
struct Meta {
    game: Game,
    service: String,
    version: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Score {
    #[serde(flatten)]
    chart: ChartMatch,
    score: u32,
    lamp: String,
    time_achieved: i64,
    judgements: Judgements,
    optional: OptionalScore,
    score_meta: ScoreMeta,
}
#[derive(Debug, Serialize, Deserialize)]
struct Judgements {
    #[serde(skip_serializing_if = "Option::is_none")]
    pgreat: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cool: Option<u32>,
    great: u32,
    good: u32,
    bad: u32,
    poor: u32,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OptionalScore {
    bp: u32,
    max_combo: u32,
}
#[derive(Debug, Serialize, Deserialize)]
struct ScoreMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    random: Option<Random>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum Random {
    Single(String),
    Double([String; 2]),
}

fn random(value: &str) -> Result<Option<&'static str>> {
    Ok(match value {
        "normal" => Some("NONRAN"),
        "mirror" => Some("MIRROR"),
        "random" | "f-random" | "mf-random" => Some("RANDOM"),
        "r-random" => Some("R-RANDOM"),
        "s-random" => Some("S-RANDOM"),
        "spiral" | "h-random" | "all-scr" | "random-ex" | "s-random-ex" => None,
        _ => bail!("Bokutachi: unknown saved arrangement"),
    })
}

fn lamp(value: &str) -> Result<&'static str> {
    Ok(match value {
        "Failed" => "FAILED",
        "AssistEasy" | "LightAssistEasy" => "ASSIST CLEAR",
        "Easy" => "EASY CLEAR",
        "Normal" => "CLEAR",
        "Hard" => "HARD CLEAR",
        "ExHard" => "EX HARD CLEAR",
        "FullCombo" | "Perfect" | "Max" => "FULL COMBO",
        _ => bail!("Bokutachi: no playable clear lamp"),
    })
}

pub(crate) fn convert(payload: &IrScoreSubmission, chart: ChartMatch) -> Result<ImportRequest> {
    let ctx = context(payload)?;
    ensure!(
        if ctx.game.is_pms() {
            chart.match_type == "tachiSongID" && chart.difficulty.as_deref() == Some("CHART")
        } else {
            chart.match_type == "bmsChartHash"
                && chart.identifier == payload.chart.sha256
                && chart.difficulty.is_none()
        },
        "Bokutachi: invalid chart match for game"
    );
    let left = random(option(payload, "arrange_1p")?)?;
    let arrangement = if ctx.game == Game::Bms14k {
        left.zip(random(option(payload, "arrange_2p")?)?)
            .map(|(l, r)| Random::Double([l.into(), r.into()]))
    } else {
        left.map(|v| Random::Single(v.into()))
    };
    let r = &payload.result;
    let f = r.judges.fast;
    let s = r.judges.slow;
    let add = |a: u32, b: u32| a.checked_add(b).context("Bokutachi: judgement count overflow");
    let top = add(f.pgreat, s.pgreat)?;
    let time_achieved = r
        .played_at
        .checked_mul(1000)
        .filter(|v| *v >= 0)
        .context("Bokutachi: invalid saved play time")?;
    Ok(ImportRequest {
        meta: Meta {
            game: ctx.game,
            service: "BMZ Player".into(),
            version: payload.client.version.clone(),
        },
        scores: [Score {
            chart,
            score: r.ex_score,
            lamp: if ctx.course_stage.is_some() && r.clear == "NoPlay" {
                "NO PLAY"
            } else {
                lamp(&r.clear)?
            }
            .into(),
            time_achieved,
            judgements: Judgements {
                pgreat: (!ctx.game.is_pms()).then_some(top),
                cool: ctx.game.is_pms().then_some(top),
                great: add(f.great, s.great)?,
                good: add(f.good, s.good)?,
                bad: add(f.bad, s.bad)?,
                poor: add(add(f.poor, s.poor)?, add(f.empty_poor, s.empty_poor)?)?,
            },
            optional: OptionalScore { bp: r.min_bp, max_combo: r.max_combo },
            score_meta: ScoreMeta { random: arrangement },
        }],
    })
}
