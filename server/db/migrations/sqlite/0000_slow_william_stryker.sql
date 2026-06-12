CREATE TABLE `best_course_scores` (
	`id` text PRIMARY KEY NOT NULL,
	`player_id` text NOT NULL,
	`course_hash` text NOT NULL,
	`course_score_id` text NOT NULL,
	`ex_score` integer NOT NULL,
	`clear_type` text NOT NULL,
	`clear_rank` integer NOT NULL,
	`course_clear` integer NOT NULL,
	`max_combo` integer NOT NULL,
	`bp` integer NOT NULL,
	`device_type` text NOT NULL,
	`gauge` text NOT NULL,
	`ln_policy` text NOT NULL,
	`scoring` text NOT NULL,
	`played_at` integer,
	`server_received_at` integer NOT NULL,
	`verification` text DEFAULT 'unverified' NOT NULL,
	FOREIGN KEY (`player_id`) REFERENCES `profiles`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`course_hash`) REFERENCES `ir_courses`(`course_hash`) ON UPDATE no action ON DELETE no action,
	FOREIGN KEY (`course_score_id`) REFERENCES `course_scores`(`id`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE UNIQUE INDEX `idx_best_course_scores_player_course_rule` ON `best_course_scores` (`player_id`,`course_hash`,`gauge`,`ln_policy`,`scoring`);--> statement-breakpoint
CREATE INDEX `idx_best_course_scores_ranking` ON `best_course_scores` (`course_hash`,`gauge`,`ln_policy`,`scoring`,`ex_score`);--> statement-breakpoint
CREATE TABLE `best_scores` (
	`id` text PRIMARY KEY NOT NULL,
	`player_id` text NOT NULL,
	`chart_sha256` text NOT NULL,
	`score_id` text NOT NULL,
	`ex_score` integer NOT NULL,
	`clear_type` text NOT NULL,
	`clear_rank` integer NOT NULL,
	`max_combo` integer NOT NULL,
	`min_bp` integer NOT NULL,
	`min_cb` integer NOT NULL,
	`device_type` text DEFAULT 'keyboard' NOT NULL,
	`double_option` text DEFAULT 'off' NOT NULL,
	`gauge` text NOT NULL,
	`ln_policy` text NOT NULL,
	`effective_ln_mode` text NOT NULL,
	`scoring` text NOT NULL,
	`played_at` integer,
	`server_received_at` integer NOT NULL,
	`verification` text DEFAULT 'unverified' NOT NULL,
	`updated_at` integer DEFAULT (unixepoch('subsec') * 1000) NOT NULL,
	FOREIGN KEY (`player_id`) REFERENCES `profiles`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`chart_sha256`) REFERENCES `charts`(`sha256`) ON UPDATE no action ON DELETE restrict,
	FOREIGN KEY (`score_id`) REFERENCES `scores`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `idx_best_scores_player_chart_rule` ON `best_scores` (`player_id`,`chart_sha256`,`gauge`,`ln_policy`,`scoring`,`double_option`);--> statement-breakpoint
CREATE INDEX `idx_best_scores_chart_rule_rank` ON `best_scores` (`chart_sha256`,`gauge`,`ln_policy`,`scoring`,`double_option`,`ex_score`);--> statement-breakpoint
CREATE INDEX `idx_best_scores_player` ON `best_scores` (`player_id`);--> statement-breakpoint
CREATE TABLE `charts` (
	`sha256` text PRIMARY KEY NOT NULL,
	`md5` text,
	`title` text DEFAULT '' NOT NULL,
	`subtitle` text,
	`genre` text,
	`artist` text,
	`subartists` text DEFAULT '[]' NOT NULL,
	`mode` text NOT NULL,
	`level` integer,
	`total` integer,
	`judge_rank` integer,
	`min_bpm` integer,
	`max_bpm` integer,
	`notes` integer DEFAULT 0 NOT NULL,
	`ln_notes` integer DEFAULT 0 NOT NULL,
	`cn_notes` integer DEFAULT 0 NOT NULL,
	`hcn_notes` integer DEFAULT 0 NOT NULL,
	`mine_notes` integer DEFAULT 0 NOT NULL,
	`has_random` integer DEFAULT false NOT NULL,
	`has_stop` integer DEFAULT false NOT NULL,
	`has_undefined_ln` integer DEFAULT false NOT NULL,
	`has_defined_ln` integer DEFAULT false NOT NULL,
	`has_defined_cn` integer DEFAULT false NOT NULL,
	`has_defined_hcn` integer DEFAULT false NOT NULL,
	`has_ln` integer DEFAULT false NOT NULL,
	`has_cn` integer DEFAULT false NOT NULL,
	`has_hcn` integer DEFAULT false NOT NULL,
	`has_mine` integer DEFAULT false NOT NULL,
	`source_url` text,
	`append_url` text,
	`headers` text DEFAULT '{}' NOT NULL,
	`created_at` integer DEFAULT (unixepoch('subsec') * 1000) NOT NULL,
	`updated_at` integer DEFAULT (unixepoch('subsec') * 1000) NOT NULL
);
--> statement-breakpoint
CREATE INDEX `idx_charts_md5` ON `charts` (`md5`);--> statement-breakpoint
CREATE INDEX `idx_charts_title` ON `charts` (`title`);--> statement-breakpoint
CREATE TABLE `course_scores` (
	`id` text PRIMARY KEY NOT NULL,
	`player_id` text NOT NULL,
	`course_hash` text NOT NULL,
	`client_name` text NOT NULL,
	`client_version` text NOT NULL,
	`platform` text NOT NULL,
	`gauge` text NOT NULL,
	`ln_policy` text NOT NULL,
	`scoring` text NOT NULL,
	`clear_type` text NOT NULL,
	`clear_rank` integer NOT NULL,
	`course_clear` integer NOT NULL,
	`course_failed` integer NOT NULL,
	`played_entries` integer NOT NULL,
	`trophies` text DEFAULT '[]' NOT NULL,
	`ex_score` integer NOT NULL,
	`max_ex_score` integer NOT NULL,
	`max_combo` integer NOT NULL,
	`bp` integer NOT NULL,
	`judges` text DEFAULT '{}' NOT NULL,
	`gauge_value` integer NOT NULL,
	`entries` text DEFAULT '[]' NOT NULL,
	`played_at` integer,
	`server_received_at` integer DEFAULT (unixepoch('subsec') * 1000) NOT NULL,
	`device_type` text NOT NULL,
	`evidence` text DEFAULT '{}' NOT NULL,
	`verification` text DEFAULT 'unverified' NOT NULL,
	`accepted` integer DEFAULT true NOT NULL,
	`idempotency_key` text NOT NULL,
	FOREIGN KEY (`player_id`) REFERENCES `profiles`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`course_hash`) REFERENCES `ir_courses`(`course_hash`) ON UPDATE no action ON DELETE no action
);
--> statement-breakpoint
CREATE UNIQUE INDEX `idx_course_scores_player_idempotency` ON `course_scores` (`player_id`,`idempotency_key`);--> statement-breakpoint
CREATE INDEX `idx_course_scores_course` ON `course_scores` (`course_hash`,`server_received_at`);--> statement-breakpoint
CREATE TABLE `device_keys` (
	`id` text PRIMARY KEY NOT NULL,
	`player_id` text NOT NULL,
	`public_key` text NOT NULL,
	`algorithm` text DEFAULT 'ed25519' NOT NULL,
	`revoked_at` integer,
	`created_at` integer DEFAULT (unixepoch('subsec') * 1000) NOT NULL,
	FOREIGN KEY (`player_id`) REFERENCES `profiles`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `idx_device_keys_player` ON `device_keys` (`player_id`);--> statement-breakpoint
CREATE TABLE `ir_courses` (
	`course_hash` text PRIMARY KEY NOT NULL,
	`title` text DEFAULT '' NOT NULL,
	`kind` text DEFAULT 'course' NOT NULL,
	`charts` text DEFAULT '[]' NOT NULL,
	`chart_count` integer NOT NULL,
	`constraints` text DEFAULT '{}' NOT NULL,
	`source_url` text,
	`created_at` integer DEFAULT (unixepoch('subsec') * 1000) NOT NULL,
	`updated_at` integer DEFAULT (unixepoch('subsec') * 1000) NOT NULL
);
--> statement-breakpoint
CREATE TABLE `profiles` (
	`id` text PRIMARY KEY NOT NULL,
	`display_name` text DEFAULT '' NOT NULL,
	`bio` text DEFAULT '' NOT NULL,
	`created_at` integer DEFAULT (unixepoch('subsec') * 1000) NOT NULL,
	`updated_at` integer DEFAULT (unixepoch('subsec') * 1000) NOT NULL,
	FOREIGN KEY (`id`) REFERENCES `users`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE TABLE `replay_objects` (
	`id` text PRIMARY KEY NOT NULL,
	`score_id` text NOT NULL,
	`player_id` text NOT NULL,
	`object_path` text,
	`hash` text NOT NULL,
	`format` text NOT NULL,
	`status` text DEFAULT 'metadata_only' NOT NULL,
	`size_bytes` integer,
	`created_at` integer DEFAULT (unixepoch('subsec') * 1000) NOT NULL,
	`updated_at` integer DEFAULT (unixepoch('subsec') * 1000) NOT NULL,
	FOREIGN KEY (`score_id`) REFERENCES `scores`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`player_id`) REFERENCES `profiles`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `idx_replay_objects_score` ON `replay_objects` (`score_id`);--> statement-breakpoint
CREATE INDEX `idx_replay_objects_player` ON `replay_objects` (`player_id`);--> statement-breakpoint
CREATE TABLE `rival_relationships` (
	`owner_player_id` text NOT NULL,
	`target_player_id` text NOT NULL,
	`relation_type` text DEFAULT 'rival' NOT NULL,
	`created_at` integer DEFAULT (unixepoch('subsec') * 1000) NOT NULL,
	PRIMARY KEY(`owner_player_id`, `target_player_id`, `relation_type`),
	FOREIGN KEY (`owner_player_id`) REFERENCES `profiles`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`target_player_id`) REFERENCES `profiles`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `idx_rival_relationships_target` ON `rival_relationships` (`target_player_id`);--> statement-breakpoint
CREATE TABLE `scores` (
	`id` text PRIMARY KEY NOT NULL,
	`player_id` text NOT NULL,
	`chart_sha256` text NOT NULL,
	`client_name` text NOT NULL,
	`client_version` text NOT NULL,
	`platform` text NOT NULL,
	`play_mode` text NOT NULL,
	`key_mode` text NOT NULL,
	`gauge` text NOT NULL,
	`ln_policy` text NOT NULL,
	`effective_ln_mode` text NOT NULL,
	`judge_algorithm` text NOT NULL,
	`scoring` text NOT NULL,
	`clear_type` text NOT NULL,
	`clear_rank` integer NOT NULL,
	`played_at` integer,
	`server_received_at` integer DEFAULT (unixepoch('subsec') * 1000) NOT NULL,
	`duration_ms` integer,
	`judges` text DEFAULT '{}' NOT NULL,
	`ex_score` integer NOT NULL,
	`avg_judge_ms` integer,
	`max_combo` integer NOT NULL,
	`notes` integer NOT NULL,
	`pass_notes` integer NOT NULL,
	`bp` integer NOT NULL,
	`cb` integer NOT NULL,
	`min_bp` integer NOT NULL,
	`min_cb` integer NOT NULL,
	`device_type` text DEFAULT 'keyboard' NOT NULL,
	`double_option` text DEFAULT 'off' NOT NULL,
	`play_options` text DEFAULT '{}' NOT NULL,
	`replay_hash` text,
	`replay_format` text,
	`replay_upload_intent` text,
	`evidence` text DEFAULT '{}' NOT NULL,
	`verification` text DEFAULT 'unverified' NOT NULL,
	`accepted` integer DEFAULT true NOT NULL,
	`rejection_reason` text,
	`idempotency_key` text NOT NULL,
	`created_at` integer DEFAULT (unixepoch('subsec') * 1000) NOT NULL,
	FOREIGN KEY (`player_id`) REFERENCES `profiles`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`chart_sha256`) REFERENCES `charts`(`sha256`) ON UPDATE no action ON DELETE restrict
);
--> statement-breakpoint
CREATE UNIQUE INDEX `idx_scores_player_idempotency` ON `scores` (`player_id`,`idempotency_key`);--> statement-breakpoint
CREATE INDEX `idx_scores_player_chart_rule` ON `scores` (`player_id`,`chart_sha256`,`gauge`,`ln_policy`,`scoring`,`double_option`);--> statement-breakpoint
CREATE INDEX `idx_scores_chart_rule` ON `scores` (`chart_sha256`,`gauge`,`ln_policy`,`scoring`,`double_option`);--> statement-breakpoint
CREATE INDEX `idx_scores_received_at` ON `scores` (`server_received_at`);--> statement-breakpoint
CREATE TABLE `sessions` (
	`token_hash` text PRIMARY KEY NOT NULL,
	`user_id` text NOT NULL,
	`kind` text NOT NULL,
	`expires_at` integer NOT NULL,
	`revoked_at` integer,
	`created_at` integer DEFAULT (unixepoch('subsec') * 1000) NOT NULL,
	FOREIGN KEY (`user_id`) REFERENCES `users`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `idx_sessions_user` ON `sessions` (`user_id`);--> statement-breakpoint
CREATE INDEX `idx_sessions_kind_expiry` ON `sessions` (`kind`,`expires_at`);--> statement-breakpoint
CREATE TABLE `users` (
	`id` text PRIMARY KEY NOT NULL,
	`email` text NOT NULL,
	`password_hash` text NOT NULL,
	`created_at` integer DEFAULT (unixepoch('subsec') * 1000) NOT NULL,
	`updated_at` integer DEFAULT (unixepoch('subsec') * 1000) NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX `idx_users_email` ON `users` (`email`);