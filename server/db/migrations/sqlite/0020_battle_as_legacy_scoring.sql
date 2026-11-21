-- BMZ Player 0.5.3 and earlier counted the BATTLE AS auto scratch as PGREAT.
-- Keep those scores as history, but remove them from bests and rankings.
-- Newer clients send play_options.auto_scratch_scoring = 'unscored'.
UPDATE scores
SET accepted = 0, rejection_reason = 'battle_auto_scratch_legacy_scoring'
WHERE double_option = 'battle_auto_scratch'
  AND accepted = 1
  AND coalesce(json_extract(play_options, '$.auto_scratch_scoring'), '') != 'unscored';
--> statement-breakpoint
DELETE FROM best_scores
WHERE double_option = 'battle_auto_scratch'
  AND EXISTS (
    SELECT 1 FROM scores
    WHERE scores.player_id = best_scores.player_id
      AND scores.chart_sha256 = best_scores.chart_sha256
      AND scores.ln_policy = best_scores.ln_policy
      AND scores.double_option = best_scores.double_option
      AND scores.rule_mode = best_scores.rule_mode
      AND scores.scoring = best_scores.scoring
      AND scores.rejection_reason = 'battle_auto_scratch_legacy_scoring'
  );
