ALTER TABLE `best_scores` ADD `best_ex_score_id` text REFERENCES `scores`(`id`) ON UPDATE no action ON DELETE cascade;--> statement-breakpoint
ALTER TABLE `best_scores` ADD `best_clear_score_id` text REFERENCES `scores`(`id`) ON UPDATE no action ON DELETE cascade;--> statement-breakpoint
ALTER TABLE `best_scores` ADD `best_max_combo_score_id` text REFERENCES `scores`(`id`) ON UPDATE no action ON DELETE cascade;--> statement-breakpoint
ALTER TABLE `best_scores` ADD `best_min_bp_score_id` text REFERENCES `scores`(`id`) ON UPDATE no action ON DELETE cascade;--> statement-breakpoint
ALTER TABLE `best_scores` ADD `best_min_cb_score_id` text REFERENCES `scores`(`id`) ON UPDATE no action ON DELETE cascade;
