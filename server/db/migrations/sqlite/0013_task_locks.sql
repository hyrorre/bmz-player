CREATE TABLE `task_locks` (
	`name` text PRIMARY KEY NOT NULL,
	`owner` text NOT NULL,
	`lease_until` integer NOT NULL
);
