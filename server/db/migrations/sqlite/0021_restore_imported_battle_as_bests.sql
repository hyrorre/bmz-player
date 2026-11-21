-- 0020 also rejected imported BATTLE AS scores and removed valid bests on the same key.
UPDATE scores
SET accepted = 1, rejection_reason = NULL
WHERE double_option = 'battle_auto_scratch'
  AND source_kind <> 'local'
  AND accepted = 0
  AND rejection_reason = 'battle_auto_scratch_legacy_scoring';
--> statement-breakpoint
-- Rank the accepted histories independently for EX, lamp, combo, BP and CB.
WITH ranked AS (
  SELECT scores.*,
    row_number() OVER (
      score_key ORDER BY ex_score DESC, clear_rank DESC, min_bp, min_cb, max_combo DESC,
        coalesce(played_at, server_received_at), id
    ) AS ex_position,
    first_value(id) OVER clear_order AS clear_id,
    first_value(clear_type) OVER clear_order AS best_clear_type,
    first_value(clear_rank) OVER clear_order AS best_clear_rank,
    first_value(id) OVER combo_order AS combo_id,
    first_value(max_combo) OVER combo_order AS best_combo,
    first_value(id) OVER bp_order AS bp_id,
    first_value(min_bp) OVER bp_order AS best_bp,
    first_value(id) OVER cb_order AS cb_id,
    first_value(min_cb) OVER cb_order AS best_cb
  FROM scores
  WHERE double_option = 'battle_auto_scratch' AND accepted = 1
  WINDOW
    score_key AS (PARTITION BY player_id, chart_sha256, ln_policy, double_option, rule_mode, scoring),
    clear_order AS (score_key ORDER BY clear_rank DESC, coalesce(played_at, server_received_at), id),
    combo_order AS (score_key ORDER BY max_combo DESC, server_received_at, id),
    bp_order AS (score_key ORDER BY min_bp, server_received_at, id),
    cb_order AS (score_key ORDER BY min_cb, server_received_at, id)
)
INSERT OR REPLACE INTO best_scores (
  id, player_id, chart_sha256, score_id, best_ex_score_id, best_clear_score_id,
  best_max_combo_score_id, best_min_bp_score_id, best_min_cb_score_id,
  ex_score, clear_type, clear_rank, max_combo, min_bp, min_cb, device_type, double_option,
  gauge, ln_policy, effective_ln_mode, rule_mode, scoring, played_at, server_received_at,
  verification
)
SELECT coalesce(existing.id, lower(hex(randomblob(16)))), r.player_id, r.chart_sha256,
  r.id, r.id, r.clear_id, r.combo_id, r.bp_id, r.cb_id,
  r.ex_score, r.best_clear_type, r.best_clear_rank, r.best_combo, r.best_bp, r.best_cb,
  r.device_type, r.double_option, r.gauge, r.ln_policy, r.effective_ln_mode, r.rule_mode,
  r.scoring, r.played_at, r.server_received_at, r.verification
FROM ranked r
LEFT JOIN best_scores existing USING (player_id, chart_sha256, ln_policy, double_option, rule_mode, scoring)
WHERE r.ex_position = 1;
