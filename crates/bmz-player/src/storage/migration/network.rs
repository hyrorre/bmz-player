use super::*;

pub const NETWORK_MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
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
            "CREATE INDEX idx_ir_score_jobs_status_next_attempt
            ON ir_score_jobs(status, next_attempt_at);",
            "CREATE INDEX idx_ir_score_jobs_local_score
            ON ir_score_jobs(kind, local_score_id);",
            "CREATE TABLE ir_score_submissions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            job_id INTEGER NOT NULL,
            provider TEXT NOT NULL,
            account_id TEXT NOT NULL DEFAULT '',
            kind TEXT NOT NULL DEFAULT 'score',
            local_score_id INTEGER NOT NULL,
            remote_score_id TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL,
            submitted_at INTEGER NOT NULL,
            log_path TEXT NOT NULL DEFAULT '',
            error TEXT NOT NULL DEFAULT '',
            FOREIGN KEY(job_id) REFERENCES ir_score_jobs(id) ON DELETE CASCADE
        );",
            "CREATE INDEX idx_ir_score_submissions_local_score
            ON ir_score_submissions(kind, local_score_id);",
            "CREATE INDEX idx_ir_score_submissions_submitted_at
            ON ir_score_submissions(submitted_at);",
        ],
    },
    Migration {
        version: 2,
        statements: &[
            "CREATE TABLE ir_rival_scores (
                provider TEXT NOT NULL,
                rival_id TEXT NOT NULL,
                body TEXT NOT NULL,
                chart_sha256 TEXT NOT NULL,
                ln_mode INTEGER NOT NULL,
                ex_score INTEGER NOT NULL,
                clear_type INTEGER NOT NULL,
                max_combo INTEGER NOT NULL,
                min_bp INTEGER NOT NULL,
                play_option INTEGER NOT NULL,
                arrange_1p TEXT NOT NULL DEFAULT '',
                arrange_2p TEXT NOT NULL DEFAULT '',
                double_option TEXT NOT NULL DEFAULT '',
                play_seed INTEGER,
                fetched_at INTEGER NOT NULL,
                PRIMARY KEY(provider, rival_id, body, chart_sha256, ln_mode)
            );",
            "CREATE TABLE ir_rival_score_sync (
                provider TEXT NOT NULL,
                rival_id TEXT NOT NULL,
                body TEXT NOT NULL,
                etag TEXT NOT NULL DEFAULT '',
                fetched_at INTEGER NOT NULL,
                PRIMARY KEY(provider, rival_id, body)
            );",
        ],
    },
    Migration {
        version: 3,
        statements: &["ALTER TABLE ir_score_submissions
                ADD COLUMN response_json TEXT NOT NULL DEFAULT '';"],
    },
    Migration {
        version: 4,
        statements: &[
            // Rebuild both tables together so foreign keys and submission history survive.
            // No payload, status or retry schedule is changed by this migration.
            "ALTER TABLE ir_score_jobs RENAME TO ir_score_jobs_v3;",
            "ALTER TABLE ir_score_submissions RENAME TO ir_score_submissions_v3;",
            "CREATE TABLE ir_score_jobs (
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
                submission_key TEXT NOT NULL,
                UNIQUE(provider, account_id, kind, submission_key)
            );",
            "INSERT INTO ir_score_jobs
             SELECT *, CASE
                WHEN json_valid(payload_json) THEN
                    COALESCE(json_extract(payload_json, '$.idempotency_key'),
                             json_extract(payload_json, '$.remote_score_id'),
                             'local:' || local_score_id)
                ELSE 'local:' || local_score_id END
             FROM ir_score_jobs_v3;",
            "CREATE TABLE ir_score_submissions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                job_id INTEGER NOT NULL,
                provider TEXT NOT NULL,
                account_id TEXT NOT NULL DEFAULT '',
                kind TEXT NOT NULL DEFAULT 'score',
                local_score_id INTEGER NOT NULL,
                remote_score_id TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL,
                submitted_at INTEGER NOT NULL,
                log_path TEXT NOT NULL DEFAULT '',
                error TEXT NOT NULL DEFAULT '',
                response_json TEXT NOT NULL DEFAULT '',
                FOREIGN KEY(job_id) REFERENCES ir_score_jobs(id) ON DELETE CASCADE
            );",
            "INSERT INTO ir_score_submissions SELECT * FROM ir_score_submissions_v3;",
            "DROP TABLE ir_score_submissions_v3;",
            "DROP TABLE ir_score_jobs_v3;",
            "CREATE INDEX idx_ir_score_jobs_status_next_attempt
                ON ir_score_jobs(status, next_attempt_at);",
            "CREATE INDEX idx_ir_score_jobs_local_score
                ON ir_score_jobs(kind, local_score_id);",
            "CREATE INDEX idx_ir_score_submissions_local_score
                ON ir_score_submissions(kind, local_score_id);",
            "CREATE INDEX idx_ir_score_submissions_submitted_at
                ON ir_score_submissions(submitted_at);",
        ],
    },
    Migration {
        version: 5,
        statements: &[
            "CREATE TABLE ir_score_success_ledger (
                provider TEXT NOT NULL,
                account_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                submission_key TEXT NOT NULL,
                local_score_id INTEGER NOT NULL,
                remote_score_id TEXT NOT NULL DEFAULT '',
                submitted_at INTEGER NOT NULL,
                PRIMARY KEY(provider, account_id, kind, submission_key)
            );",
            "INSERT OR REPLACE INTO ir_score_success_ledger (
                provider, account_id, kind, submission_key, local_score_id,
                remote_score_id, submitted_at
             )
             SELECT s.provider, s.account_id, s.kind, j.submission_key,
                    s.local_score_id, s.remote_score_id, MAX(s.submitted_at)
             FROM ir_score_submissions s
             JOIN ir_score_jobs j ON j.id = s.job_id
             WHERE s.status = 'succeeded'
             GROUP BY s.provider, s.account_id, s.kind, j.submission_key;",
            "CREATE INDEX idx_ir_score_success_ledger_local_score
                ON ir_score_success_ledger(kind, local_score_id);",
            "CREATE INDEX idx_ir_score_success_ledger_remote_score
                ON ir_score_success_ledger(provider, account_id, remote_score_id);",
        ],
    },
    Migration {
        version: 6,
        statements: &[
            "ALTER TABLE ir_score_success_ledger
                ADD COLUMN submission_payload_json TEXT NOT NULL DEFAULT '';",
            "ALTER TABLE ir_score_success_ledger
                ADD COLUMN chart_sha256 TEXT NOT NULL DEFAULT '';",
            "ALTER TABLE ir_score_success_ledger
                ADD COLUMN ln_policy TEXT NOT NULL DEFAULT '';",
            // Only retained succeeded jobs can provide their original request body.
            // Already-pruned/cleared payloads intentionally remain unavailable.
            "UPDATE ir_score_success_ledger
             SET submission_payload_json = COALESCE((
                    SELECT payload_json FROM ir_score_jobs
                    WHERE provider = ir_score_success_ledger.provider
                      AND account_id = ir_score_success_ledger.account_id
                      AND kind = ir_score_success_ledger.kind
                      AND submission_key = ir_score_success_ledger.submission_key
                      AND status = 'succeeded'
                 ), ''),
                 chart_sha256 = COALESCE((
                    SELECT chart_sha256 FROM ir_score_jobs
                    WHERE provider = ir_score_success_ledger.provider
                      AND account_id = ir_score_success_ledger.account_id
                      AND kind = ir_score_success_ledger.kind
                      AND submission_key = ir_score_success_ledger.submission_key
                      AND status = 'succeeded'
                 ), ''),
                 ln_policy = COALESCE((
                    SELECT ln_policy FROM ir_score_jobs
                    WHERE provider = ir_score_success_ledger.provider
                      AND account_id = ir_score_success_ledger.account_id
                      AND kind = ir_score_success_ledger.kind
                      AND submission_key = ir_score_success_ledger.submission_key
                      AND status = 'succeeded'
                 ), '');",
        ],
    },
    Migration {
        version: 7,
        statements: &[
            "CREATE TABLE bokutachi_imports (
                job_id INTEGER PRIMARY KEY REFERENCES ir_score_jobs(id) ON DELETE CASCADE,
                progress_json TEXT NOT NULL
            );",
            "CREATE TABLE bokutachi_rate_limits (
                origin TEXT PRIMARY KEY,
                next_post_at INTEGER NOT NULL
            );",
        ],
    },
];
