use super::*;

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
        // Effective BMS-scale TOTAL for beatoraja skin ref 368 (chart_totalgauge).
        // BMSON info.total is normalized before storage.
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
    Migration {
        version: 20,
        // Course play results now belong to profile-local score.db.  Keep
        // library.db focused on chart/course metadata and drop the old
        // library-owned result tables without row migration.
        statements: &[
            "DROP TABLE IF EXISTS course_trophy_achievements;",
            "DROP TABLE IF EXISTS course_replay_slots;",
            "DROP TABLE IF EXISTS course_replays;",
            "DROP TABLE IF EXISTS course_score_charts;",
            "DROP TABLE IF EXISTS course_scores;",
        ],
    },
    Migration {
        version: 21,
        // Persist exact long-note pair counts so select/course views can derive
        // score-target counts for the active LN policy without reparsing BMS.
        statements: &[
            "ALTER TABLE charts ADD COLUMN undefined_ln_pairs INTEGER NOT NULL DEFAULT 0;",
            "ALTER TABLE charts ADD COLUMN defined_ln_pairs INTEGER NOT NULL DEFAULT 0;",
            "ALTER TABLE charts ADD COLUMN defined_cn_pairs INTEGER NOT NULL DEFAULT 0;",
            "ALTER TABLE charts ADD COLUMN defined_hcn_pairs INTEGER NOT NULL DEFAULT 0;",
        ],
    },
    Migration {
        version: 22,
        // Raw BMS headers are not consumed from library.db.  Earlier releases
        // also captured Base62 channel data here, which could make the
        // library database disproportionately large.
        statements: &["UPDATE charts SET headers_json = '{}' WHERE headers_json <> '{}';"],
    },
    Migration {
        version: 23,
        // Course entries are initially resolved when their course is imported.
        // Repair entries whose matching chart was imported later, preserving
        // the same SHA-256-first, MD5-fallback rule used by course import.
        statements: &["UPDATE course_entries
             SET chart_id = COALESCE(
                 (
                     SELECT id
                     FROM charts
                     WHERE course_entries.sha256 <> ''
                       AND charts.sha256 = course_entries.sha256
                     ORDER BY id
                     LIMIT 1
                 ),
                 (
                     SELECT id
                     FROM charts
                     WHERE course_entries.md5 <> ''
                       AND charts.md5 = course_entries.md5
                     ORDER BY id
                     LIMIT 1
                 )
             )
             WHERE chart_id IS NULL
               AND EXISTS (
                   SELECT 1
                   FROM charts
                   WHERE (course_entries.sha256 <> '' AND charts.sha256 = course_entries.sha256)
                      OR (course_entries.md5 <> '' AND charts.md5 = course_entries.md5)
               );"],
    },
    Migration {
        version: 24,
        // Difficulty-table navigation filters entries by table and level.
        // Keep that lookup indexed without changing the stored data.
        statements: &["CREATE INDEX idx_dte_table_id_level
            ON difficulty_table_entries(table_id, level);"],
    },
    Migration {
        version: 25,
        // Preserve missing-chart acquisition metadata supplied by difficulty tables.
        statements: &[
            "ALTER TABLE difficulty_table_entries ADD COLUMN url TEXT NOT NULL DEFAULT '';",
            "ALTER TABLE difficulty_table_entries ADD COLUMN append_url TEXT NOT NULL DEFAULT '';",
            "ALTER TABLE difficulty_table_entries ADD COLUMN ipfs TEXT NOT NULL DEFAULT '';",
            "ALTER TABLE difficulty_table_entries ADD COLUMN append_ipfs TEXT NOT NULL DEFAULT '';",
        ],
    },
    Migration {
        version: 26,
        // Existing tables predate download metadata persistence. Refetch them once even when
        // regular startup refreshes are disabled, then mark successful upserts as current.
        statements: &["ALTER TABLE difficulty_tables
             ADD COLUMN download_metadata_version INTEGER NOT NULL DEFAULT 0;"],
    },
    Migration {
        version: 27,
        // beatoraja records whether a song folder contains a text document while
        // scanning the library. NULL lets an interrupted upgrade resume its
        // one-time backfill on the next startup.
        statements: &["ALTER TABLE charts ADD COLUMN has_document INTEGER;"],
    },
    Migration {
        version: 28,
        // 正規化ゲインは loudness_lufs と再生目標から導出できるため保存しない。
        statements: &["ALTER TABLE chart_analysis DROP COLUMN normalization_gain;"],
    },
    Migration {
        version: 29,
        // `scanned_at` は再スキャンごとに更新されるため NEW フォルダには使えない。
        // 初回発見時刻を別に保持し、既存行は現在保存されている scan 時刻で初期化する。
        statements: &[
            "ALTER TABLE chart_files ADD COLUMN first_seen_at INTEGER NOT NULL DEFAULT 0;",
            "UPDATE chart_files SET first_seen_at = scanned_at WHERE first_seen_at = 0;",
            "CREATE INDEX idx_chart_files_first_seen_at ON chart_files(first_seen_at DESC);",
        ],
    },
    Migration {
        version: 30,
        // Windows accepts both slash styles. Folder navigation stores `/`, while
        // an initial filesystem scan commonly discovers `\`; merge rows that
        // therefore identify the same file before normalizing persisted keys.
        statements: &[r#"
            CREATE TEMP TABLE bmz_root_path_survivors AS
            SELECT path_key, id AS keep_root_id
            FROM (
                SELECT
                    id,
                    REPLACE(path, '\', '/') AS path_key,
                    ROW_NUMBER() OVER (
                        PARTITION BY REPLACE(path, '\', '/')
                        ORDER BY id DESC
                    ) AS path_rank
                FROM roots
            )
            WHERE path_rank = 1;

            CREATE TEMP TABLE bmz_root_path_map AS
            SELECT roots.id AS old_root_id, survivors.keep_root_id
            FROM roots
            JOIN bmz_root_path_survivors survivors
              ON survivors.path_key = REPLACE(roots.path, '\', '/');

            CREATE UNIQUE INDEX bmz_root_path_map_old_idx
                ON bmz_root_path_map(old_root_id);
            CREATE INDEX bmz_root_path_map_keep_idx
                ON bmz_root_path_map(keep_root_id);

            UPDATE roots AS keep
            SET last_scan_at = (
                SELECT MAX(candidate.last_scan_at)
                FROM roots candidate
                JOIN bmz_root_path_map path_map
                  ON path_map.old_root_id = candidate.id
                WHERE path_map.keep_root_id = keep.id
            )
            WHERE keep.id IN (SELECT keep_root_id FROM bmz_root_path_survivors);

            UPDATE chart_files
            SET root_id = (
                SELECT path_map.keep_root_id
                FROM bmz_root_path_map path_map
                WHERE path_map.old_root_id = chart_files.root_id
            )
            WHERE root_id IN (
                SELECT old_root_id
                FROM bmz_root_path_map
                WHERE old_root_id <> keep_root_id
            );

            DELETE FROM roots
            WHERE id IN (
                SELECT old_root_id
                FROM bmz_root_path_map
                WHERE old_root_id <> keep_root_id
            );

            UPDATE roots SET path = REPLACE(path, '\', '/');

            CREATE TEMP TABLE bmz_chart_file_path_survivors AS
            SELECT path_key, id AS keep_file_id
            FROM (
                SELECT
                    id,
                    REPLACE(path, '\', '/') AS path_key,
                    ROW_NUMBER() OVER (
                        PARTITION BY REPLACE(path, '\', '/')
                        ORDER BY scanned_at DESC, id DESC
                    ) AS path_rank
                FROM chart_files
            )
            WHERE path_rank = 1;

            CREATE TEMP TABLE bmz_chart_file_path_map AS
            SELECT
                chart_files.id AS old_file_id,
                survivors.keep_file_id,
                old_link.chart_id AS old_chart_id,
                keep_link.chart_id AS keep_chart_id
            FROM chart_files
            JOIN bmz_chart_file_path_survivors survivors
              ON survivors.path_key = REPLACE(chart_files.path, '\', '/')
            LEFT JOIN chart_file_links old_link
              ON old_link.chart_file_id = chart_files.id
            LEFT JOIN chart_file_links keep_link
              ON keep_link.chart_file_id = survivors.keep_file_id;

            CREATE UNIQUE INDEX bmz_chart_file_path_map_old_idx
                ON bmz_chart_file_path_map(old_file_id);
            CREATE INDEX bmz_chart_file_path_map_keep_idx
                ON bmz_chart_file_path_map(keep_file_id);
            CREATE INDEX bmz_chart_file_path_map_old_chart_idx
                ON bmz_chart_file_path_map(old_chart_id);

            UPDATE chart_files AS keep
            SET first_seen_at = (
                SELECT MIN(candidate.first_seen_at)
                FROM chart_files candidate
                JOIN bmz_chart_file_path_map path_map
                  ON path_map.old_file_id = candidate.id
                WHERE path_map.keep_file_id = keep.id
            )
            WHERE keep.id IN (
                SELECT keep_file_id FROM bmz_chart_file_path_survivors
            );

            UPDATE course_entries AS entry
            SET chart_id = (
                SELECT path_map.keep_chart_id
                FROM bmz_chart_file_path_map path_map
                WHERE path_map.old_chart_id = entry.chart_id
                  AND path_map.old_file_id <> path_map.keep_file_id
                ORDER BY path_map.old_file_id DESC
                LIMIT 1
            )
            WHERE EXISTS (
                SELECT 1
                FROM bmz_chart_file_path_map path_map
                WHERE path_map.old_chart_id = entry.chart_id
                  AND path_map.old_file_id <> path_map.keep_file_id
            );

            DELETE FROM chart_import_warnings
            WHERE chart_file_id IN (
                SELECT old_file_id
                FROM bmz_chart_file_path_map
                WHERE old_file_id <> keep_file_id
            );

            DELETE FROM chart_file_links
            WHERE chart_file_id IN (
                SELECT old_file_id
                FROM bmz_chart_file_path_map
                WHERE old_file_id <> keep_file_id
            );

            DELETE FROM chart_files
            WHERE id IN (
                SELECT old_file_id
                FROM bmz_chart_file_path_map
                WHERE old_file_id <> keep_file_id
            );

            DELETE FROM charts
            WHERE id IN (
                SELECT old_chart_id
                FROM bmz_chart_file_path_map
                WHERE old_file_id <> keep_file_id
                  AND old_chart_id IS NOT NULL
            )
              AND NOT EXISTS (
                  SELECT 1
                  FROM chart_file_links
                  WHERE chart_file_links.chart_id = charts.id
              );

            UPDATE chart_files SET path = REPLACE(path, '\', '/');

            DROP TABLE bmz_chart_file_path_map;
            DROP TABLE bmz_chart_file_path_survivors;
            DROP TABLE bmz_root_path_map;
            DROP TABLE bmz_root_path_survivors;
        "#],
    },
    Migration {
        version: 31,
        // Windows canonicalization can prefix local paths with `\\?\` and UNC
        // paths with `\\?\UNC\`. Strip those prefixes and merge any rows that
        // already exist under the corresponding ordinary path.
        statements: &[r#"
            CREATE TEMP TABLE bmz31_root_path_keys (
                id INTEGER PRIMARY KEY,
                path_key TEXT NOT NULL
            );
            INSERT INTO bmz31_root_path_keys (id, path_key)
            SELECT
                id,
                CASE
                    WHEN LOWER(SUBSTR(normalized_path, 1, 8)) = '//?/unc/'
                        THEN '//' || SUBSTR(normalized_path, 9)
                    WHEN SUBSTR(normalized_path, 1, 4) = '//?/'
                        THEN SUBSTR(normalized_path, 5)
                    ELSE normalized_path
                END
            FROM (
                SELECT id, REPLACE(path, '\', '/') AS normalized_path
                FROM roots
            );

            CREATE TEMP TABLE bmz31_root_path_survivors AS
            SELECT path_key, id AS keep_root_id
            FROM (
                SELECT
                    id,
                    path_key,
                    ROW_NUMBER() OVER (
                        PARTITION BY path_key
                        ORDER BY id DESC
                    ) AS path_rank
                FROM bmz31_root_path_keys
            )
            WHERE path_rank = 1;

            CREATE TEMP TABLE bmz31_root_path_map AS
            SELECT path_keys.id AS old_root_id, survivors.keep_root_id
            FROM bmz31_root_path_keys path_keys
            JOIN bmz31_root_path_survivors survivors
              ON survivors.path_key = path_keys.path_key;

            CREATE UNIQUE INDEX bmz31_root_path_map_old_idx
                ON bmz31_root_path_map(old_root_id);
            CREATE INDEX bmz31_root_path_map_keep_idx
                ON bmz31_root_path_map(keep_root_id);

            UPDATE roots AS keep
            SET last_scan_at = (
                SELECT MAX(candidate.last_scan_at)
                FROM roots candidate
                JOIN bmz31_root_path_map path_map
                  ON path_map.old_root_id = candidate.id
                WHERE path_map.keep_root_id = keep.id
            )
            WHERE keep.id IN (SELECT keep_root_id FROM bmz31_root_path_survivors);

            UPDATE chart_files
            SET root_id = (
                SELECT path_map.keep_root_id
                FROM bmz31_root_path_map path_map
                WHERE path_map.old_root_id = chart_files.root_id
            )
            WHERE root_id IN (
                SELECT old_root_id
                FROM bmz31_root_path_map
                WHERE old_root_id <> keep_root_id
            );

            DELETE FROM roots
            WHERE id IN (
                SELECT old_root_id
                FROM bmz31_root_path_map
                WHERE old_root_id <> keep_root_id
            );

            UPDATE roots
            SET path = (
                SELECT path_keys.path_key
                FROM bmz31_root_path_keys path_keys
                WHERE path_keys.id = roots.id
            );

            CREATE TEMP TABLE bmz31_chart_file_path_keys (
                id INTEGER PRIMARY KEY,
                path_key TEXT NOT NULL
            );
            INSERT INTO bmz31_chart_file_path_keys (id, path_key)
            SELECT
                id,
                CASE
                    WHEN LOWER(SUBSTR(normalized_path, 1, 8)) = '//?/unc/'
                        THEN '//' || SUBSTR(normalized_path, 9)
                    WHEN SUBSTR(normalized_path, 1, 4) = '//?/'
                        THEN SUBSTR(normalized_path, 5)
                    ELSE normalized_path
                END
            FROM (
                SELECT id, REPLACE(path, '\', '/') AS normalized_path
                FROM chart_files
            );

            CREATE TEMP TABLE bmz31_chart_file_path_survivors AS
            SELECT path_key, id AS keep_file_id
            FROM (
                SELECT
                    path_keys.id,
                    path_keys.path_key,
                    ROW_NUMBER() OVER (
                        PARTITION BY path_keys.path_key
                        ORDER BY chart_files.scanned_at DESC, path_keys.id DESC
                    ) AS path_rank
                FROM bmz31_chart_file_path_keys path_keys
                JOIN chart_files ON chart_files.id = path_keys.id
            )
            WHERE path_rank = 1;

            CREATE TEMP TABLE bmz31_chart_file_path_map AS
            SELECT
                path_keys.id AS old_file_id,
                survivors.keep_file_id,
                old_link.chart_id AS old_chart_id,
                keep_link.chart_id AS keep_chart_id
            FROM bmz31_chart_file_path_keys path_keys
            JOIN bmz31_chart_file_path_survivors survivors
              ON survivors.path_key = path_keys.path_key
            LEFT JOIN chart_file_links old_link
              ON old_link.chart_file_id = path_keys.id
            LEFT JOIN chart_file_links keep_link
              ON keep_link.chart_file_id = survivors.keep_file_id;

            CREATE UNIQUE INDEX bmz31_chart_file_path_map_old_idx
                ON bmz31_chart_file_path_map(old_file_id);
            CREATE INDEX bmz31_chart_file_path_map_keep_idx
                ON bmz31_chart_file_path_map(keep_file_id);
            CREATE INDEX bmz31_chart_file_path_map_old_chart_idx
                ON bmz31_chart_file_path_map(old_chart_id);

            UPDATE chart_files AS keep
            SET first_seen_at = (
                SELECT MIN(candidate.first_seen_at)
                FROM chart_files candidate
                JOIN bmz31_chart_file_path_map path_map
                  ON path_map.old_file_id = candidate.id
                WHERE path_map.keep_file_id = keep.id
            )
            WHERE keep.id IN (
                SELECT keep_file_id FROM bmz31_chart_file_path_survivors
            );

            UPDATE course_entries AS entry
            SET chart_id = (
                SELECT path_map.keep_chart_id
                FROM bmz31_chart_file_path_map path_map
                WHERE path_map.old_chart_id = entry.chart_id
                  AND path_map.old_file_id <> path_map.keep_file_id
                ORDER BY path_map.old_file_id DESC
                LIMIT 1
            )
            WHERE EXISTS (
                SELECT 1
                FROM bmz31_chart_file_path_map path_map
                WHERE path_map.old_chart_id = entry.chart_id
                  AND path_map.old_file_id <> path_map.keep_file_id
            );

            DELETE FROM chart_import_warnings
            WHERE chart_file_id IN (
                SELECT old_file_id
                FROM bmz31_chart_file_path_map
                WHERE old_file_id <> keep_file_id
            );

            DELETE FROM chart_file_links
            WHERE chart_file_id IN (
                SELECT old_file_id
                FROM bmz31_chart_file_path_map
                WHERE old_file_id <> keep_file_id
            );

            DELETE FROM chart_files
            WHERE id IN (
                SELECT old_file_id
                FROM bmz31_chart_file_path_map
                WHERE old_file_id <> keep_file_id
            );

            DELETE FROM charts
            WHERE id IN (
                SELECT old_chart_id
                FROM bmz31_chart_file_path_map
                WHERE old_file_id <> keep_file_id
                  AND old_chart_id IS NOT NULL
            )
              AND NOT EXISTS (
                  SELECT 1
                  FROM chart_file_links
                  WHERE chart_file_links.chart_id = charts.id
              );

            UPDATE chart_files
            SET path = (
                SELECT path_keys.path_key
                FROM bmz31_chart_file_path_keys path_keys
                WHERE path_keys.id = chart_files.id
            );

            UPDATE charts
            SET folder_path = CASE
                WHEN LOWER(SUBSTR(REPLACE(folder_path, '\', '/'), 1, 8)) = '//?/unc/'
                    THEN '//' || SUBSTR(REPLACE(folder_path, '\', '/'), 9)
                WHEN SUBSTR(REPLACE(folder_path, '\', '/'), 1, 4) = '//?/'
                    THEN SUBSTR(REPLACE(folder_path, '\', '/'), 5)
                ELSE REPLACE(folder_path, '\', '/')
            END;

            DROP TABLE bmz31_chart_file_path_map;
            DROP TABLE bmz31_chart_file_path_survivors;
            DROP TABLE bmz31_chart_file_path_keys;
            DROP TABLE bmz31_root_path_map;
            DROP TABLE bmz31_root_path_survivors;
            DROP TABLE bmz31_root_path_keys;
        "#],
    },
    Migration {
        version: 32,
        // Gated integrated loudnessだけでは、短い大音量区間と合成後peakを
        // プレイ時のゲインへ反映できない。再生目標からゲインを再導出できるよう
        // raw analysis metricsを保存する。
        statements: &[
            "ALTER TABLE chart_analysis ADD COLUMN short_term_lufs REAL;",
            "ALTER TABLE chart_analysis ADD COLUMN sample_peak REAL;",
        ],
    },
    Migration {
        version: 33,
        // Filesystemを走査するmaintenanceを通常migrationから分離し、成功済みtaskを
        // 次回起動以降に繰り返さないためのledgerを持つ。
        statements: &["CREATE TABLE library_maintenance (
            task TEXT PRIMARY KEY,
            completed_at INTEGER NOT NULL
        );"],
    },
    Migration {
        version: 34,
        statements: &[
            "ALTER TABLE charts ADD COLUMN has_defined_hln INTEGER NOT NULL DEFAULT 0;",
            "ALTER TABLE charts ADD COLUMN defined_hln_pairs INTEGER NOT NULL DEFAULT 0;",
        ],
    },
    Migration {
        version: 35,
        statements: &["ALTER TABLE charts ADD COLUMN has_conditional INTEGER NOT NULL DEFAULT 0;"],
    },
];
