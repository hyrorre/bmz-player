use super::*;

pub const SCORE_MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        statements: &[
            "CREATE TABLE score_history (
            id INTEGER PRIMARY KEY,
            chart_sha256 TEXT NOT NULL,
            played_at INTEGER NOT NULL,
            clear_type TEXT NOT NULL,
            gauge_type TEXT NOT NULL,
            gauge_value REAL NOT NULL,
            total_notes INTEGER NOT NULL,
            ex_score INTEGER NOT NULL,
            bp INTEGER NOT NULL,
            cb INTEGER NOT NULL,
            max_combo INTEGER NOT NULL,
            fast_pgreat INTEGER NOT NULL,
            slow_pgreat INTEGER NOT NULL,
            fast_great INTEGER NOT NULL,
            slow_great INTEGER NOT NULL,
            fast_good INTEGER NOT NULL,
            slow_good INTEGER NOT NULL,
            fast_bad INTEGER NOT NULL,
            slow_bad INTEGER NOT NULL,
            fast_poor INTEGER NOT NULL,
            slow_poor INTEGER NOT NULL,
            fast_empty_poor INTEGER NOT NULL,
            slow_empty_poor INTEGER NOT NULL,
            random_seed INTEGER,
            gauge_option TEXT NOT NULL,
            assist_mask INTEGER NOT NULL DEFAULT 0,
            autoplay INTEGER NOT NULL DEFAULT 0,
            replay_path TEXT NOT NULL
        );",
            "CREATE TABLE score_best (
            chart_sha256 TEXT PRIMARY KEY,
            clear_type TEXT NOT NULL,
            gauge_type TEXT NOT NULL,
            gauge_value REAL NOT NULL,
            ex_score INTEGER NOT NULL,
            bp INTEGER NOT NULL,
            cb INTEGER NOT NULL,
            max_combo INTEGER NOT NULL,
            fast_pgreat INTEGER NOT NULL,
            slow_pgreat INTEGER NOT NULL,
            fast_great INTEGER NOT NULL,
            slow_great INTEGER NOT NULL,
            fast_good INTEGER NOT NULL,
            slow_good INTEGER NOT NULL,
            fast_bad INTEGER NOT NULL,
            slow_bad INTEGER NOT NULL,
            fast_poor INTEGER NOT NULL,
            slow_poor INTEGER NOT NULL,
            fast_empty_poor INTEGER NOT NULL,
            slow_empty_poor INTEGER NOT NULL,
            played_at INTEGER NOT NULL,
            replay_path TEXT NOT NULL
        );",
            "CREATE INDEX idx_score_history_chart_sha256 ON score_history(chart_sha256);",
            "CREATE INDEX idx_score_history_played_at ON score_history(played_at DESC);",
            "CREATE INDEX idx_score_best_clear_type ON score_best(clear_type);",
            "CREATE INDEX idx_score_best_ex_score ON score_best(ex_score DESC);",
        ],
    },
    Migration {
        version: 2,
        statements: &[
            "CREATE TABLE replay_slots (
            chart_sha256 TEXT NOT NULL,
            slot         INTEGER NOT NULL CHECK (slot BETWEEN 0 AND 3),
            rule         TEXT NOT NULL,
            replay_path  TEXT NOT NULL,
            played_at    INTEGER NOT NULL,
            ex_score     INTEGER NOT NULL,
            bp           INTEGER NOT NULL,
            cb           INTEGER NOT NULL,
            max_combo    INTEGER NOT NULL,
            clear_rank   INTEGER NOT NULL,
            PRIMARY KEY(chart_sha256, slot)
        );",
            "CREATE INDEX idx_replay_slots_chart ON replay_slots(chart_sha256);",
        ],
    },
    Migration {
        version: 3,
        statements: &[
            "ALTER TABLE score_history ADD COLUMN ghost TEXT NOT NULL DEFAULT '';",
            "ALTER TABLE score_best ADD COLUMN ghost TEXT NOT NULL DEFAULT '';",
        ],
    },
    Migration {
        version: 4,
        // Per-chart score history rows can be tagged with the score.db
        // `course_scores.id` of the course attempt they belong to, so a chart
        // play can be traced back to its course context.  NULL means "solo
        // play" or "course history written before this migration".
        //
        // No FK was added when the column was introduced, because course_scores
        // lived in library.db at the time.  Keep it as a plain nullable integer
        // for existing DB compatibility.
        statements: &[
            "ALTER TABLE score_history ADD COLUMN course_score_id INTEGER;",
            "CREATE INDEX idx_score_history_course_score_id
                ON score_history(course_score_id)
                WHERE course_score_id IS NOT NULL;",
        ],
    },
    Migration {
        version: 5,
        statements: &[
            "ALTER TABLE score_history ADD COLUMN rule_mode TEXT NOT NULL DEFAULT 'Beatoraja';",
        ],
    },
    Migration {
        version: 6,
        statements: &[
            "ALTER TABLE score_best ADD COLUMN play_count INTEGER NOT NULL DEFAULT 0;",
            "ALTER TABLE score_best ADD COLUMN clear_count INTEGER NOT NULL DEFAULT 0;",
            "UPDATE score_best
                SET play_count = (
                    SELECT COUNT(*)
                    FROM score_history
                    WHERE score_history.chart_sha256 = score_best.chart_sha256
                ),
                clear_count = (
                    SELECT COUNT(*)
                    FROM score_history
                    WHERE score_history.chart_sha256 = score_best.chart_sha256
                      AND score_history.clear_type NOT IN ('', 'NoPlay', 'Failed')
                );",
        ],
    },
    Migration {
        version: 7,
        // Split per-chart best scores and replay slots by normalized BMZ LN
        // score policy. Existing rows are imported as ForceLn, the canonical
        // policy for old score.db files that predate policy-aware storage.
        statements: &[
            "ALTER TABLE score_history ADD COLUMN ln_policy TEXT NOT NULL DEFAULT 'ForceLn';",
            "ALTER TABLE score_best RENAME TO score_best_old;",
            "CREATE TABLE score_best (
                chart_sha256 TEXT NOT NULL,
                ln_policy TEXT NOT NULL,
                clear_type TEXT NOT NULL,
                gauge_type TEXT NOT NULL,
                gauge_value REAL NOT NULL,
                ex_score INTEGER NOT NULL,
                bp INTEGER NOT NULL,
                cb INTEGER NOT NULL,
                max_combo INTEGER NOT NULL,
                fast_pgreat INTEGER NOT NULL,
                slow_pgreat INTEGER NOT NULL,
                fast_great INTEGER NOT NULL,
                slow_great INTEGER NOT NULL,
                fast_good INTEGER NOT NULL,
                slow_good INTEGER NOT NULL,
                fast_bad INTEGER NOT NULL,
                slow_bad INTEGER NOT NULL,
                fast_poor INTEGER NOT NULL,
                slow_poor INTEGER NOT NULL,
                fast_empty_poor INTEGER NOT NULL,
                slow_empty_poor INTEGER NOT NULL,
                played_at INTEGER NOT NULL,
                replay_path TEXT NOT NULL,
                ghost TEXT NOT NULL DEFAULT '',
                play_count INTEGER NOT NULL DEFAULT 0,
                clear_count INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY(chart_sha256, ln_policy)
            );",
            "INSERT INTO score_best (
                chart_sha256, ln_policy, clear_type, gauge_type, gauge_value,
                ex_score, bp, cb, max_combo, fast_pgreat, slow_pgreat,
                fast_great, slow_great, fast_good, slow_good, fast_bad,
                slow_bad, fast_poor, slow_poor, fast_empty_poor,
                slow_empty_poor, played_at, replay_path, ghost,
                play_count, clear_count
            )
            SELECT
                chart_sha256, 'ForceLn', clear_type, gauge_type, gauge_value,
                ex_score, bp, cb, max_combo, fast_pgreat, slow_pgreat,
                fast_great, slow_great, fast_good, slow_good, fast_bad,
                slow_bad, fast_poor, slow_poor, fast_empty_poor,
                slow_empty_poor, played_at, replay_path, ghost,
                play_count, clear_count
            FROM score_best_old;",
            "DROP TABLE score_best_old;",
            "DROP INDEX IF EXISTS idx_score_best_clear_type;",
            "DROP INDEX IF EXISTS idx_score_best_ex_score;",
            "CREATE INDEX idx_score_best_chart ON score_best(chart_sha256);",
            "CREATE INDEX idx_score_best_clear_type ON score_best(clear_type);",
            "CREATE INDEX idx_score_best_ex_score ON score_best(ex_score DESC);",
            "ALTER TABLE replay_slots RENAME TO replay_slots_old;",
            "CREATE TABLE replay_slots (
                chart_sha256 TEXT NOT NULL,
                ln_policy   TEXT NOT NULL,
                slot        INTEGER NOT NULL CHECK (slot BETWEEN 0 AND 3),
                rule        TEXT NOT NULL,
                replay_path TEXT NOT NULL,
                played_at   INTEGER NOT NULL,
                ex_score    INTEGER NOT NULL,
                bp          INTEGER NOT NULL,
                cb          INTEGER NOT NULL,
                max_combo   INTEGER NOT NULL,
                clear_rank  INTEGER NOT NULL,
                PRIMARY KEY(chart_sha256, ln_policy, slot)
            );",
            "INSERT INTO replay_slots (
                chart_sha256, ln_policy, slot, rule, replay_path, played_at,
                ex_score, bp, cb, max_combo, clear_rank
            )
            SELECT
                chart_sha256, 'ForceLn', slot, rule, replay_path, played_at,
                ex_score, bp, cb, max_combo, clear_rank
            FROM replay_slots_old;",
            "DROP TABLE replay_slots_old;",
            "DROP INDEX IF EXISTS idx_replay_slots_chart;",
            "CREATE INDEX idx_replay_slots_chart ON replay_slots(chart_sha256, ln_policy);",
        ],
    },
    Migration {
        version: 8,
        // Profile-wide player metadata/statistics and per-play previous-best
        // snapshots.  `score_history.old_*` stores the best score before this
        // play for the same (chart_sha256, ln_policy), so result/update deltas
        // can be reconstructed without a separate log database.
        statements: &[
            "CREATE TABLE player_info (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                player_uuid TEXT NOT NULL,
                display_name TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );",
            "INSERT INTO player_info (id, player_uuid, display_name, created_at, updated_at)
             VALUES (
                1,
                lower(hex(randomblob(16))),
                '',
                CAST(strftime('%s', 'now') AS INTEGER),
                CAST(strftime('%s', 'now') AS INTEGER)
             );",
            "CREATE TABLE player_stats (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                play_count INTEGER NOT NULL DEFAULT 0,
                clear_count INTEGER NOT NULL DEFAULT 0,
                max_combo INTEGER NOT NULL DEFAULT 0,
                fast_pgreat INTEGER NOT NULL DEFAULT 0,
                slow_pgreat INTEGER NOT NULL DEFAULT 0,
                fast_great INTEGER NOT NULL DEFAULT 0,
                slow_great INTEGER NOT NULL DEFAULT 0,
                fast_good INTEGER NOT NULL DEFAULT 0,
                slow_good INTEGER NOT NULL DEFAULT 0,
                fast_bad INTEGER NOT NULL DEFAULT 0,
                slow_bad INTEGER NOT NULL DEFAULT 0,
                fast_poor INTEGER NOT NULL DEFAULT 0,
                slow_poor INTEGER NOT NULL DEFAULT 0,
                fast_empty_poor INTEGER NOT NULL DEFAULT 0,
                slow_empty_poor INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
            );",
            "INSERT INTO player_stats (
                id, play_count, clear_count, max_combo,
                fast_pgreat, slow_pgreat, fast_great, slow_great,
                fast_good, slow_good, fast_bad, slow_bad,
                fast_poor, slow_poor, fast_empty_poor, slow_empty_poor,
                updated_at
            )
            SELECT
                1,
                COUNT(*),
                COALESCE(SUM(CASE WHEN clear_type NOT IN ('', 'NoPlay', 'Failed') THEN 1 ELSE 0 END), 0),
                COALESCE(MAX(max_combo), 0),
                COALESCE(SUM(fast_pgreat), 0),
                COALESCE(SUM(slow_pgreat), 0),
                COALESCE(SUM(fast_great), 0),
                COALESCE(SUM(slow_great), 0),
                COALESCE(SUM(fast_good), 0),
                COALESCE(SUM(slow_good), 0),
                COALESCE(SUM(fast_bad), 0),
                COALESCE(SUM(slow_bad), 0),
                COALESCE(SUM(fast_poor), 0),
                COALESCE(SUM(slow_poor), 0),
                COALESCE(SUM(fast_empty_poor), 0),
                COALESCE(SUM(slow_empty_poor), 0),
                COALESCE(MAX(played_at), 0)
            FROM score_history;",
            "ALTER TABLE score_history ADD COLUMN old_clear_type TEXT;",
            "ALTER TABLE score_history ADD COLUMN old_ex_score INTEGER;",
            "ALTER TABLE score_history ADD COLUMN old_max_combo INTEGER;",
            "ALTER TABLE score_history ADD COLUMN old_bp INTEGER;",
            "ALTER TABLE score_history ADD COLUMN old_cb INTEGER;",
        ],
    },
    Migration {
        version: 9,
        statements: &[
            "CREATE TABLE ir_accounts (
                provider TEXT NOT NULL,
                account_id TEXT NOT NULL,
                account_display_name TEXT NOT NULL DEFAULT '',
                role TEXT NOT NULL DEFAULT 'submit_only',
                enabled INTEGER NOT NULL DEFAULT 1,
                last_login_at INTEGER,
                last_success_at INTEGER,
                PRIMARY KEY(provider, account_id)
            );",
            "CREATE TABLE ir_score_jobs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                provider TEXT NOT NULL,
                account_id TEXT NOT NULL DEFAULT '',
                local_score_id INTEGER NOT NULL,
                chart_sha256 TEXT NOT NULL,
                ln_policy TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                attempt_count INTEGER NOT NULL DEFAULT 0,
                next_attempt_at INTEGER NOT NULL DEFAULT 0,
                last_error TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                UNIQUE(provider, account_id, local_score_id)
            );",
            "CREATE INDEX idx_ir_score_jobs_status_next_attempt
                ON ir_score_jobs(status, next_attempt_at);",
            "CREATE INDEX idx_ir_score_jobs_local_score
                ON ir_score_jobs(local_score_id);",
            "CREATE TABLE ir_score_submissions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                job_id INTEGER NOT NULL,
                provider TEXT NOT NULL,
                account_id TEXT NOT NULL DEFAULT '',
                local_score_id INTEGER NOT NULL,
                remote_score_id TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL,
                submitted_at INTEGER NOT NULL,
                response_json TEXT NOT NULL DEFAULT '',
                error TEXT NOT NULL DEFAULT '',
                FOREIGN KEY(job_id) REFERENCES ir_score_jobs(id) ON DELETE CASCADE
            );",
            "CREATE INDEX idx_ir_score_submissions_local_score
                ON ir_score_submissions(local_score_id);",
        ],
    },
    Migration {
        version: 10,
        statements: &[
            "ALTER TABLE score_history ADD COLUMN device_type TEXT NOT NULL DEFAULT 'keyboard';",
            "ALTER TABLE score_best ADD COLUMN device_type TEXT NOT NULL DEFAULT 'keyboard';",
        ],
    },
    Migration {
        version: 11,
        statements: &[
            // IR ジョブにコーススコア用の kind ('score' | 'course') を追加する。
            // 単曲とコースで local_score_id の空間が別 (score_history.id /
            // course_scores.id) のため、UNIQUE に kind を含める必要があり
            // テーブルを作り直す。
            "CREATE TABLE ir_score_jobs_new (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                provider TEXT NOT NULL,
                account_id TEXT NOT NULL DEFAULT '',
                kind TEXT NOT NULL DEFAULT 'score',
                local_score_id INTEGER NOT NULL,
                chart_sha256 TEXT NOT NULL,
                ln_policy TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                attempt_count INTEGER NOT NULL DEFAULT 0,
                next_attempt_at INTEGER NOT NULL DEFAULT 0,
                last_error TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                UNIQUE(provider, account_id, kind, local_score_id)
            );",
            "INSERT INTO ir_score_jobs_new (
                id, provider, account_id, kind, local_score_id, chart_sha256,
                ln_policy, payload_json, status, attempt_count, next_attempt_at,
                last_error, created_at, updated_at
            )
            SELECT id, provider, account_id, 'score', local_score_id, chart_sha256,
                ln_policy, payload_json, status, attempt_count, next_attempt_at,
                last_error, created_at, updated_at
            FROM ir_score_jobs;",
            "DROP TABLE ir_score_jobs;",
            "ALTER TABLE ir_score_jobs_new RENAME TO ir_score_jobs;",
            "CREATE INDEX idx_ir_score_jobs_status_next_attempt
                ON ir_score_jobs(status, next_attempt_at);",
            "CREATE INDEX idx_ir_score_jobs_local_score
                ON ir_score_jobs(local_score_id);",
        ],
    },
    Migration {
        version: 12,
        statements: &[
            "ALTER TABLE score_history ADD COLUMN arrange TEXT NOT NULL DEFAULT 'Normal';",
        ],
    },
    Migration {
        version: 13,
        statements: &[
            "ALTER TABLE score_history ADD COLUMN double_option TEXT NOT NULL DEFAULT 'Off';",
            "ALTER TABLE score_best RENAME TO score_best_old;",
            "CREATE TABLE score_best (
                chart_sha256 TEXT NOT NULL,
                ln_policy TEXT NOT NULL,
                double_option TEXT NOT NULL DEFAULT 'Off',
                clear_type TEXT NOT NULL,
                gauge_type TEXT NOT NULL,
                gauge_value REAL NOT NULL,
                ex_score INTEGER NOT NULL,
                bp INTEGER NOT NULL,
                cb INTEGER NOT NULL,
                max_combo INTEGER NOT NULL,
                fast_pgreat INTEGER NOT NULL,
                slow_pgreat INTEGER NOT NULL,
                fast_great INTEGER NOT NULL,
                slow_great INTEGER NOT NULL,
                fast_good INTEGER NOT NULL,
                slow_good INTEGER NOT NULL,
                fast_bad INTEGER NOT NULL,
                slow_bad INTEGER NOT NULL,
                fast_poor INTEGER NOT NULL,
                slow_poor INTEGER NOT NULL,
                fast_empty_poor INTEGER NOT NULL,
                slow_empty_poor INTEGER NOT NULL,
                played_at INTEGER NOT NULL,
                replay_path TEXT NOT NULL,
                ghost TEXT NOT NULL DEFAULT '',
                play_count INTEGER NOT NULL DEFAULT 0,
                clear_count INTEGER NOT NULL DEFAULT 0,
                device_type TEXT NOT NULL DEFAULT 'keyboard',
                PRIMARY KEY(chart_sha256, ln_policy, double_option)
            );",
            "INSERT INTO score_best (
                chart_sha256, ln_policy, double_option, clear_type, gauge_type,
                gauge_value, ex_score, bp, cb, max_combo, fast_pgreat,
                slow_pgreat, fast_great, slow_great, fast_good, slow_good,
                fast_bad, slow_bad, fast_poor, slow_poor, fast_empty_poor,
                slow_empty_poor, played_at, replay_path, ghost, play_count,
                clear_count, device_type
            )
            SELECT
                chart_sha256, ln_policy, 'Off', clear_type, gauge_type,
                gauge_value, ex_score, bp, cb, max_combo, fast_pgreat,
                slow_pgreat, fast_great, slow_great, fast_good, slow_good,
                fast_bad, slow_bad, fast_poor, slow_poor, fast_empty_poor,
                slow_empty_poor, played_at, replay_path, ghost, play_count,
                clear_count, device_type
            FROM score_best_old;",
            "DROP TABLE score_best_old;",
            "DROP INDEX IF EXISTS idx_score_best_chart;",
            "DROP INDEX IF EXISTS idx_score_best_clear_type;",
            "DROP INDEX IF EXISTS idx_score_best_ex_score;",
            "CREATE INDEX idx_score_best_chart ON score_best(chart_sha256, ln_policy, double_option);",
            "CREATE INDEX idx_score_best_clear_type ON score_best(clear_type);",
            "CREATE INDEX idx_score_best_ex_score ON score_best(ex_score DESC);",
            "ALTER TABLE replay_slots RENAME TO replay_slots_old;",
            "CREATE TABLE replay_slots (
                chart_sha256 TEXT NOT NULL,
                ln_policy   TEXT NOT NULL,
                double_option TEXT NOT NULL DEFAULT 'Off',
                slot        INTEGER NOT NULL CHECK (slot BETWEEN 0 AND 3),
                rule        TEXT NOT NULL,
                replay_path TEXT NOT NULL,
                played_at   INTEGER NOT NULL,
                ex_score    INTEGER NOT NULL,
                bp          INTEGER NOT NULL,
                cb          INTEGER NOT NULL,
                max_combo   INTEGER NOT NULL,
                clear_rank  INTEGER NOT NULL,
                PRIMARY KEY(chart_sha256, ln_policy, double_option, slot)
            );",
            "INSERT INTO replay_slots (
                chart_sha256, ln_policy, double_option, slot, rule, replay_path,
                played_at, ex_score, bp, cb, max_combo, clear_rank
            )
            SELECT
                chart_sha256, ln_policy, 'Off', slot, rule, replay_path,
                played_at, ex_score, bp, cb, max_combo, clear_rank
            FROM replay_slots_old;",
            "DROP TABLE replay_slots_old;",
            "DROP INDEX IF EXISTS idx_replay_slots_chart;",
            "CREATE INDEX idx_replay_slots_chart
                ON replay_slots(chart_sha256, ln_policy, double_option);",
        ],
    },
    Migration {
        version: 14,
        statements: &["ALTER TABLE player_stats ADD COLUMN playtime_seconds INTEGER NOT NULL DEFAULT 0;"],
    },
    Migration {
        version: 15,
        statements: &[
            "ALTER TABLE score_best RENAME TO score_best_old;",
            "CREATE TABLE score_best (
                chart_sha256 TEXT NOT NULL,
                ln_policy TEXT NOT NULL,
                double_option TEXT NOT NULL DEFAULT 'Off',
                rule_mode TEXT NOT NULL DEFAULT 'Beatoraja',
                clear_type TEXT NOT NULL,
                gauge_type TEXT NOT NULL,
                gauge_value REAL NOT NULL,
                ex_score INTEGER NOT NULL,
                bp INTEGER NOT NULL,
                cb INTEGER NOT NULL,
                max_combo INTEGER NOT NULL,
                fast_pgreat INTEGER NOT NULL,
                slow_pgreat INTEGER NOT NULL,
                fast_great INTEGER NOT NULL,
                slow_great INTEGER NOT NULL,
                fast_good INTEGER NOT NULL,
                slow_good INTEGER NOT NULL,
                fast_bad INTEGER NOT NULL,
                slow_bad INTEGER NOT NULL,
                fast_poor INTEGER NOT NULL,
                slow_poor INTEGER NOT NULL,
                fast_empty_poor INTEGER NOT NULL,
                slow_empty_poor INTEGER NOT NULL,
                played_at INTEGER NOT NULL,
                replay_path TEXT NOT NULL,
                ghost TEXT NOT NULL DEFAULT '',
                play_count INTEGER NOT NULL DEFAULT 0,
                clear_count INTEGER NOT NULL DEFAULT 0,
                device_type TEXT NOT NULL DEFAULT 'keyboard',
                PRIMARY KEY(chart_sha256, ln_policy, double_option, rule_mode)
            );",
            "INSERT INTO score_best (
                chart_sha256, ln_policy, double_option, rule_mode, clear_type,
                gauge_type, gauge_value, ex_score, bp, cb, max_combo,
                fast_pgreat, slow_pgreat, fast_great, slow_great, fast_good,
                slow_good, fast_bad, slow_bad, fast_poor, slow_poor,
                fast_empty_poor, slow_empty_poor, played_at, replay_path,
                ghost, play_count, clear_count, device_type
            )
            SELECT
                chart_sha256, ln_policy, double_option, 'Beatoraja', clear_type,
                gauge_type, gauge_value, ex_score, bp, cb, max_combo,
                fast_pgreat, slow_pgreat, fast_great, slow_great, fast_good,
                slow_good, fast_bad, slow_bad, fast_poor, slow_poor,
                fast_empty_poor, slow_empty_poor, played_at, replay_path,
                ghost, play_count, clear_count, device_type
            FROM score_best_old;",
            "DROP TABLE score_best_old;",
            "DROP INDEX IF EXISTS idx_score_best_chart;",
            "DROP INDEX IF EXISTS idx_score_best_clear_type;",
            "DROP INDEX IF EXISTS idx_score_best_ex_score;",
            "CREATE INDEX idx_score_best_chart
                ON score_best(chart_sha256, ln_policy, double_option, rule_mode);",
            "CREATE INDEX idx_score_best_clear_type ON score_best(clear_type);",
            "CREATE INDEX idx_score_best_ex_score ON score_best(ex_score DESC);",
            "ALTER TABLE replay_slots RENAME TO replay_slots_old;",
            "CREATE TABLE replay_slots (
                chart_sha256 TEXT NOT NULL,
                ln_policy   TEXT NOT NULL,
                double_option TEXT NOT NULL DEFAULT 'Off',
                rule_mode TEXT NOT NULL DEFAULT 'Beatoraja',
                slot        INTEGER NOT NULL CHECK (slot BETWEEN 0 AND 3),
                rule        TEXT NOT NULL,
                replay_path TEXT NOT NULL,
                played_at   INTEGER NOT NULL,
                ex_score    INTEGER NOT NULL,
                bp          INTEGER NOT NULL,
                cb          INTEGER NOT NULL,
                max_combo   INTEGER NOT NULL,
                clear_rank  INTEGER NOT NULL,
                PRIMARY KEY(chart_sha256, ln_policy, double_option, rule_mode, slot)
            );",
            "INSERT INTO replay_slots (
                chart_sha256, ln_policy, double_option, rule_mode, slot, rule,
                replay_path, played_at, ex_score, bp, cb, max_combo, clear_rank
            )
            SELECT
                chart_sha256, ln_policy, double_option, 'Beatoraja', slot, rule,
                replay_path, played_at, ex_score, bp, cb, max_combo, clear_rank
            FROM replay_slots_old;",
            "DROP TABLE replay_slots_old;",
            "DROP INDEX IF EXISTS idx_replay_slots_chart;",
            "CREATE INDEX idx_replay_slots_chart
                ON replay_slots(chart_sha256, ln_policy, double_option, rule_mode);",
        ],
    },
    Migration {
        version: 16,
        statements: &[
            "CREATE TABLE course_scores (
                id INTEGER PRIMARY KEY,
                course_hash TEXT NOT NULL,
                source TEXT NOT NULL DEFAULT '',
                course_key TEXT NOT NULL DEFAULT '',
                title TEXT NOT NULL DEFAULT '',
                kind TEXT NOT NULL DEFAULT '',
                constraints_json TEXT NOT NULL DEFAULT '{}',
                chart_sha256s_json TEXT NOT NULL DEFAULT '[]',
                ex_score INTEGER NOT NULL,
                max_ex_score INTEGER NOT NULL,
                clear_type TEXT NOT NULL,
                gauge_type TEXT NOT NULL,
                gauge_value REAL NOT NULL,
                max_combo INTEGER NOT NULL,
                bp INTEGER NOT NULL,
                course_failed INTEGER NOT NULL,
                course_clear INTEGER NOT NULL,
                arrange TEXT NOT NULL DEFAULT 'Normal',
                trophies_json TEXT NOT NULL,
                played_at INTEGER NOT NULL
            );",
            "CREATE INDEX idx_score_course_scores_hash_played
                ON course_scores(course_hash, played_at);",
            "CREATE INDEX idx_score_course_scores_hash_ex_score
                ON course_scores(course_hash, ex_score DESC);",
            "CREATE INDEX idx_score_course_scores_source_key
                ON course_scores(source, course_key);",
            "CREATE TABLE course_score_charts (
                course_score_id INTEGER NOT NULL
                    REFERENCES course_scores(id) ON DELETE CASCADE,
                position INTEGER NOT NULL,
                chart_sha256 TEXT NOT NULL,
                ex_score INTEGER NOT NULL,
                max_combo INTEGER NOT NULL,
                clear_type TEXT NOT NULL,
                gauge_value REAL NOT NULL,
                PRIMARY KEY(course_score_id, position)
            );",
            "CREATE INDEX idx_score_course_score_charts_chart
                ON course_score_charts(chart_sha256);",
            "CREATE TABLE course_replays (
                course_score_id INTEGER NOT NULL
                    REFERENCES course_scores(id) ON DELETE CASCADE,
                position INTEGER NOT NULL,
                chart_sha256 TEXT NOT NULL,
                replay_path TEXT NOT NULL,
                PRIMARY KEY(course_score_id, position)
            );",
            "CREATE TABLE course_replay_slots (
                course_hash TEXT NOT NULL,
                slot INTEGER NOT NULL CHECK (slot BETWEEN 0 AND 3),
                rule TEXT NOT NULL,
                course_score_id INTEGER NOT NULL
                    REFERENCES course_scores(id) ON DELETE CASCADE,
                played_at INTEGER NOT NULL,
                ex_score INTEGER NOT NULL,
                bp INTEGER NOT NULL,
                max_combo INTEGER NOT NULL,
                clear_rank INTEGER NOT NULL,
                PRIMARY KEY(course_hash, slot)
            );",
            "CREATE INDEX idx_score_course_replay_slots_hash
                ON course_replay_slots(course_hash);",
            "CREATE TABLE course_trophy_achievements (
                course_score_id INTEGER NOT NULL
                    REFERENCES course_scores(id) ON DELETE CASCADE,
                course_hash TEXT NOT NULL,
                trophy_name TEXT NOT NULL,
                PRIMARY KEY(course_score_id, trophy_name)
            );",
            "CREATE INDEX idx_score_course_trophy_achievements_hash_name
                ON course_trophy_achievements(course_hash, trophy_name);",
        ],
    },
    Migration {
        version: 17,
        // IR/network retry state is profile-local network data, not score
        // history.  Fresh score.db files briefly create these legacy tables via
        // older migrations, then this migration removes them; existing rows are
        // intentionally not migrated.
        //
        // NOTE: v17 より前の score.db に未送信の IR ジョブ
        // (ir_score_jobs / ir_score_submissions) が残っていた場合、それらは
        // network.db へコピーされず、この DROP で失われる。現時点で旧バージョン
        // からの移行対象ユーザーがほぼ存在しないため、データ移行は意図的に
        // 実装しないと判断した (2026-07)。もし将来この判断を変える場合は、
        // この migration より前に score.db → network.db へのコピー処理を挟む
        // 新しい移行手順が必要になる。
        statements: &[
            "DROP TABLE IF EXISTS ir_score_submissions;",
            "DROP TABLE IF EXISTS ir_score_jobs;",
            "DROP TABLE IF EXISTS ir_accounts;",
        ],
    },
    Migration {
        version: 18,
        statements: &[
            "ALTER TABLE course_scores ADD COLUMN rule_mode TEXT NOT NULL DEFAULT 'Beatoraja';",
            "DROP INDEX IF EXISTS idx_score_course_scores_hash_played;",
            "DROP INDEX IF EXISTS idx_score_course_scores_hash_ex_score;",
            "CREATE INDEX idx_score_course_scores_hash_played
                ON course_scores(course_hash, rule_mode, played_at);",
            "CREATE INDEX idx_score_course_scores_hash_ex_score
                ON course_scores(course_hash, rule_mode, ex_score DESC);",
            "DROP INDEX IF EXISTS idx_score_course_replay_slots_hash;",
            "ALTER TABLE course_replay_slots RENAME TO course_replay_slots_old;",
            "CREATE TABLE course_replay_slots (
                course_hash TEXT NOT NULL,
                rule_mode TEXT NOT NULL DEFAULT 'Beatoraja',
                slot INTEGER NOT NULL CHECK (slot BETWEEN 0 AND 3),
                rule TEXT NOT NULL,
                course_score_id INTEGER NOT NULL
                    REFERENCES course_scores(id) ON DELETE CASCADE,
                played_at INTEGER NOT NULL,
                ex_score INTEGER NOT NULL,
                bp INTEGER NOT NULL,
                max_combo INTEGER NOT NULL,
                clear_rank INTEGER NOT NULL,
                PRIMARY KEY(course_hash, rule_mode, slot)
            );",
            "INSERT INTO course_replay_slots (
                course_hash, rule_mode, slot, rule, course_score_id, played_at,
                ex_score, bp, max_combo, clear_rank
            )
            SELECT
                course_hash, 'Beatoraja', slot, rule, course_score_id, played_at,
                ex_score, bp, max_combo, clear_rank
            FROM course_replay_slots_old;",
            "DROP TABLE course_replay_slots_old;",
            "CREATE INDEX idx_score_course_replay_slots_hash
                ON course_replay_slots(course_hash, rule_mode);",
        ],
    },
    Migration {
        version: 19,
        // Rebuild score_history so course_score_id can reference the
        // profile-local course_scores table, and stop storing per-history
        // ghosts.  score_best.ghost remains the source for pacemaker/MyBest
        // ghost playback.
        statements: &[
            "ALTER TABLE score_history RENAME TO score_history_old;",
            "CREATE TABLE score_history (
                id INTEGER PRIMARY KEY,
                chart_sha256 TEXT NOT NULL,
                ln_policy TEXT NOT NULL DEFAULT 'ForceLn',
                double_option TEXT NOT NULL DEFAULT 'Off',
                played_at INTEGER NOT NULL,
                clear_type TEXT NOT NULL,
                gauge_type TEXT NOT NULL,
                gauge_value REAL NOT NULL,
                total_notes INTEGER NOT NULL,
                ex_score INTEGER NOT NULL,
                bp INTEGER NOT NULL,
                cb INTEGER NOT NULL,
                max_combo INTEGER NOT NULL,
                fast_pgreat INTEGER NOT NULL,
                slow_pgreat INTEGER NOT NULL,
                fast_great INTEGER NOT NULL,
                slow_great INTEGER NOT NULL,
                fast_good INTEGER NOT NULL,
                slow_good INTEGER NOT NULL,
                fast_bad INTEGER NOT NULL,
                slow_bad INTEGER NOT NULL,
                fast_poor INTEGER NOT NULL,
                slow_poor INTEGER NOT NULL,
                fast_empty_poor INTEGER NOT NULL,
                slow_empty_poor INTEGER NOT NULL,
                random_seed INTEGER,
                arrange TEXT NOT NULL DEFAULT 'Normal',
                gauge_option TEXT NOT NULL,
                rule_mode TEXT NOT NULL DEFAULT 'Beatoraja',
                assist_mask INTEGER NOT NULL DEFAULT 0,
                autoplay INTEGER NOT NULL DEFAULT 0,
                device_type TEXT NOT NULL DEFAULT 'keyboard',
                replay_path TEXT NOT NULL,
                course_score_id INTEGER REFERENCES course_scores(id) ON DELETE SET NULL,
                old_clear_type TEXT,
                old_ex_score INTEGER,
                old_max_combo INTEGER,
                old_bp INTEGER,
                old_cb INTEGER
            );",
            "INSERT INTO score_history (
                id, chart_sha256, ln_policy, double_option, played_at,
                clear_type, gauge_type, gauge_value, total_notes, ex_score,
                bp, cb, max_combo, fast_pgreat, slow_pgreat, fast_great,
                slow_great, fast_good, slow_good, fast_bad, slow_bad,
                fast_poor, slow_poor, fast_empty_poor, slow_empty_poor,
                random_seed, arrange, gauge_option, rule_mode, assist_mask,
                autoplay, device_type, replay_path, course_score_id,
                old_clear_type, old_ex_score, old_max_combo, old_bp, old_cb
            )
            SELECT
                id, chart_sha256, ln_policy, double_option, played_at,
                clear_type, gauge_type, gauge_value, total_notes, ex_score,
                bp, cb, max_combo, fast_pgreat, slow_pgreat, fast_great,
                slow_great, fast_good, slow_good, fast_bad, slow_bad,
                fast_poor, slow_poor, fast_empty_poor, slow_empty_poor,
                random_seed, arrange, gauge_option, rule_mode, assist_mask,
                autoplay, device_type, replay_path,
                CASE
                    WHEN course_score_id IS NOT NULL
                     AND EXISTS (
                        SELECT 1 FROM course_scores
                        WHERE course_scores.id = score_history_old.course_score_id
                     )
                    THEN course_score_id
                    ELSE NULL
                END,
                old_clear_type, old_ex_score, old_max_combo, old_bp, old_cb
            FROM score_history_old;",
            "DROP TABLE score_history_old;",
            "CREATE INDEX idx_score_history_chart_sha256 ON score_history(chart_sha256);",
            "CREATE INDEX idx_score_history_played_at ON score_history(played_at DESC);",
            "CREATE INDEX idx_score_history_course_score_id
                ON score_history(course_score_id)
                WHERE course_score_id IS NOT NULL;",
            "DROP INDEX IF EXISTS idx_score_best_chart;",
            "DROP INDEX IF EXISTS idx_replay_slots_chart;",
            "DROP INDEX IF EXISTS idx_score_course_replay_slots_hash;",
        ],
    },
    Migration {
        version: 20,
        // Imported scores need durable provenance so repeated imports can be
        // deduplicated without treating a local play as the same source row.
        // Historical rows predate provenance tracking and remain local scores.
        statements: &[
            "ALTER TABLE score_history
                ADD COLUMN source_kind TEXT NOT NULL DEFAULT 'Local';",
            "ALTER TABLE score_history
                ADD COLUMN arrange_2p TEXT NOT NULL DEFAULT 'Normal';",
            "CREATE INDEX idx_score_history_source_kind_chart_sha256
                ON score_history(source_kind, chart_sha256);",
        ],
    },
    Migration {
        version: 21,
        // `double_option` remains the score aggregation bucket.  Keep the
        // actually applied option separately because FLIP shares the Off
        // bucket but must remain visible in score history.
        statements: &[
            "ALTER TABLE score_history
                ADD COLUMN applied_double_option TEXT NOT NULL DEFAULT 'Off';",
        ],
    },
    Migration {
        version: 22,
        // score_best はスコア側の各列を保持する履歴行を明示的に参照する。
        // これにより外部score DBの自己申告デバイスを訂正しても、同値の
        // ローカルベストを誤って更新しない。
        statements: &[
            "ALTER TABLE score_best ADD COLUMN best_score_history_id INTEGER;",
            "UPDATE score_best
             SET best_score_history_id = (
                SELECT score_history.id
                FROM score_history
                WHERE score_history.chart_sha256 = score_best.chart_sha256
                  AND score_history.ln_policy = score_best.ln_policy
                  AND score_history.double_option = score_best.double_option
                  AND score_history.rule_mode = score_best.rule_mode
                  AND score_history.ex_score = score_best.ex_score
                  AND score_history.fast_pgreat = score_best.fast_pgreat
                  AND score_history.slow_pgreat = score_best.slow_pgreat
                  AND score_history.fast_great = score_best.fast_great
                  AND score_history.slow_great = score_best.slow_great
                  AND score_history.fast_good = score_best.fast_good
                  AND score_history.slow_good = score_best.slow_good
                  AND score_history.fast_bad = score_best.fast_bad
                  AND score_history.slow_bad = score_best.slow_bad
                  AND score_history.fast_poor = score_best.fast_poor
                  AND score_history.slow_poor = score_best.slow_poor
                  AND score_history.fast_empty_poor = score_best.fast_empty_poor
                  AND score_history.slow_empty_poor = score_best.slow_empty_poor
                  AND score_history.played_at = score_best.played_at
                  AND score_history.replay_path = score_best.replay_path
                  AND score_history.device_type = score_best.device_type
                ORDER BY score_history.id ASC
                LIMIT 1
             );",
            "CREATE INDEX idx_score_best_best_score_history_id
                ON score_best(best_score_history_id)
                WHERE best_score_history_id IS NOT NULL;",
        ],
    },
    Migration {
        version: 23,
        statements: &[
            "CREATE TABLE daily_statistics_state (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                reset_at INTEGER NOT NULL DEFAULT 0
            );",
            "INSERT INTO daily_statistics_state (id, reset_at) VALUES (1, 0);",
        ],
    },
    Migration {
        version: 24,
        // Existing BMZ local rows used one unrestricted seed for both arrange
        // and BMS #RANDOM. Keep that meaning explicit instead of reinterpreting
        // the number. Imported beatoraja rows already use the packed 24-bit
        // side format and can be labelled accordingly.
        statements: &[
            "ALTER TABLE score_history
                ADD COLUMN seed_scheme TEXT NOT NULL DEFAULT 'legacy_shared_v3';",
            "UPDATE score_history
             SET seed_scheme = 'beatoraja_24bit_v1'
             WHERE source_kind = 'Beatoraja';",
        ],
    },
    Migration {
        version: 25,
        statements: &[
            "CREATE TABLE IF NOT EXISTS score_history_sources (
                id INTEGER PRIMARY KEY,
                score_history_id INTEGER NOT NULL
                    REFERENCES score_history(id) ON DELETE CASCADE,
                source TEXT NOT NULL,
                provider TEXT NOT NULL,
                account_id TEXT NOT NULL,
                remote_score_id TEXT NOT NULL,
                verification TEXT NOT NULL DEFAULT '',
                server_received_at INTEGER NOT NULL DEFAULT 0,
                imported_at INTEGER NOT NULL,
                UNIQUE(source, provider, account_id, remote_score_id)
            );",
            "CREATE INDEX IF NOT EXISTS idx_score_history_sources_history
                ON score_history_sources(score_history_id);",
        ],
    },
    Migration {
        version: 26,
        // Some imported score histories do not include their final gauge value.
        // Rebuild the two score tables so NULL can represent that absence
        // without inventing a value from the clear lamp.
        statements: &[
            "CREATE TEMP TABLE score_history_sources_backup AS
                SELECT id, score_history_id, source, provider, account_id,
                       remote_score_id, verification, server_received_at, imported_at
                FROM score_history_sources;",
            "DROP TABLE score_history_sources;",
            "ALTER TABLE score_history RENAME TO score_history_old;",
            "CREATE TABLE score_history (
                id INTEGER PRIMARY KEY,
                chart_sha256 TEXT NOT NULL,
                ln_policy TEXT NOT NULL DEFAULT 'ForceLn',
                double_option TEXT NOT NULL DEFAULT 'Off',
                played_at INTEGER NOT NULL,
                clear_type TEXT NOT NULL,
                gauge_type TEXT NOT NULL,
                gauge_value REAL,
                total_notes INTEGER NOT NULL,
                ex_score INTEGER NOT NULL,
                bp INTEGER NOT NULL,
                cb INTEGER NOT NULL,
                max_combo INTEGER NOT NULL,
                fast_pgreat INTEGER NOT NULL,
                slow_pgreat INTEGER NOT NULL,
                fast_great INTEGER NOT NULL,
                slow_great INTEGER NOT NULL,
                fast_good INTEGER NOT NULL,
                slow_good INTEGER NOT NULL,
                fast_bad INTEGER NOT NULL,
                slow_bad INTEGER NOT NULL,
                fast_poor INTEGER NOT NULL,
                slow_poor INTEGER NOT NULL,
                fast_empty_poor INTEGER NOT NULL,
                slow_empty_poor INTEGER NOT NULL,
                random_seed INTEGER,
                arrange TEXT NOT NULL DEFAULT 'Normal',
                gauge_option TEXT NOT NULL,
                rule_mode TEXT NOT NULL DEFAULT 'Beatoraja',
                assist_mask INTEGER NOT NULL DEFAULT 0,
                autoplay INTEGER NOT NULL DEFAULT 0,
                device_type TEXT NOT NULL DEFAULT 'keyboard',
                replay_path TEXT NOT NULL,
                course_score_id INTEGER REFERENCES course_scores(id) ON DELETE SET NULL,
                old_clear_type TEXT,
                old_ex_score INTEGER,
                old_max_combo INTEGER,
                old_bp INTEGER,
                old_cb INTEGER,
                source_kind TEXT NOT NULL DEFAULT 'Local',
                arrange_2p TEXT NOT NULL DEFAULT 'Normal',
                applied_double_option TEXT NOT NULL DEFAULT 'Off',
                seed_scheme TEXT NOT NULL DEFAULT 'legacy_shared_v3'
            );",
            "INSERT INTO score_history (
                id, chart_sha256, ln_policy, double_option, played_at,
                clear_type, gauge_type, gauge_value, total_notes, ex_score,
                bp, cb, max_combo, fast_pgreat, slow_pgreat, fast_great,
                slow_great, fast_good, slow_good, fast_bad, slow_bad,
                fast_poor, slow_poor, fast_empty_poor, slow_empty_poor,
                random_seed, arrange, gauge_option, rule_mode, assist_mask,
                autoplay, device_type, replay_path, course_score_id,
                old_clear_type, old_ex_score, old_max_combo, old_bp, old_cb,
                source_kind, arrange_2p, applied_double_option, seed_scheme
            )
            SELECT
                id, chart_sha256, ln_policy, double_option, played_at,
                clear_type, gauge_type, gauge_value, total_notes, ex_score,
                bp, cb, max_combo, fast_pgreat, slow_pgreat, fast_great,
                slow_great, fast_good, slow_good, fast_bad, slow_bad,
                fast_poor, slow_poor, fast_empty_poor, slow_empty_poor,
                random_seed, arrange, gauge_option, rule_mode, assist_mask,
                autoplay, device_type, replay_path, course_score_id,
                old_clear_type, old_ex_score, old_max_combo, old_bp, old_cb,
                source_kind, arrange_2p, applied_double_option, seed_scheme
            FROM score_history_old;",
            "DROP TABLE score_history_old;",
            "CREATE INDEX idx_score_history_chart_sha256 ON score_history(chart_sha256);",
            "CREATE INDEX idx_score_history_played_at ON score_history(played_at DESC);",
            "CREATE INDEX idx_score_history_course_score_id
                ON score_history(course_score_id)
                WHERE course_score_id IS NOT NULL;",
            "CREATE INDEX idx_score_history_source_kind_chart_sha256
                ON score_history(source_kind, chart_sha256);",
            "CREATE TABLE score_history_sources (
                id INTEGER PRIMARY KEY,
                score_history_id INTEGER NOT NULL
                    REFERENCES score_history(id) ON DELETE CASCADE,
                source TEXT NOT NULL,
                provider TEXT NOT NULL,
                account_id TEXT NOT NULL,
                remote_score_id TEXT NOT NULL,
                verification TEXT NOT NULL DEFAULT '',
                server_received_at INTEGER NOT NULL DEFAULT 0,
                imported_at INTEGER NOT NULL,
                UNIQUE(source, provider, account_id, remote_score_id)
            );",
            "INSERT INTO score_history_sources (
                id, score_history_id, source, provider, account_id,
                remote_score_id, verification, server_received_at, imported_at
            )
            SELECT
                id, score_history_id, source, provider, account_id,
                remote_score_id, verification, server_received_at, imported_at
            FROM score_history_sources_backup;",
            "DROP TABLE score_history_sources_backup;",
            "CREATE INDEX idx_score_history_sources_history
                ON score_history_sources(score_history_id);",
            "ALTER TABLE score_best RENAME TO score_best_old;",
            "CREATE TABLE score_best (
                chart_sha256 TEXT NOT NULL,
                ln_policy TEXT NOT NULL,
                double_option TEXT NOT NULL DEFAULT 'Off',
                rule_mode TEXT NOT NULL DEFAULT 'Beatoraja',
                clear_type TEXT NOT NULL,
                gauge_type TEXT NOT NULL,
                gauge_value REAL,
                ex_score INTEGER NOT NULL,
                bp INTEGER NOT NULL,
                cb INTEGER NOT NULL,
                max_combo INTEGER NOT NULL,
                fast_pgreat INTEGER NOT NULL,
                slow_pgreat INTEGER NOT NULL,
                fast_great INTEGER NOT NULL,
                slow_great INTEGER NOT NULL,
                fast_good INTEGER NOT NULL,
                slow_good INTEGER NOT NULL,
                fast_bad INTEGER NOT NULL,
                slow_bad INTEGER NOT NULL,
                fast_poor INTEGER NOT NULL,
                slow_poor INTEGER NOT NULL,
                fast_empty_poor INTEGER NOT NULL,
                slow_empty_poor INTEGER NOT NULL,
                played_at INTEGER NOT NULL,
                replay_path TEXT NOT NULL,
                ghost TEXT NOT NULL DEFAULT '',
                play_count INTEGER NOT NULL DEFAULT 0,
                clear_count INTEGER NOT NULL DEFAULT 0,
                device_type TEXT NOT NULL DEFAULT 'keyboard',
                best_score_history_id INTEGER,
                PRIMARY KEY(chart_sha256, ln_policy, double_option, rule_mode)
            );",
            "INSERT INTO score_best (
                chart_sha256, ln_policy, double_option, rule_mode,
                clear_type, gauge_type, gauge_value, ex_score, bp, cb,
                max_combo, fast_pgreat, slow_pgreat, fast_great, slow_great,
                fast_good, slow_good, fast_bad, slow_bad, fast_poor,
                slow_poor, fast_empty_poor, slow_empty_poor, played_at,
                replay_path, ghost, play_count, clear_count, device_type,
                best_score_history_id
            )
            SELECT
                chart_sha256, ln_policy, double_option, rule_mode,
                clear_type, gauge_type, gauge_value, ex_score, bp, cb,
                max_combo, fast_pgreat, slow_pgreat, fast_great, slow_great,
                fast_good, slow_good, fast_bad, slow_bad, fast_poor,
                slow_poor, fast_empty_poor, slow_empty_poor, played_at,
                replay_path, ghost, play_count, clear_count, device_type,
                best_score_history_id
            FROM score_best_old;",
            "DROP TABLE score_best_old;",
            "CREATE INDEX idx_score_best_clear_type ON score_best(clear_type);",
            "CREATE INDEX idx_score_best_ex_score ON score_best(ex_score DESC);",
            "CREATE INDEX idx_score_best_best_score_history_id
                ON score_best(best_score_history_id)
                WHERE best_score_history_id IS NOT NULL;",
        ],
    },
    Migration {
        version: 27,
        // Course scores use one normalized LN policy derived from the merged
        // profiles of all course charts. Historical rows predate that context
        // and remain in the ForceLn compatibility bucket.
        statements: &[
            "ALTER TABLE course_scores
                ADD COLUMN ln_policy TEXT NOT NULL DEFAULT 'ForceLn';",
            "DROP INDEX IF EXISTS idx_score_course_scores_hash_played;",
            "DROP INDEX IF EXISTS idx_score_course_scores_hash_ex_score;",
            "CREATE INDEX idx_score_course_scores_hash_played
                ON course_scores(course_hash, ln_policy, rule_mode, played_at);",
            "CREATE INDEX idx_score_course_scores_hash_ex_score
                ON course_scores(course_hash, ln_policy, rule_mode, ex_score DESC);",
            "ALTER TABLE course_replay_slots RENAME TO course_replay_slots_old;",
            "CREATE TABLE course_replay_slots (
                course_hash TEXT NOT NULL,
                ln_policy TEXT NOT NULL DEFAULT 'ForceLn',
                rule_mode TEXT NOT NULL DEFAULT 'Beatoraja',
                slot INTEGER NOT NULL CHECK (slot BETWEEN 0 AND 3),
                rule TEXT NOT NULL,
                course_score_id INTEGER NOT NULL
                    REFERENCES course_scores(id) ON DELETE CASCADE,
                played_at INTEGER NOT NULL,
                ex_score INTEGER NOT NULL,
                bp INTEGER NOT NULL,
                max_combo INTEGER NOT NULL,
                clear_rank INTEGER NOT NULL,
                PRIMARY KEY(course_hash, ln_policy, rule_mode, slot)
            );",
            "INSERT INTO course_replay_slots (
                course_hash, ln_policy, rule_mode, slot, rule, course_score_id,
                played_at, ex_score, bp, max_combo, clear_rank
            )
            SELECT
                old.course_hash, scores.ln_policy, old.rule_mode, old.slot,
                old.rule, old.course_score_id, old.played_at, old.ex_score,
                old.bp, old.max_combo, old.clear_rank
            FROM course_replay_slots_old old
            JOIN course_scores scores ON scores.id = old.course_score_id;",
            "DROP TABLE course_replay_slots_old;",
        ],
    },
    Migration {
        version: 28,
        // A beatoraja `.brd` contains playback data but no result summary.
        // Keep those metrics nullable and retain provenance so imported slots
        // never masquerade as a zero-score local play.
        statements: &[
            "ALTER TABLE replay_slots RENAME TO replay_slots_old;",
            "CREATE TABLE replay_slots (
                chart_sha256 TEXT NOT NULL,
                ln_policy TEXT NOT NULL,
                double_option TEXT NOT NULL DEFAULT 'Off',
                rule_mode TEXT NOT NULL DEFAULT 'Beatoraja',
                slot INTEGER NOT NULL CHECK (slot BETWEEN 0 AND 3),
                rule TEXT NOT NULL,
                replay_path TEXT NOT NULL,
                played_at INTEGER NOT NULL,
                ex_score INTEGER,
                bp INTEGER,
                cb INTEGER,
                max_combo INTEGER,
                clear_rank INTEGER,
                source_kind TEXT NOT NULL DEFAULT 'Local',
                source_path TEXT NOT NULL DEFAULT '',
                PRIMARY KEY(chart_sha256, ln_policy, double_option, rule_mode, slot)
            );",
            "INSERT INTO replay_slots (
                chart_sha256, ln_policy, double_option, rule_mode, slot, rule,
                replay_path, played_at, ex_score, bp, cb, max_combo, clear_rank,
                source_kind, source_path
            )
            SELECT
                chart_sha256, ln_policy, double_option, rule_mode, slot, rule,
                replay_path, played_at, ex_score, bp, cb, max_combo, clear_rank,
                'Local', ''
            FROM replay_slots_old;",
            "DROP TABLE replay_slots_old;",
        ],
    },
    Migration {
        version: 29,
        // Bulk replay import uses the compressed source hash to skip files
        // that were already converted into the same replay slot.
        statements: &["ALTER TABLE replay_slots
                ADD COLUMN source_fingerprint TEXT NOT NULL DEFAULT '';"],
    },
    Migration {
        version: 30,
        // Imported course `.brd` files contain playback data but no score
        // summary.  Keep a replay-only attempt behind the existing course
        // replay-slot foreign key, while excluding it from score history and
        // best-score queries.
        statements: &[
            "ALTER TABLE course_scores
                ADD COLUMN replay_only INTEGER NOT NULL DEFAULT 0;",
            "ALTER TABLE course_scores
                ADD COLUMN replay_source_kind TEXT NOT NULL DEFAULT 'Local';",
            "ALTER TABLE course_scores
                ADD COLUMN replay_source_path TEXT NOT NULL DEFAULT '';",
            "ALTER TABLE course_scores
                ADD COLUMN replay_source_fingerprint TEXT NOT NULL DEFAULT '';",
        ],
    },
    Migration {
        version: 31,
        // Normal attempts remain derived from history so deletion/reconciliation
        // cannot leave a stale timestamp. Only clear-only attempts need storage.
        statements: &[
            "CREATE TABLE score_unrecorded_plays (
                chart_sha256 TEXT NOT NULL,
                ln_policy TEXT NOT NULL,
                double_option TEXT NOT NULL,
                rule_mode TEXT NOT NULL,
                played_at INTEGER NOT NULL,
                PRIMARY KEY(chart_sha256, ln_policy, double_option, rule_mode)
            );",
            "INSERT INTO score_unrecorded_plays
             SELECT chart_sha256, ln_policy, double_option, rule_mode, played_at
             FROM score_best WHERE best_score_history_id IS NULL AND played_at > 0;",
            "CREATE INDEX idx_score_history_last_play
             ON score_history(chart_sha256, ln_policy, double_option, rule_mode, played_at)
             WHERE autoplay = 0;",
        ],
    },
];
