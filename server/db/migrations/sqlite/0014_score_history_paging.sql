CREATE INDEX `idx_scores_player_scoring_accepted_received_id`
ON `scores` (`player_id`, `scoring`, `accepted`, `server_received_at`, `id`);
