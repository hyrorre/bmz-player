import type {
  ChartModerationStatus,
  ModerationAction,
  UserModerationStatus,
} from '#shared/constants/moderation'

export interface Pagination {
  limit: number
  offset: number
  total: number
  has_more: boolean
}

export interface ModerationActor {
  id: string
  display_name: string
}

export interface AdminUserListItem {
  id: string
  email: string
  display_name: string
  status: UserModerationStatus
  revision: number
  moderation_updated_at: string | null
  created_at: string
}

export interface AdminUserDetail {
  user: {
    id: string
    email: string
    display_name: string
    bio: string
    created_at: string
    is_admin: boolean
  }
  moderation: {
    status: UserModerationStatus
    reason: string
    revision: number
    updated_at: string | null
    updated_by: ModerationActor | null
  }
  stats: {
    score_count: number
    accepted_score_count: number
    best_score_count: number
    course_score_count: number
    active_session_count: number
    last_score_at: string | null
  }
  latest_scores: Array<{
    score_id: string
    chart_sha256: string
    title: string
    artist: string | null
    clear: string
    ex_score: number
    accepted: boolean
    server_received_at: string
    chart_status: ChartModerationStatus
  }>
}

export interface AdminChartListItem {
  sha256: string
  registered: boolean
  md5: string | null
  title: string
  subtitle: string | null
  artist: string | null
  mode: string | null
  level: number | null
  status: ChartModerationStatus
  reason: string
  revision: number
  moderation_updated_at: string | null
  player_count: number
  score_count: number
}

export interface AdminChartDetail {
  chart: {
    sha256: string
    md5: string | null
    title: string
    subtitle: string | null
    artist: string | null
    genre: string | null
    mode: string
    level: number | null
    difficulty: string | null
    notes: number
    created_at: string
  } | null
  moderation: {
    status: ChartModerationStatus
    reason: string
    note: string
    revision: number
    updated_at: string | null
    updated_by: ModerationActor | null
  }
  stats: { player_count: number; score_count: number }
  players: Array<{
    player_id: string
    display_name: string
    user_status: UserModerationStatus
    score_count: number
    best_ex_score: number
    last_received_at: string
  }>
  same_md5_charts: Array<{ sha256: string; title: string; status: ChartModerationStatus }>
}

export interface ModerationChangeResult<TStatus extends string> {
  result: 'changed' | 'unchanged' | 'conflict'
  status: TStatus
  revision: number
  audit_log_id: string | null
}

export interface AdminAuditLog {
  id: string
  created_at: string
  actor: ModerationActor
  action: ModerationAction
  target_type: 'user' | 'chart'
  target_id: string
  target_label: string
  previous_status: string
  new_status: string
  reason: string
  metadata: Record<string, unknown>
}
