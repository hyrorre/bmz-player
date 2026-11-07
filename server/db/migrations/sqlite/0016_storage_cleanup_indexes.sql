CREATE INDEX `idx_device_keys_player_revoked` ON `device_keys` (`player_id`, `revoked_at`);
CREATE INDEX `idx_device_keys_revoked` ON `device_keys` (`revoked_at`);
CREATE INDEX `idx_replay_objects_status_updated` ON `replay_objects` (`status`, `updated_at`);
CREATE INDEX `idx_replay_objects_blob_cleanup` ON `replay_objects` (`status`, `updated_at`) WHERE `object_path` IS NOT NULL;
