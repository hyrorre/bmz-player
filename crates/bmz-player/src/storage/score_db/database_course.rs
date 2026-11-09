use super::*;

impl ScoreDatabase {
    pub fn insert_course_score(&mut self, record: &CourseScoreInsert) -> Result<i64> {
        super::super::course_score_db::insert_course_score(&mut self.conn, record)
    }

    pub fn insert_imported_course_replay(
        &mut self,
        record: &CourseScoreInsert,
        slot: u8,
        source_path: &str,
        source_fingerprint: &str,
    ) -> Result<i64> {
        super::super::course_score_db::insert_imported_course_replay(
            &mut self.conn,
            record,
            slot,
            source_path,
            source_fingerprint,
        )
    }

    pub fn course_replay_slot_source(
        &self,
        course_hash: &str,
        ln_policy: LnScorePolicy,
        rule_mode: RuleMode,
        slot: u8,
    ) -> Result<Option<CourseReplaySlotSource>> {
        super::super::course_score_db::course_replay_slot_source(
            &self.conn,
            course_hash,
            ln_policy,
            rule_mode,
            slot,
        )
    }

    pub fn best_course_score(
        &self,
        course_hash: &str,
        ln_policy: LnScorePolicy,
        rule_mode: RuleMode,
    ) -> Result<Option<CourseBestScore>> {
        super::super::course_score_db::best_course_score(
            &self.conn,
            course_hash,
            ln_policy,
            rule_mode,
        )
    }

    pub fn best_course_clear(
        &self,
        course_hash: &str,
        ln_policy: LnScorePolicy,
        rule_mode: RuleMode,
    ) -> Result<Option<bmz_core::clear::ClearType>> {
        super::super::course_score_db::best_course_clear(
            &self.conn,
            course_hash,
            ln_policy,
            rule_mode,
        )
    }

    pub fn list_course_score_charts(
        &self,
        course_score_id: i64,
    ) -> Result<Vec<CourseScoreChartRecord>> {
        super::super::course_score_db::list_course_score_charts(&self.conn, course_score_id)
    }

    pub fn list_course_replays(&self, course_score_id: i64) -> Result<Vec<CourseReplayRecord>> {
        super::super::course_score_db::list_course_replays(&self.conn, course_score_id)
    }

    pub fn course_replay_attempt_is_complete(&self, course_score_id: i64) -> Result<bool> {
        super::super::course_score_db::course_replay_attempt_is_complete(
            &self.conn,
            course_score_id,
        )
    }

    pub fn latest_course_score_id(
        &self,
        course_hash: &str,
        ln_policy: LnScorePolicy,
        rule_mode: RuleMode,
    ) -> Result<Option<i64>> {
        super::super::course_score_db::latest_course_score_id(
            &self.conn,
            course_hash,
            ln_policy,
            rule_mode,
        )
    }

    pub fn list_recent_course_scores(
        &self,
        course_hash: &str,
        ln_policy: LnScorePolicy,
        rule_mode: RuleMode,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<CourseScoreEntry>> {
        super::super::course_score_db::list_recent_course_scores(
            &self.conn,
            course_hash,
            ln_policy,
            rule_mode,
            limit,
            offset,
        )
    }

    pub fn list_recent_course_scores_all_contexts(
        &self,
        course_hash: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<CourseScoreEntry>> {
        super::super::course_score_db::list_recent_course_scores_all_contexts(
            &self.conn,
            course_hash,
            limit,
            offset,
        )
    }

    pub fn course_score_entry_by_id(
        &self,
        course_score_id: i64,
    ) -> Result<Option<CourseScoreEntry>> {
        super::super::course_score_db::course_score_entry_by_id(&self.conn, course_score_id)
    }

    pub fn upsert_course_replay_slot(&mut self, record: &CourseReplaySlotRecord) -> Result<()> {
        super::super::course_score_db::upsert_course_replay_slot(&mut self.conn, record)
    }

    pub fn course_replay_slot(
        &self,
        course_hash: &str,
        ln_policy: LnScorePolicy,
        rule_mode: RuleMode,
        slot: u8,
    ) -> Result<Option<CourseReplaySlotRecord>> {
        super::super::course_score_db::course_replay_slot(
            &self.conn,
            course_hash,
            ln_policy,
            rule_mode,
            slot,
        )
    }

    pub fn course_replay_slots_for_course(
        &self,
        course_hash: &str,
        ln_policy: LnScorePolicy,
        rule_mode: RuleMode,
    ) -> Result<[Option<CourseReplaySlotRecord>; 4]> {
        super::super::course_score_db::course_replay_slots_for_course(
            &self.conn,
            course_hash,
            ln_policy,
            rule_mode,
        )
    }

    pub fn course_replay_slot_presence(
        &self,
        course_hash: &str,
        ln_policy: LnScorePolicy,
        rule_mode: RuleMode,
    ) -> Result<[bool; 4]> {
        super::super::course_score_db::course_replay_slot_presence(
            &self.conn,
            course_hash,
            ln_policy,
            rule_mode,
        )
    }

    pub fn achieved_trophy_names_for_definition(
        &self,
        course_hash: &str,
        ln_policy: LnScorePolicy,
        rule_mode: RuleMode,
        trophies: &[bmz_core::course::CourseTrophy],
    ) -> Result<Vec<String>> {
        super::super::course_score_db::achieved_trophy_names_for_definition(
            &self.conn,
            course_hash,
            ln_policy,
            rule_mode,
            trophies,
        )
    }

    pub fn best_course_score_for_trophy(
        &self,
        course_hash: &str,
        ln_policy: LnScorePolicy,
        rule_mode: RuleMode,
        trophy_name: &str,
    ) -> Result<Option<CourseBestScore>> {
        super::super::course_score_db::best_course_score_for_trophy(
            &self.conn,
            course_hash,
            ln_policy,
            rule_mode,
            trophy_name,
        )
    }

    /// Tag the given `score_history` rows with a course attempt id.
    ///
    /// `course_score_id` references this score DB's `course_scores.id`.
    pub fn tag_score_history_with_course(
        &mut self,
        score_history_ids: &[i64],
        course_score_id: i64,
    ) -> Result<usize> {
        if score_history_ids.is_empty() {
            return Ok(0);
        }
        let tx = self.conn.transaction()?;
        let mut total = 0_usize;
        {
            let mut stmt =
                tx.prepare("UPDATE score_history SET course_score_id = ?1 WHERE id = ?2")?;
            for id in score_history_ids {
                total += stmt.execute(params![course_score_id, id])?;
            }
        }
        tx.commit()?;
        Ok(total)
    }
}
