export type LnScorePolicy = 'AutoLn' | 'AutoCn' | 'AutoHcn' | 'ForceLn' | 'ForceCn' | 'ForceHcn'

export type EffectiveLnMode = 'ln' | 'cn' | 'hcn'

export type IrRankingScope = 'global' | 'self_and_rivals' | 'rivals' | 'self' | 'around_self'

export type IrVerificationStatus = 'unverified' | 'signed' | 'invalid' | 'trusted'

export type IrDeviceType = 'keyboard' | 'controller'

export type IrDoubleOption = 'off' | 'battle' | 'battle_auto_scratch'

export interface IrChartLnProfile {
  has_undefined_ln: boolean
  has_defined_ln: boolean
  has_defined_cn: boolean
  has_defined_hcn: boolean
}

export interface IrScoreSubmission {
  client: {
    name: string
    version: string
    platform: string
  }
  chart: {
    sha256: string
    md5?: string | null
    ln_profile?: Partial<IrChartLnProfile>
    title?: string
    subtitle?: string | null
    genre?: string | null
    artist?: string | null
    subartists?: string[]
    mode?: string
    level?: number | null
    total?: number | null
    judge?: number | null
    bpm?: {
      min?: number | null
      max?: number | null
    }
    notes?: {
      total?: number
      ln?: number
      cn?: number
      hcn?: number
      mine?: number
    }
    features?: Record<string, boolean>
    urls?: {
      source?: string | null
      append?: string | null
    }
    headers?: Record<string, string>
  }
  rule: {
    play_mode: string
    key_mode: string
    gauge: string
    ln_policy: LnScorePolicy
    effective_ln_mode: EffectiveLnMode
    judge_algorithm: string
    scoring: 'bms_ex_score_v1'
  }
  result: {
    clear: string
    /** ISO 文字列、または BMZ client からの unix 秒。 */
    played_at?: string | number | null
    duration_ms?: number | null
    judges: {
      fast: IrJudgeCounts
      slow: IrJudgeCounts
    }
    ex_score: number
    avg_judge_ms?: number | null
    max_combo: number
    notes: number
    pass_notes?: number
    min_bp: number
    min_cb: number
  }
  play_options: {
    device_type: IrDeviceType
    double_option?: IrDoubleOption
  } & Record<string, unknown>
  replay?: {
    hash?: string | null
    format?: string | null
    upload_intent?: string | null
  }
  evidence?: Record<string, unknown>
  idempotency_key: string
}

export interface IrJudgeCounts {
  pgreat: number
  great: number
  good: number
  bad: number
  poor: number
  empty_poor: number
}

export interface IrPreviousBest {
  clear_type: string
  ex_score: number
  max_combo: number
  min_bp: number
  min_cb: number
}

export interface IrSubmitResponse {
  accepted: boolean
  score_id: string
  best_updated: boolean
  updated_fields: {
    ex_score: boolean
    clear: boolean
    max_combo: boolean
    min_bp: boolean
    min_cb: boolean
  }
  server_received_at: string
  previous_best?: IrPreviousBest | null
  rankings?: Partial<Record<IrRankingScope, IrScopedRankingResponse>>
}

export interface IrScopedRankingResponse {
  succeeded: boolean
  data?: IrRanking
  error?: string
}

export interface IrRanking {
  chart: {
    sha256: string
  }
  rule: {
    scoring: string
    gauge?: string
    ln_policy?: LnScorePolicy
    effective_ln_mode?: EffectiveLnMode
    double_option?: IrDoubleOption
  }
  ranking: {
    scope: IrRankingScope
    sort: 'ex_score_desc'
    /** 全プレイヤー中のクリア率 (%)。スコアが無い場合は null。 */
    clear_rate: number | null
    entries: IrRankingEntry[]
    self?: {
      rank: number
      score_id: string
      included_in_entries: boolean
    }
    pagination: {
      limit: number
      offset: number
      /** scope 内の総エントリ数。 */
      total: number
      has_more: boolean
    }
  }
}

export interface IrRankingEntry {
  rank: number
  scope_rank: number
  player: {
    id: string
    display_name: string
  }
  score: {
    score_id: string
    clear: string
    ex_score: number
    max_combo: number
    min_bp: number
    min_cb: number
    gauge: string
    ln_policy: LnScorePolicy
    double_option: IrDoubleOption
    device_type: IrDeviceType
    played_at: string | null
    verification: IrVerificationStatus
  }
  stats?: {
    play_count: number
    clear_count: number
  }
  relation: {
    is_self: boolean
    is_rival: boolean
  }
}
