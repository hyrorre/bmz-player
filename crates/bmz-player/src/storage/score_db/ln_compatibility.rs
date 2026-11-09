//! Read-time aliases only. Persisted attempts, replay files and IR identities
//! always retain their original policy. No historical rows are migrated.
use super::*;
use crate::ln_policy::{ChartLnProfile, uniform_auto_legacy_policy};
use crate::storage::library_db::{CHART_IMPORT_VERSION, LibraryDatabase};

impl ScoreDatabase {
    pub(crate) fn replay_ln_policy_matches(
        &self,
        sha: [u8; 32],
        requested: LnScorePolicy,
        stored: LnScorePolicy,
    ) -> Result<bool> {
        self.initialize_ln_compatibility(None)?;
        if requested == stored {
            return Ok(true);
        }
        Ok(self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM ln_chart_aliases WHERE sha = ?1 AND canonical = ?2 AND old_policy = ?3)",
            params![hash_to_hex(&sha), requested.as_str(), stored.as_str()], |row| row.get(0))?)
    }
    pub(super) fn legacy_course_policy(
        &self,
        hash: &str,
        policy: LnScorePolicy,
    ) -> Result<Option<LnScorePolicy>> {
        self.initialize_ln_compatibility(None)?;
        let value: Option<String> = self.conn.query_row(
            "SELECT old_policy FROM ln_course_aliases WHERE course_hash = ?1 AND canonical = ?2",
            params![hash, policy.as_str()], |row| row.get(0)).optional()?;
        Ok(value.as_deref().and_then(LnScorePolicy::from_str_opt))
    }
    pub(super) fn initialize_ln_compatibility(&self, path: Option<&Path>) -> Result<()> {
        let initialized: bool = self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM temp.sqlite_master WHERE name = 'ln_compatible_profiles')",
            [], |row| row.get(0))?;
        if initialized {
            return Ok(());
        }
        // Opening a DB before running migrations is supported. Invalid views
        // would prevent SQLite ALTER TABLE from completing those migrations.
        let ready: bool = self.conn.query_row(
            "SELECT COUNT(*) = 5 FROM main.sqlite_master WHERE type = 'table'
                AND name IN ('score_best', 'score_history', 'replay_slots', 'course_scores', 'score_unrecorded_plays')",
            [], |row| row.get(0))?;
        if !ready {
            return Ok(());
        }
        let path = path.or_else(|| self.conn.path().map(Path::new));
        self.conn.execute_batch(
            "CREATE TEMP TABLE ln_compatible_profiles (
                sha TEXT PRIMARY KEY, undefined INTEGER, ln INTEGER, cn INTEGER, hcn INTEGER);
             CREATE TEMP VIEW ln_chart_aliases AS
             SELECT p.sha, r.old_policy, r.canonical FROM ln_compatible_profiles p
             JOIN ln_uniform_alias_rules r USING (undefined, ln, cn, hcn);
             CREATE TEMP TABLE ln_uniform_alias_rules (
                undefined INTEGER, ln INTEGER, cn INTEGER, hcn INTEGER, old_policy TEXT, canonical TEXT);
             CREATE TEMP VIEW ln_course_profiles AS
             SELECT cs.course_hash, MAX(p.undefined) undefined, MAX(p.ln) ln, MAX(p.cn) cn, MAX(p.hcn) hcn
             FROM main.course_scores cs, json_each(cs.chart_sha256s_json) stage
             LEFT JOIN ln_compatible_profiles p ON p.sha = stage.value
             GROUP BY cs.course_hash
             HAVING COUNT(*) = COUNT(p.sha) AND COUNT(*) > 0
                AND COUNT(DISTINCT cs.chart_sha256s_json) = 1
                AND NOT EXISTS (SELECT 1 FROM main.course_scores missing
                    WHERE missing.course_hash = cs.course_hash
                      AND json_array_length(missing.chart_sha256s_json) = 0);
             CREATE TEMP VIEW ln_course_aliases AS
             SELECT p.course_hash, r.old_policy, r.canonical FROM ln_course_profiles p
             JOIN ln_uniform_alias_rules r USING (undefined, ln, cn, hcn)
             WHERE NOT EXISTS (SELECT 1 FROM main.course_scores cs
                WHERE cs.course_hash = p.course_hash AND cs.ln_policy = r.old_policy
                  AND (json_extract(cs.constraints_json, '$.ln') IS NULL OR
                    json_extract(cs.constraints_json, '$.ln') NOT IN
                        ('default', CASE r.old_policy WHEN 'AutoLn' THEN 'ln'
                            WHEN 'AutoCn' THEN 'cn' ELSE 'hcn' END)));"
        )?;
        for bits in 0..16 {
            let p = ChartLnProfile {
                has_undefined_ln: bits & 1 != 0,
                has_defined_ln: bits & 2 != 0,
                has_defined_cn: bits & 4 != 0,
                has_defined_hcn: bits & 8 != 0,
            };
            for policy in [LnScorePolicy::ForceLn, LnScorePolicy::ForceCn, LnScorePolicy::ForceHcn]
            {
                if let Some(old) = uniform_auto_legacy_policy(policy, p) {
                    self.conn.execute(
                        "INSERT INTO ln_uniform_alias_rules VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![
                            p.has_undefined_ln,
                            p.has_defined_ln,
                            p.has_defined_cn,
                            p.has_defined_hcn,
                            old.as_str(),
                            policy.as_str()
                        ],
                    )?;
                }
            }
        }
        for (table, key, aliases, alias_key) in [
            ("score_history", "chart_sha256", "ln_chart_aliases", "sha"),
            ("score_unrecorded_plays", "chart_sha256", "ln_chart_aliases", "sha"),
            ("replay_slots", "chart_sha256", "ln_chart_aliases", "sha"),
            ("course_scores", "course_hash", "ln_course_aliases", "course_hash"),
        ] {
            // Logical fields come first; SQLite gives the stored duplicates
            // private suffixed names. Keep b.* lazy so opening an empty DB or
            // altering unrelated history columns does not freeze its schema.
            self.conn.execute_batch(&format!(
                "CREATE TEMP VIEW {table}_compatible AS SELECT COALESCE(a.canonical, b.ln_policy) AS ln_policy,
                    b.*, b.ln_policy stored_ln_policy
                 FROM main.{table} b LEFT JOIN {aliases} a
                   ON a.{alias_key} = b.{key} AND a.old_policy = b.ln_policy;"
            ))?;
        }
        self.create_compatible_best_view()?;
        // ProfilePaths is data/profiles/<id>/score.db. Do not search arbitrary
        // ancestors or open/create a library for a standalone score database.
        if let Some(data) = path
            .and_then(|p| p.parent())
            .and_then(Path::parent)
            .filter(|p| p.file_name().is_some_and(|n| n == "profiles"))
            .and_then(Path::parent)
        {
            let library_path = data.join("library.db");
            let schema_ready: bool = self.conn.query_row(
                "SELECT COUNT(*) = 4 FROM main.sqlite_master WHERE type = 'table'
                    AND name IN ('score_best', 'score_history', 'replay_slots', 'course_scores')",
                [],
                |row| row.get(0),
            )?;
            if library_path.is_file() && schema_ready {
                let library = LibraryDatabase::open_read_only(&library_path)?;
                self.refresh_ln_compatibility(&library)?;
            }
        }
        Ok(())
    }

    /// Evidence is refreshed after library changes and before imports. Current
    /// parser metadata, a verified same-SHA readable copy, and no RANDOM are all
    /// required. Conflicting current metadata never establishes an alias.
    pub(crate) fn refresh_ln_compatibility(&self, library: &LibraryDatabase) -> Result<()> {
        self.initialize_ln_compatibility(None)?;
        // Verify only hashes needed by legacy buckets, never hash every song
        // during profile startup or library refresh.
        let mut stmt = self.conn.prepare(
            "SELECT chart_sha256 FROM score_best WHERE ln_policy LIKE 'Auto%'
             UNION SELECT chart_sha256 FROM replay_slots WHERE ln_policy LIKE 'Auto%'
             UNION SELECT chart_sha256 FROM score_history WHERE ln_policy LIKE 'Auto%'
             UNION SELECT stage.value FROM course_scores cs, json_each(cs.chart_sha256s_json) stage
                WHERE cs.ln_policy LIKE 'Auto%'",
        )?;
        let hashes = stmt
            .query_map([], |row| hex_to_hash::<32>(&row.get::<_, String>(0)?))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut profiles = HashMap::new();
        for sha in hashes {
            let sources = library.preferred_chart_metadata(sha)?;
            let profile = sources.first().map(|s| s.chart.ln_profile);
            let trusted = profile.is_some()
                && sources.iter().all(|s| {
                    s.import_version == CHART_IMPORT_VERSION
                        && !s.chart.has_bms_random
                        && Some(s.chart.ln_profile) == profile
                })
                && sources.iter().any(|s| {
                    library
                        .verified_chart_source(s.chart.chart_id)
                        .is_ok_and(|verified| verified.chart.sha256 == sha)
                });
            profiles.insert(sha, profile.filter(|_| trusted));
        }
        self.conn.execute("DELETE FROM ln_compatible_profiles", [])?;
        for (sha, profile) in profiles {
            if let Some(p) = profile {
                self.register_ln_profile(sha, p)?;
            }
        }
        Ok(())
    }

    pub(super) fn register_ln_profile(&self, sha: [u8; 32], p: ChartLnProfile) -> Result<()> {
        self.conn.execute(
            "INSERT INTO ln_compatible_profiles VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                hash_to_hex(&sha),
                p.has_undefined_ln,
                p.has_defined_ln,
                p.has_defined_cn,
                p.has_defined_hcn
            ],
        )?;
        Ok(())
    }

    fn create_compatible_best_view(&self) -> Result<()> {
        let partition = "PARTITION BY chart_sha256, canonical, double_option, rule_mode";
        // As above, named consumers see the logical fields first; all other
        // representative metadata comes from the chosen physical row unchanged.
        let projection = format!(
            "canonical AS ln_policy, MAX(ex_score) OVER ({partition}) ex_score,
             MAX(max_combo) OVER ({partition}) max_combo,
             COALESCE(MIN(CASE WHEN numeric_record THEN bp END) OVER ({partition}), 0) bp,
             COALESCE(MIN(CASE WHEN numeric_record THEN cb END) OVER ({partition}), 0) cb,
             SUM(play_count) OVER ({partition}) play_count, SUM(clear_count) OVER ({partition}) clear_count,
             FIRST_VALUE(clear_type) OVER ({partition} ORDER BY clear_rank DESC, ln_policy = canonical DESC) clear_type,
             FIRST_VALUE(gauge_type) OVER ({partition} ORDER BY clear_rank DESC, ln_policy = canonical DESC) gauge_type,
             FIRST_VALUE(gauge_value) OVER ({partition} ORDER BY clear_rank DESC, ln_policy = canonical DESC) gauge_value,
             candidates.*"
        );
        self.conn.execute_batch(&format!(
            "CREATE TEMP VIEW score_best_compatible AS
             WITH candidates AS (
                SELECT b.*, COALESCE(a.canonical, b.ln_policy) canonical,
                    CASE b.clear_type WHEN 'Max' THEN 10 WHEN 'Perfect' THEN 9 WHEN 'FullCombo' THEN 8
                        WHEN 'ExHard' THEN 7 WHEN 'Hard' THEN 6 WHEN 'Normal' THEN 5 WHEN 'Easy' THEN 4
                        WHEN 'LightAssistEasy' THEN 3 WHEN 'AssistEasy' THEN 2 WHEN 'Failed' THEN 1 ELSE 0 END clear_rank,
                    b.best_score_history_id IS NOT NULL OR b.ex_score > 0 OR b.max_combo > 0
                        OR b.bp > 0 OR b.cb > 0 OR EXISTS (SELECT 1 FROM main.score_history prior
                            WHERE prior.chart_sha256 = b.chart_sha256 AND prior.ln_policy = b.ln_policy
                                AND prior.double_option = b.double_option AND CASE prior.rule_mode
                                    WHEN 'Lr2Oraja' THEN 'Lr2Oraja' WHEN 'Dx' THEN 'Dx'
                                    ELSE 'Beatoraja' END = b.rule_mode) numeric_record,
                    h.id representative_id, h.bp representative_bp, h.cb representative_cb,
                    h.max_combo representative_combo
                FROM main.score_best b LEFT JOIN ln_chart_aliases a
                    ON a.sha = b.chart_sha256 AND a.old_policy = b.ln_policy
                LEFT JOIN main.score_history h ON h.id = b.best_score_history_id
                    AND h.chart_sha256 = b.chart_sha256 AND h.ln_policy = b.ln_policy
                    AND h.double_option = b.double_option AND CASE h.rule_mode
                        WHEN 'Lr2Oraja' THEN 'Lr2Oraja' WHEN 'Dx' THEN 'Dx' ELSE 'Beatoraja' END = b.rule_mode
             ), merged AS (
                SELECT {projection}, ROW_NUMBER() OVER ({partition} ORDER BY ex_score DESC,
                    numeric_record DESC, representative_id IS NULL DESC,
                    representative_bp ASC, representative_cb ASC, representative_combo DESC,
                    ln_policy = canonical DESC) position
                FROM candidates
             ) SELECT * FROM merged WHERE position = 1;"
        ))?;
        Ok(())
    }
}
