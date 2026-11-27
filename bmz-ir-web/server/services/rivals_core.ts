import type { IrDoubleOption, IrRuleMode, LnScorePolicy } from '../../shared/types/ir'
import { asLnPolicy, normalizeDoubleOption } from './ir/common'

export interface RivalScoreCursor {
  chartSha256: string
  lnPolicy: LnScorePolicy
  doubleOption: IrDoubleOption
}

export function parseRivalScoreCursor(value: unknown): RivalScoreCursor | null {
  if (value === undefined) return null
  if (typeof value !== 'string') throw new Error('invalid rival score cursor')
  const [chartSha256, lnPolicy, doubleOption, extra] = value.split(':')
  if (
    !chartSha256 ||
    !/^[0-9a-f]{64}$/.test(chartSha256) ||
    !lnPolicy ||
    !doubleOption ||
    normalizeDoubleOption(doubleOption) !== doubleOption ||
    extra !== undefined
  ) {
    throw new Error('invalid rival score cursor')
  }
  return { chartSha256, lnPolicy: asLnPolicy(lnPolicy), doubleOption }
}

export function packedRivalSeed(options: Record<string, unknown> | null | undefined) {
  if (options?.seed_scheme !== 'beatoraja_24bit_v1') return undefined
  const value = options.seed ?? options.random_seed
  const seed = typeof value === 'string' && /^\d{1,15}$/.test(value) ? Number(value) : value
  return typeof seed === 'number' && Number.isSafeInteger(seed) && seed >= 0 && seed < 2 ** 48
    ? seed
    : undefined
}

export type RivalComparisonOutcome = 'win' | 'loss' | 'draw'

export interface RivalComparisonScoreRow {
  scoreId: string
  chartSha256: string
  chartMd5: string | null
  chartTitle: string
  chartSubtitle: string | null
  chartArtist: string | null
  chartMode: string
  chartLevel: number | null
  chartDifficulty: string | null
  lnPolicy: LnScorePolicy
  doubleOption: IrDoubleOption
  ruleMode: IrRuleMode
  scoring: string
  exScore: number
  minBp: number
}

export interface RivalComparisonRow {
  chart: {
    sha256: string
    md5: string | null
    title: string
    subtitle: string | null
    artist: string | null
    mode: string
    level: number | null
    difficulty: string | null
  }
  rule: {
    ln_policy: LnScorePolicy
    double_option: IrDoubleOption
    rule_mode: IrRuleMode
    scoring: string
  }
  self: { score_id: string; ex_score: number; min_bp: number }
  rival: { score_id: string; ex_score: number; min_bp: number }
  difference: { ex_score: number; min_bp: number }
  outcome: {
    ex_score: RivalComparisonOutcome
    min_bp: RivalComparisonOutcome
  }
}

export interface RivalMutation {
  targetPlayerId: string
  action: 'add' | 'remove'
}

export function parseRivalMutation(value: unknown, ownerPlayerId: string): RivalMutation {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error('rival payload must be an object')
  }
  const body = value as Record<string, unknown>
  const targetPlayerId =
    typeof body.target_player_id === 'string' ? body.target_player_id.trim() : ''
  if (!targetPlayerId || targetPlayerId === ownerPlayerId) {
    throw new Error('valid target_player_id is required')
  }
  if (body.action !== 'add' && body.action !== 'remove') {
    throw new Error('action must be add or remove')
  }
  return { targetPlayerId, action: body.action }
}

export function buildRivalComparisonRow(
  selfScore: RivalComparisonScoreRow,
  rivalScore: RivalComparisonScoreRow,
): RivalComparisonRow {
  const exScoreDifference = selfScore.exScore - rivalScore.exScore
  const minBpDifference = selfScore.minBp - rivalScore.minBp
  return {
    chart: {
      sha256: selfScore.chartSha256,
      md5: selfScore.chartMd5,
      title: selfScore.chartTitle,
      subtitle: selfScore.chartSubtitle,
      artist: selfScore.chartArtist,
      mode: selfScore.chartMode,
      level: selfScore.chartLevel,
      difficulty: selfScore.chartDifficulty,
    },
    rule: {
      ln_policy: selfScore.lnPolicy,
      double_option: selfScore.doubleOption,
      rule_mode: selfScore.ruleMode,
      scoring: selfScore.scoring,
    },
    self: {
      score_id: selfScore.scoreId,
      ex_score: selfScore.exScore,
      min_bp: selfScore.minBp,
    },
    rival: {
      score_id: rivalScore.scoreId,
      ex_score: rivalScore.exScore,
      min_bp: rivalScore.minBp,
    },
    difference: { ex_score: exScoreDifference, min_bp: minBpDifference },
    outcome: {
      ex_score: higherWins(exScoreDifference),
      min_bp: lowerWins(minBpDifference),
    },
  }
}

function higherWins(difference: number): RivalComparisonOutcome {
  return difference > 0 ? 'win' : difference < 0 ? 'loss' : 'draw'
}

function lowerWins(difference: number): RivalComparisonOutcome {
  return difference < 0 ? 'win' : difference > 0 ? 'loss' : 'draw'
}
