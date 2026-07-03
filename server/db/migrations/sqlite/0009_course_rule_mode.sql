DROP INDEX `idx_best_course_scores_player_course_rule`;--> statement-breakpoint
DROP INDEX `idx_best_course_scores_ranking`;--> statement-breakpoint
ALTER TABLE `best_course_scores` ADD `rule_mode` text DEFAULT 'Beatoraja' NOT NULL;--> statement-breakpoint
CREATE UNIQUE INDEX `idx_best_course_scores_player_course_rule` ON `best_course_scores` (`player_id`,`course_hash`,`gauge`,`ln_policy`,`rule_mode`,`scoring`);--> statement-breakpoint
CREATE INDEX `idx_best_course_scores_ranking` ON `best_course_scores` (`course_hash`,`gauge`,`ln_policy`,`rule_mode`,`scoring`,`ex_score`);--> statement-breakpoint
ALTER TABLE `course_scores` ADD `rule_mode` text DEFAULT 'Beatoraja' NOT NULL;