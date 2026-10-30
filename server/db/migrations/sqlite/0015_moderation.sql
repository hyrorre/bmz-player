-- ユーザーBAN・譜面ブロック・監査ログ。
-- 既存テーブルは変更しない。行が無いユーザー・譜面は active として扱う。
CREATE TABLE `user_moderation` (
	`user_id` text PRIMARY KEY NOT NULL,
	`status` text DEFAULT 'active' NOT NULL,
	`reason` text DEFAULT '' NOT NULL,
	`revision` integer DEFAULT 1 NOT NULL,
	`last_change_id` text NOT NULL,
	`updated_by` text NOT NULL,
	`updated_at` integer NOT NULL,
	FOREIGN KEY (`user_id`) REFERENCES `users`(`id`) ON UPDATE no action ON DELETE cascade,
	CHECK (`status` IN ('active', 'submit_suspended', 'banned'))
);
--> statement-breakpoint
CREATE INDEX `idx_user_moderation_status` ON `user_moderation` (`status`);
--> statement-breakpoint
CREATE TABLE `chart_moderation` (
	`chart_sha256` text PRIMARY KEY NOT NULL,
	`status` text DEFAULT 'active' NOT NULL,
	`reason` text DEFAULT '' NOT NULL,
	`note` text DEFAULT '' NOT NULL,
	`revision` integer DEFAULT 1 NOT NULL,
	`last_change_id` text NOT NULL,
	`updated_by` text NOT NULL,
	`updated_at` integer NOT NULL,
	CHECK (`status` IN ('active', 'under_review', 'hidden', 'blocked'))
);
--> statement-breakpoint
CREATE INDEX `idx_chart_moderation_status` ON `chart_moderation` (`status`);
--> statement-breakpoint
CREATE TABLE `moderation_audit_logs` (
	`id` text PRIMARY KEY NOT NULL,
	`actor_user_id` text NOT NULL,
	`target_type` text NOT NULL,
	`target_id` text NOT NULL,
	`action` text NOT NULL,
	`previous_status` text NOT NULL,
	`new_status` text NOT NULL,
	`reason` text DEFAULT '' NOT NULL,
	`metadata` text DEFAULT '{}' NOT NULL,
	`created_at` integer NOT NULL,
	CHECK (`target_type` IN ('user', 'chart')),
	CHECK (`action` IN ('user_suspend', 'user_ban', 'user_restore', 'chart_review', 'chart_hide', 'chart_block', 'chart_restore'))
);
--> statement-breakpoint
CREATE INDEX `idx_moderation_audit_logs_created` ON `moderation_audit_logs` (`created_at`, `id`);
--> statement-breakpoint
CREATE INDEX `idx_moderation_audit_logs_target` ON `moderation_audit_logs` (`target_type`, `target_id`, `created_at`);
--> statement-breakpoint
CREATE INDEX `idx_moderation_audit_logs_actor` ON `moderation_audit_logs` (`actor_user_id`, `created_at`);
--> statement-breakpoint
-- 監査ログは追記専用。通常のSQLから更新・削除できないようにする。
CREATE TRIGGER `moderation_audit_logs_no_update`
BEFORE UPDATE ON `moderation_audit_logs`
BEGIN
	SELECT RAISE(ABORT, 'moderation_audit_logs is append-only');
END;
--> statement-breakpoint
CREATE TRIGGER `moderation_audit_logs_no_delete`
BEFORE DELETE ON `moderation_audit_logs`
BEGIN
	SELECT RAISE(ABORT, 'moderation_audit_logs is append-only');
END;
