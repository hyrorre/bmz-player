import { IR_SCORE_GRADES } from '../../shared/constants/ir'
import type {
  IrArrangeOptions,
  IrChartLnProfile,
  IrRuleMode,
  LnScorePolicy,
} from '../../shared/types/ir'
import type { ScoreClient } from '../types/scoreDetails'

const lnLabels: Record<LnScorePolicy, string> = {
  ForceLn: 'LN',
  ForceCn: 'CN',
  ForceHcn: 'HCN',
  ForceHln: 'HLN',
  AutoLn: 'AUTO(LN)',
  AutoCn: 'AUTO(CN)',
  AutoHcn: 'AUTO(HCN)',
}
const ruleLabels: Record<IrRuleMode, string> = {
  Beatoraja: 'beatoraja',
  Lr2Oraja: 'LR2oraja',
  Dx: 'DX',
}

interface RankingLnProfile extends IrChartLnProfile {
  has_ln?: boolean
  has_cn?: boolean
  has_hcn?: boolean
  has_hln?: boolean
  ln_notes?: number
  cn_notes?: number
  hcn_notes?: number
  hln_notes?: number
}

function filterItem<T extends string>(value: T, label: string) {
  return { value, label }
}

export const ruleModeItems = [
  filterItem('ALL', 'ALL'),
  ...Object.entries(ruleLabels).map(([value, label]) => filterItem(value as IrRuleMode, label)),
]

export function lnFilterItems(profile: RankingLnProfile | undefined) {
  if (!profile) return []
  const hasDefined =
    profile.has_defined_ln ||
    profile.has_defined_cn ||
    profile.has_defined_hcn ||
    profile.has_defined_hln
  const hasLn =
    hasDefined ||
    profile.has_undefined_ln ||
    profile.has_ln ||
    profile.has_cn ||
    profile.has_hcn ||
    profile.has_hln ||
    profile.ln_notes ||
    profile.cn_notes ||
    profile.hcn_notes ||
    profile.hln_notes
  return availableLnFilterItems(Boolean(hasLn), profile.has_undefined_ln && hasDefined)
}

export function availableLnFilterItems(hasLn: boolean, showAuto: boolean) {
  if (!hasLn) return []
  return [
    filterItem('ALL', 'ALL'),
    ...Object.entries(lnLabels)
      .filter(([value]) => showAuto || value.startsWith('Force'))
      .map(([value, label]) => filterItem(value as LnScorePolicy, label)),
  ]
}

export function lnPolicyLabel(value: string) {
  return lnLabels[value as LnScorePolicy] ?? value
}

export function ruleModeLabel(value: string) {
  return ruleLabels[value as IrRuleMode] ?? value
}

export function formatRankingArrange(
  options: IrArrangeOptions | null | undefined,
  mode: string | null,
) {
  const first = options?.arrange_1p?.toUpperCase()
  const second = options?.arrange_2p?.toUpperCase()
  if ((mode?.toUpperCase() === '10K' || mode?.toUpperCase() === '14K') && (first || second)) {
    return `${first ?? '-'} / ${second ?? '-'}`
  }
  return first ?? '-'
}

export function formatCourseArrange(
  options: (IrArrangeOptions | null)[],
  charts: { mode: string | null }[],
) {
  const labels = options
    .map((entry, index) => formatRankingArrange(entry, charts[index]?.mode ?? null))
    .filter((label) => label !== '-')
  return [...new Set(labels)].join(' · ') || '-'
}

export function totalNotesFromMaxExScore(max: number | null | undefined) {
  return max != null && max > 0 && max % 2 === 0 ? max / 2 : null
}

export function formatScoreClientVersion(client?: Partial<ScoreClient> | null) {
  const version = client?.client_version?.trim()
  if (!version) return ''
  const name = client?.client_name?.trim()
  const displayName = name && /^(bmz|bmz[ -]player)$/i.test(name) ? 'bmz-player' : name
  return [displayName, version].filter(Boolean).join(' ')
}

// Match the ceil-based grade borders and nearest-grade display in BMZ Player.
export function rankingScoreDisplay(score: number, max: number | null | undefined) {
  if (max == null || max <= 0 || score < 0 || max < score) return null
  const borders = IR_SCORE_GRADES.map((_, index) =>
    index === 0 ? 0 : Math.ceil((max * (index + 1)) / 9),
  )
  const current = borders.findLastIndex((border) => border <= score)
  const next = Math.min(current + 1, IR_SCORE_GRADES.length - 1)
  const nearest = score - borders[current]! <= borders[next]! - score ? current : next
  const difference = score - borders[nearest]!
  return {
    max,
    rate: (score / max) * 100,
    grade: IR_SCORE_GRADES[current],
    difference: `${IR_SCORE_GRADES[nearest]}${difference < 0 ? '' : '+'}${difference}`,
  }
}
