/**
 * リプレイ本体アップロードの上限バイト数。
 *
 * リプレイは入力イベントの TOML テキストで、長い譜面でも通常は数百 KB に
 * 収まる。上限がないと認証済みアカウントから R2/blob への巨大 blob 連投で
 * ストレージ DoS が可能になるため、余裕を持たせた固定上限で拒否する。
 */
export const MAX_REPLAY_BYTES = 4 * 1024 * 1024

export const IR_RANKING_MAX_OFFSET = 100_000
export const IR_PLAYER_SCORE_PAGE_SIZE = 50
export const IR_DISTRIBUTION_PAGE_SIZE = 50
export const IR_DIFFICULTY_TABLE_NOT_FOUND = 'Difficulty table not found'

export const IR_CLEAR_LABELS = [
  'NO PLAY',
  'FAILED',
  'ASSIST',
  'EASY',
  'CLEAR',
  'HARD',
  'EX HARD',
  'FULL COMBO',
  'PERFECT',
  'MAX',
] as const

export const IR_SCORE_GRADES = ['F', 'E', 'D', 'C', 'B', 'A', 'AA', 'AAA', 'MAX'] as const
export const IR_SCORE_RANK_LABELS = ['NO PLAY', ...IR_SCORE_GRADES] as const

export const CLEAR_RANK: Record<string, number> = {
  no_play: 0,
  NoPlay: 0,
  failed: 1,
  Failed: 1,
  assisted_easy_clear: 2,
  AssistEasy: 2,
  LightAssistEasy: 2,
  easy_clear: 3,
  Easy: 3,
  clear: 4,
  Normal: 4,
  hard_clear: 5,
  Hard: 5,
  ex_hard_clear: 6,
  ExHard: 6,
  full_combo: 7,
  FullCombo: 7,
  perfect: 8,
  Perfect: 8,
  Max: 9,
}
