ALTER TABLE `scores` ADD `rule_mode` text DEFAULT 'Beatoraja' NOT NULL;--> statement-breakpoint
ALTER TABLE `best_scores` ADD `rule_mode` text DEFAULT 'Beatoraja' NOT NULL;--> statement-breakpoint
DELETE FROM `best_scores`;--> statement-breakpoint
DROP INDEX `idx_scores_player_chart_rule`;--> statement-breakpoint
CREATE INDEX `idx_scores_player_chart_rule` ON `scores` (`player_id`,`chart_sha256`,`ln_policy`,`double_option`,`rule_mode`,`scoring`);--> statement-breakpoint
DROP INDEX `idx_scores_chart_rule`;--> statement-breakpoint
CREATE INDEX `idx_scores_chart_rule` ON `scores` (`chart_sha256`,`ln_policy`,`double_option`,`rule_mode`,`scoring`);--> statement-breakpoint
DROP INDEX `idx_best_scores_player_chart_rule`;--> statement-breakpoint
CREATE UNIQUE INDEX `idx_best_scores_player_chart_rule` ON `best_scores` (`player_id`,`chart_sha256`,`ln_policy`,`double_option`,`rule_mode`,`scoring`);--> statement-breakpoint
DROP INDEX `idx_best_scores_chart_rule_rank`;--> statement-breakpoint
CREATE INDEX `idx_best_scores_chart_rule_rank` ON `best_scores` (`chart_sha256`,`ln_policy`,`double_option`,`rule_mode`,`scoring`,`ex_score`);
