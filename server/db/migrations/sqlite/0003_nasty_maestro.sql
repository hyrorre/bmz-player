ALTER TABLE `sessions` ADD `client_type` text DEFAULT 'web' NOT NULL;--> statement-breakpoint
ALTER TABLE `sessions` ADD `last_used_at` integer;