ALTER TABLE `profiles` ADD `daily_boundary_minutes` integer NOT NULL DEFAULT 0;--> statement-breakpoint
CREATE TABLE `difficulty_tables` (
	`id` text PRIMARY KEY NOT NULL,
	`source_url` text NOT NULL,
	`head_url` text DEFAULT '' NOT NULL,
	`name` text DEFAULT '' NOT NULL,
	`symbol` text DEFAULT '' NOT NULL,
	`level_order` text DEFAULT '[]' NOT NULL,
	`priority` integer DEFAULT 0 NOT NULL,
	`active_generation` text,
	`last_fetched_at` integer,
	`updated_at` integer DEFAULT (unixepoch('subsec') * 1000) NOT NULL
);--> statement-breakpoint
CREATE UNIQUE INDEX `idx_difficulty_tables_source` ON `difficulty_tables` (`source_url`);--> statement-breakpoint
CREATE INDEX `idx_difficulty_tables_priority` ON `difficulty_tables` (`priority`);--> statement-breakpoint
CREATE TABLE `difficulty_table_entries` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`table_id` text NOT NULL,
	`generation` text NOT NULL,
	`level` text NOT NULL,
	`md5` text DEFAULT '' NOT NULL,
	`sha256` text DEFAULT '' NOT NULL,
	`title` text DEFAULT '' NOT NULL,
	`artist` text DEFAULT '' NOT NULL,
	`comment` text DEFAULT '' NOT NULL,
	FOREIGN KEY (`table_id`) REFERENCES `difficulty_tables`(`id`) ON UPDATE no action ON DELETE cascade
);--> statement-breakpoint
CREATE INDEX `idx_difficulty_table_entries_table_generation` ON `difficulty_table_entries` (`table_id`,`generation`);--> statement-breakpoint
CREATE INDEX `idx_difficulty_table_entries_md5` ON `difficulty_table_entries` (`md5`);--> statement-breakpoint
CREATE INDEX `idx_difficulty_table_entries_sha256` ON `difficulty_table_entries` (`sha256`);--> statement-breakpoint
CREATE INDEX `idx_scores_player_played_at` ON `scores` (`player_id`,`played_at`);--> statement-breakpoint
CREATE INDEX `idx_scores_player_received_at` ON `scores` (`player_id`,`server_received_at`);
