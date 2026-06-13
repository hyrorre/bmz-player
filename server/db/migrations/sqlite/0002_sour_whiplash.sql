CREATE TABLE `auth_rate_limits` (
	`action` text NOT NULL,
	`scope` text NOT NULL,
	`scope_hash` text NOT NULL,
	`window_start` integer NOT NULL,
	`attempts` integer DEFAULT 0 NOT NULL,
	`updated_at` integer DEFAULT (unixepoch('subsec') * 1000) NOT NULL,
	PRIMARY KEY(`action`, `scope`, `scope_hash`, `window_start`)
);
--> statement-breakpoint
CREATE INDEX `idx_auth_rate_limits_updated_at` ON `auth_rate_limits` (`updated_at`);