ALTER TABLE profiles ADD COLUMN score_history_revision INTEGER NOT NULL DEFAULT 0;
--> statement-breakpoint
CREATE TABLE replay_blob_deletions (
  object_path TEXT PRIMARY KEY NOT NULL,
  next_attempt_at INTEGER NOT NULL DEFAULT 0
);
--> statement-breakpoint
CREATE INDEX idx_replay_blob_deletions_due
  ON replay_blob_deletions (next_attempt_at, object_path);
--> statement-breakpoint
CREATE TRIGGER increment_score_history_revision_on_insert AFTER INSERT ON scores
BEGIN
  UPDATE profiles SET score_history_revision = score_history_revision + 1 WHERE id = NEW.player_id;
END;
--> statement-breakpoint
CREATE TRIGGER increment_score_history_revision_on_update AFTER UPDATE ON scores
BEGIN
  UPDATE profiles SET score_history_revision = score_history_revision + 1
    WHERE id IN (OLD.player_id, NEW.player_id);
END;
--> statement-breakpoint
CREATE TRIGGER preserve_replay_path_before_score_delete BEFORE DELETE ON scores
BEGIN
  INSERT OR IGNORE INTO replay_blob_deletions (object_path)
    SELECT object_path FROM replay_objects WHERE score_id = OLD.id AND object_path IS NOT NULL;
  UPDATE profiles SET score_history_revision = score_history_revision + 1 WHERE id = OLD.player_id;
END;
