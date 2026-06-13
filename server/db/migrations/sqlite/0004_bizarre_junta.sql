ALTER TABLE `sessions` ADD `session_group_id` text;--> statement-breakpoint
CREATE INDEX `idx_sessions_group` ON `sessions` (`user_id`,`session_group_id`);