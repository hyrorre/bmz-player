use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;

use super::common::configure_connection;

pub struct Migration {
    pub version: i32,
    pub statements: &'static [&'static str],
}

pub fn migrate_library_db(path: &Path) -> Result<()> {
    let mut conn = Connection::open(path)?;
    configure_connection(&conn)?;
    run_migrations(&mut conn, LIBRARY_MIGRATIONS)
}

pub fn migrate_score_db(path: &Path) -> Result<()> {
    let mut conn = Connection::open(path)?;
    configure_connection(&conn)?;
    run_migrations(&mut conn, SCORE_MIGRATIONS)
}

pub fn run_migrations(conn: &mut Connection, migrations: &[Migration]) -> Result<()> {
    let current_version: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;

    for migration in migrations {
        if migration.version > current_version {
            let tx = conn.transaction()?;
            for stmt in migration.statements {
                tx.execute_batch(stmt)?;
            }
            tx.pragma_update(None, "user_version", migration.version)?;
            tx.commit()?;
        }
    }

    Ok(())
}

pub const LIBRARY_MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        statements: &[
            "CREATE TABLE roots (
            id INTEGER PRIMARY KEY,
            path TEXT NOT NULL UNIQUE,
            enabled INTEGER NOT NULL DEFAULT 1,
            recursive INTEGER NOT NULL DEFAULT 1,
            last_scan_at INTEGER
        );",
            "CREATE TABLE chart_files (
            id INTEGER PRIMARY KEY,
            root_id INTEGER,
            path TEXT NOT NULL UNIQUE,
            file_size INTEGER NOT NULL,
            modified_at INTEGER NOT NULL,
            md5 TEXT NOT NULL,
            sha256 TEXT NOT NULL,
            scanned_at INTEGER NOT NULL,
            parse_status TEXT NOT NULL,
            FOREIGN KEY(root_id) REFERENCES roots(id)
        );",
            "CREATE TABLE charts (
            id INTEGER PRIMARY KEY,
            sha256 TEXT NOT NULL UNIQUE,
            md5 TEXT NOT NULL,
            title TEXT NOT NULL,
            subtitle TEXT NOT NULL,
            artist TEXT NOT NULL,
            subartist TEXT NOT NULL,
            genre TEXT NOT NULL,
            difficulty_name TEXT NOT NULL,
            play_level TEXT NOT NULL,
            mode TEXT NOT NULL,
            total_notes INTEGER NOT NULL,
            initial_bpm REAL NOT NULL,
            min_bpm REAL,
            max_bpm REAL,
            length_ms INTEGER,
            ln_type TEXT NOT NULL,
            has_bga INTEGER NOT NULL DEFAULT 0,
            has_long_notes INTEGER NOT NULL DEFAULT 0,
            has_mines INTEGER NOT NULL DEFAULT 0,
            folder_path TEXT NOT NULL,
            stage_file TEXT NOT NULL,
            preview_file TEXT NOT NULL,
            import_version INTEGER NOT NULL
        );",
            "CREATE TABLE chart_file_links (
            chart_id INTEGER NOT NULL,
            chart_file_id INTEGER NOT NULL,
            PRIMARY KEY(chart_id, chart_file_id),
            FOREIGN KEY(chart_id) REFERENCES charts(id),
            FOREIGN KEY(chart_file_id) REFERENCES chart_files(id)
        );",
            "CREATE TABLE chart_import_warnings (
            id INTEGER PRIMARY KEY,
            chart_file_id INTEGER NOT NULL,
            code TEXT NOT NULL,
            message TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            FOREIGN KEY(chart_file_id) REFERENCES chart_files(id)
        );",
            "CREATE INDEX idx_chart_files_sha256 ON chart_files(sha256);",
            "CREATE INDEX idx_chart_files_root_id ON chart_files(root_id);",
            "CREATE INDEX idx_charts_title ON charts(title);",
            "CREATE INDEX idx_charts_artist ON charts(artist);",
            "CREATE INDEX idx_charts_folder_path ON charts(folder_path);",
            "CREATE INDEX idx_charts_mode ON charts(mode);",
        ],
    },
    Migration {
        version: 2,
        statements: &[
            // Recreate charts without UNIQUE(sha256) and chart_file_links with UNIQUE(chart_file_id).
            // Both tables are renamed first, then recreated, so FK constraints on the new tables
            // are satisfied when data is copied (charts populated before chart_file_links).
            "ALTER TABLE charts RENAME TO charts_old;",
            "ALTER TABLE chart_file_links RENAME TO chart_file_links_old;",
            "CREATE TABLE charts (
            id INTEGER PRIMARY KEY,
            sha256 TEXT NOT NULL,
            md5 TEXT NOT NULL,
            title TEXT NOT NULL,
            subtitle TEXT NOT NULL,
            artist TEXT NOT NULL,
            subartist TEXT NOT NULL,
            genre TEXT NOT NULL,
            difficulty_name TEXT NOT NULL,
            play_level TEXT NOT NULL,
            mode TEXT NOT NULL,
            total_notes INTEGER NOT NULL,
            initial_bpm REAL NOT NULL,
            min_bpm REAL,
            max_bpm REAL,
            length_ms INTEGER,
            ln_type TEXT NOT NULL,
            has_bga INTEGER NOT NULL DEFAULT 0,
            has_long_notes INTEGER NOT NULL DEFAULT 0,
            has_mines INTEGER NOT NULL DEFAULT 0,
            folder_path TEXT NOT NULL,
            stage_file TEXT NOT NULL,
            preview_file TEXT NOT NULL,
            import_version INTEGER NOT NULL
        );",
            "CREATE TABLE chart_file_links (
            chart_id INTEGER NOT NULL,
            chart_file_id INTEGER NOT NULL UNIQUE,
            PRIMARY KEY(chart_id, chart_file_id),
            FOREIGN KEY(chart_id) REFERENCES charts(id),
            FOREIGN KEY(chart_file_id) REFERENCES chart_files(id)
        );",
            "INSERT INTO charts SELECT * FROM charts_old;",
            "INSERT INTO chart_file_links SELECT * FROM chart_file_links_old;",
            "DROP TABLE chart_file_links_old;",
            "DROP TABLE charts_old;",
            "CREATE INDEX idx_charts_title ON charts(title);",
            "CREATE INDEX idx_charts_artist ON charts(artist);",
            "CREATE INDEX idx_charts_folder_path ON charts(folder_path);",
            "CREATE INDEX idx_charts_mode ON charts(mode);",
            "CREATE INDEX idx_charts_md5 ON charts(md5);",
            "CREATE INDEX idx_charts_sha256 ON charts(sha256);",
        ],
    },
    Migration {
        version: 3,
        statements: &[
            "CREATE TABLE difficulty_tables (
                id INTEGER PRIMARY KEY,
                source_url TEXT NOT NULL UNIQUE,
                head_url TEXT NOT NULL,
                name TEXT NOT NULL,
                symbol TEXT NOT NULL,
                level_order TEXT NOT NULL,
                fetched_at INTEGER NOT NULL
            );",
            "CREATE TABLE difficulty_table_entries (
                id INTEGER PRIMARY KEY,
                table_id INTEGER NOT NULL REFERENCES difficulty_tables(id) ON DELETE CASCADE,
                level TEXT NOT NULL,
                md5 TEXT NOT NULL,
                sha256 TEXT NOT NULL,
                title TEXT NOT NULL,
                artist TEXT NOT NULL,
                comment TEXT NOT NULL
            );",
            "CREATE INDEX idx_dte_table_id ON difficulty_table_entries(table_id);",
            "CREATE INDEX idx_dte_md5 ON difficulty_table_entries(md5);",
            "CREATE INDEX idx_dte_sha256 ON difficulty_table_entries(sha256);",
        ],
    },
    Migration {
        version: 4,
        // chart_import_warnings は警告書き込みのたびに
        // `DELETE ... WHERE chart_file_id = ?` を発行する。インデックスが無いと
        // 毎回テーブル全走査になり、warnings テーブルの肥大とともにスキャンが極端に遅くなる。
        statements: &["CREATE INDEX idx_chart_import_warnings_chart_file_id
             ON chart_import_warnings(chart_file_id);"],
    },
    Migration {
        version: 5,
        // folder_path はスラッシュ `/` を正準とする。Windows で取り込まれた既存行は
        // バックスラッシュ区切りのため、選曲画面のフォルダ走査クエリと一致しない。
        // 既存行のバックスラッシュをスラッシュに正規化する。
        statements: &["UPDATE charts SET folder_path = REPLACE(folder_path, '\\', '/');"],
    },
    Migration {
        version: 6,
        statements: &[
            "CREATE TABLE courses (
                id INTEGER PRIMARY KEY,
                source TEXT NOT NULL,
                course_key TEXT NOT NULL,
                title TEXT NOT NULL,
                kind TEXT NOT NULL,
                class_constraint TEXT NOT NULL,
                speed_constraint TEXT NOT NULL,
                judge_constraint TEXT NOT NULL,
                gauge_constraint TEXT NOT NULL,
                ln_constraint TEXT NOT NULL,
                source_constraints TEXT NOT NULL,
                trophies_json TEXT NOT NULL,
                release INTEGER NOT NULL DEFAULT 1,
                imported_at INTEGER NOT NULL,
                UNIQUE(source, course_key)
            );",
            "CREATE TABLE course_entries (
                course_id INTEGER NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
                position INTEGER NOT NULL,
                md5 TEXT NOT NULL,
                sha256 TEXT NOT NULL,
                title_hint TEXT NOT NULL,
                chart_id INTEGER REFERENCES charts(id),
                PRIMARY KEY(course_id, position)
            );",
            "CREATE INDEX idx_courses_source ON courses(source);",
            "CREATE INDEX idx_courses_kind ON courses(kind);",
            "CREATE INDEX idx_course_entries_chart_id ON course_entries(chart_id);",
            "CREATE INDEX idx_course_entries_md5 ON course_entries(md5);",
            "CREATE INDEX idx_course_entries_sha256 ON course_entries(sha256);",
        ],
    },
    Migration {
        version: 7,
        statements: &[
            "ALTER TABLE charts ADD COLUMN banner_file TEXT NOT NULL DEFAULT '';",
            "ALTER TABLE charts ADD COLUMN backbmp_file TEXT NOT NULL DEFAULT '';",
            "ALTER TABLE charts ADD COLUMN judge_rank INTEGER;",
            "ALTER TABLE charts ADD COLUMN gauge_total REAL;",
        ],
    },
    Migration {
        version: 8,
        // Course list order should follow the difficulty table's JSON ordering
        // (the order specified by the table author), not alphabetical by title.
        // `source_position` is the index of the course within its source array.
        statements: &[
            "ALTER TABLE courses ADD COLUMN source_position INTEGER NOT NULL DEFAULT 0;",
            "CREATE INDEX idx_courses_source_position ON courses(source, source_position);",
        ],
    },
    Migration {
        version: 9,
        // Persist aggregated course play results plus their per-chart breakdown.
        // Course scores live alongside the `courses` table because the FK to
        // courses(id) cannot cross databases.
        statements: &[
            "CREATE TABLE course_scores (
                id INTEGER PRIMARY KEY,
                course_id INTEGER NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
                ex_score INTEGER NOT NULL,
                max_ex_score INTEGER NOT NULL,
                clear_type TEXT NOT NULL,
                gauge_type TEXT NOT NULL,
                gauge_value REAL NOT NULL,
                max_combo INTEGER NOT NULL,
                bp INTEGER NOT NULL,
                course_failed INTEGER NOT NULL,
                course_clear INTEGER NOT NULL,
                trophies_json TEXT NOT NULL,
                played_at INTEGER NOT NULL
            );",
            "CREATE INDEX idx_course_scores_course ON course_scores(course_id, played_at);",
            "CREATE INDEX idx_course_scores_course_ex_score
                ON course_scores(course_id, ex_score DESC);",
            "CREATE TABLE course_score_charts (
                course_score_id INTEGER NOT NULL
                    REFERENCES course_scores(id) ON DELETE CASCADE,
                position INTEGER NOT NULL,
                chart_id INTEGER NOT NULL,
                ex_score INTEGER NOT NULL,
                max_combo INTEGER NOT NULL,
                clear_type TEXT NOT NULL,
                gauge_value REAL NOT NULL,
                PRIMARY KEY(course_score_id, position)
            );",
            "CREATE INDEX idx_course_score_charts_chart ON course_score_charts(chart_id);",
        ],
    },
    Migration {
        version: 10,
        // Per-chart replay file paths for a course attempt.  Replay file format
        // is identical to per-chart replays; only the storage table is new so
        // that the whole sequence can be replayed back to back later.
        statements: &["CREATE TABLE course_replays (
                course_score_id INTEGER NOT NULL
                    REFERENCES course_scores(id) ON DELETE CASCADE,
                position INTEGER NOT NULL,
                chart_id INTEGER NOT NULL,
                replay_path TEXT NOT NULL,
                PRIMARY KEY(course_score_id, position)
            );"],
    },
    Migration {
        version: 11,
        // Course-level replay slots, mirroring the per-chart `replay_slots`
        // shape in `score.db`.  Slots are addressed by (course_id, slot)
        // and point at a course_scores row whose aggregate metrics passed
        // the slot's rule (Always / ScoreUpdate / BpUpdate /
        // MaxComboUpdate / ClearUpdate).
        statements: &[
            "CREATE TABLE course_replay_slots (
                course_id INTEGER NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
                slot INTEGER NOT NULL CHECK (slot BETWEEN 0 AND 3),
                rule TEXT NOT NULL,
                course_score_id INTEGER NOT NULL
                    REFERENCES course_scores(id) ON DELETE CASCADE,
                played_at INTEGER NOT NULL,
                ex_score INTEGER NOT NULL,
                bp INTEGER NOT NULL,
                max_combo INTEGER NOT NULL,
                clear_rank INTEGER NOT NULL,
                PRIMARY KEY(course_id, slot)
            );",
            "CREATE INDEX idx_course_replay_slots_course
                ON course_replay_slots(course_id);",
        ],
    },
    Migration {
        version: 12,
        // Per-attempt trophy achievements, denormalized for indexed queries.
        // `course_scores.trophies_json` still stores the JSON list as-is for
        // round-trip/audit purposes; this table makes \"which trophies were
        // ever achieved\" and \"best score that achieved trophy X\" cheap.
        //
        // PRIMARY KEY ensures each attempt contributes at most one row per
        // trophy name.  CASCADE fires when either the course or the attempt
        // is deleted.
        statements: &[
            "CREATE TABLE course_trophy_achievements (
                course_score_id INTEGER NOT NULL
                    REFERENCES course_scores(id) ON DELETE CASCADE,
                course_id INTEGER NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
                trophy_name TEXT NOT NULL,
                PRIMARY KEY(course_score_id, trophy_name)
            );",
            "CREATE INDEX idx_course_trophy_achievements_course_name
                ON course_trophy_achievements(course_id, trophy_name);",
        ],
    },
    Migration {
        version: 13,
        // beatoraja GradeBar keeps separate normal / mirror / random course
        // scores.  Persist the arrange used for each course attempt so select
        // trophies can be derived from the same three buckets.
        statements: &[
            "ALTER TABLE course_scores ADD COLUMN arrange TEXT NOT NULL DEFAULT 'Normal';",
            "CREATE INDEX idx_course_scores_course_arrange
                ON course_scores(course_id, arrange);",
        ],
    },
    Migration {
        version: 14,
        // beatoraja keeps per-chart SongInformation in a separate information
        // table.  BMZ stores the same scan-time analysis beside charts, keyed
        // by chart_id because library.db can intentionally keep multiple chart
        // rows with the same sha256 at different paths.
        statements: &[
            "CREATE TABLE chart_analysis (
                chart_id INTEGER PRIMARY KEY REFERENCES charts(id) ON DELETE CASCADE,
                normal_notes INTEGER NOT NULL,
                long_notes INTEGER NOT NULL,
                scratch_notes INTEGER NOT NULL,
                long_scratch_notes INTEGER NOT NULL,
                density REAL NOT NULL,
                peak_density REAL NOT NULL,
                end_density REAL NOT NULL,
                total_gauge REAL NOT NULL,
                main_bpm REAL NOT NULL,
                distribution_json TEXT NOT NULL,
                speed_changes_json TEXT NOT NULL,
                lane_notes_json TEXT NOT NULL,
                analysis_version INTEGER NOT NULL
            );",
            "CREATE INDEX idx_chart_analysis_main_bpm ON chart_analysis(main_bpm);",
        ],
    },
    Migration {
        version: 15,
        // Store the long-note makeup needed to normalize BMZ score policies.
        // Existing rows default to no long notes; rescanning charts refreshes
        // the four flags from the parsed chart model.
        statements: &[
            "ALTER TABLE charts ADD COLUMN has_undefined_ln INTEGER NOT NULL DEFAULT 0;",
            "ALTER TABLE charts ADD COLUMN has_defined_ln INTEGER NOT NULL DEFAULT 0;",
            "ALTER TABLE charts ADD COLUMN has_defined_cn INTEGER NOT NULL DEFAULT 0;",
            "ALTER TABLE charts ADD COLUMN has_defined_hcn INTEGER NOT NULL DEFAULT 0;",
        ],
    },
    Migration {
        version: 16,
        // Raw BMS #TOTAL for beatoraja skin ref 368 (chart_totalgauge).
        // Distinct from gauge_total, which applies the gameplay default formula.
        statements: &["ALTER TABLE charts ADD COLUMN bms_total REAL NOT NULL DEFAULT 0;"],
    },
    Migration {
        version: 17,
        // Source BMS defines `#RANDOM` sections (beatoraja `hasRandomSequence`).
        statements: &["ALTER TABLE charts ADD COLUMN has_bms_random INTEGER NOT NULL DEFAULT 0;"],
    },
    Migration {
        version: 18,
        statements: &[
            "ALTER TABLE charts ADD COLUMN source_url TEXT NOT NULL DEFAULT '';",
            "ALTER TABLE charts ADD COLUMN append_url TEXT NOT NULL DEFAULT '';",
            "ALTER TABLE charts ADD COLUMN headers_json TEXT NOT NULL DEFAULT '{}';",
        ],
    },
    Migration {
        version: 19,
        statements: &[
            "ALTER TABLE chart_analysis ADD COLUMN loudness_lufs REAL;",
            "ALTER TABLE chart_analysis ADD COLUMN normalization_gain REAL;",
            "ALTER TABLE chart_analysis
                ADD COLUMN loudness_analysis_version INTEGER NOT NULL DEFAULT 0;",
        ],
    },
];

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
        // Per-chart score history rows can be tagged with the library.db
        // `course_scores.id` of the course attempt they belong to, so a chart
        // play can be traced back to its course context.  NULL means "solo
        // play" or "course history written before this migration".
        //
        // No FK is added because `score.db` and `library.db` are separate
        // SQLite databases and FKs cannot span them.  `course_score_id` is
        // a plain INTEGER index into `library.db`'s `course_scores.id`.
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
];
