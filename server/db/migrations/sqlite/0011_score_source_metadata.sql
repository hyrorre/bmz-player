ALTER TABLE `scores` ADD `applied_double_option` text NOT NULL DEFAULT 'off';--> statement-breakpoint
ALTER TABLE `scores` ADD `source_kind` text NOT NULL DEFAULT 'local';
