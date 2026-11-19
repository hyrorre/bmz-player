ALTER TABLE replay_blob_deletions ADD COLUMN failed_attempts INTEGER NOT NULL DEFAULT 0;
--> statement-breakpoint
DROP TRIGGER increment_score_history_revision_on_update;
--> statement-breakpoint
CREATE TRIGGER increment_score_history_revision_on_update
AFTER UPDATE OF
  id, player_id, chart_sha256, ln_policy, double_option, rule_mode, scoring,
  play_options, accepted, ex_score, clear_type, clear_rank, max_combo, min_bp, min_cb,
  device_type, gauge, effective_ln_mode, played_at, server_received_at
ON scores
BEGIN
  UPDATE profiles SET score_history_revision = score_history_revision + 1
    WHERE id IN (OLD.player_id, NEW.player_id);
END;
