/** Fields shared by chart and course history rows. IDs and BP stay page-specific. */
export interface HistoryScore {
  played_at: string | null
  server_received_at: string
  ex_score: number
  clear: string
  max_combo: number
  gauge: string
  ln_policy: string
  rule_mode: string
}
