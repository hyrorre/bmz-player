use super::*;

/// Keep batched hash lookups below SQLite's historical 999-variable default.
pub(super) const CHART_HASH_LOOKUP_BATCH_SIZE: usize = 500;
pub(super) const CHART_ANALYSIS_LOOKUP_BATCH_SIZE: usize = 500;

pub(super) fn charts_by_hash_column(
    conn: &Connection,
    column: &'static str,
    hashes: &[&str],
) -> Result<HashMap<String, ChartListItem>> {
    debug_assert!(matches!(column, "md5" | "sha256"));
    let mut map = HashMap::new();
    if hashes.is_empty() {
        return Ok(map);
    }

    let mut unique_hashes = hashes.to_vec();
    unique_hashes.sort_unstable();
    unique_hashes.dedup();

    for chunk in unique_hashes.chunks(CHART_HASH_LOOKUP_BATCH_SIZE) {
        let placeholders = std::iter::repeat_n("?", chunk.len()).collect::<Vec<_>>().join(", ");
        let sql = format!(
            "SELECT {CHART_LIST_ITEM_COLUMNS_C}, latest.lookup_hash
             FROM charts c
             JOIN (
                 SELECT {column} AS lookup_hash, MAX(id) AS chart_id
                 FROM charts
                 WHERE {column} IN ({placeholders})
                 GROUP BY {column}
             ) latest ON latest.chart_id = c.id"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(chunk.iter().copied()), |row| {
            Ok((row.get::<_, String>("lookup_hash")?, chart_list_item_from_row(row)?))
        })?;
        for row in rows {
            let (hash, chart) = row?;
            map.insert(hash, chart);
        }
    }
    Ok(map)
}

pub(super) const CHART_LIST_ITEM_COLUMNS: &str = "
    id,
    md5,
    sha256,
    title,
    subtitle,
    artist,
    difficulty_name,
    play_level,
    mode,
    total_notes,
    initial_bpm,
    COALESCE(min_bpm, initial_bpm),
    COALESCE(max_bpm, initial_bpm),
    length_ms,
    folder_path,
    stage_file,
    banner_file,
    backbmp_file,
    preview_file,
    COALESCE(has_document, 0),
    has_long_notes,
    has_mines,
    judge_rank,
    has_undefined_ln,
    has_defined_ln,
    has_defined_cn,
    has_defined_hcn,
    subartist,
    genre,
    COALESCE(bms_total, 0),
    undefined_ln_pairs,
    defined_ln_pairs,
    defined_cn_pairs,
    defined_hcn_pairs,
    has_bga,
    has_bms_random,
    has_defined_hln,
    defined_hln_pairs";

pub(super) const CHART_LIST_ITEM_COLUMNS_C: &str = "
    c.id,
    c.md5,
    c.sha256,
    c.title,
    c.subtitle,
    c.artist,
    c.difficulty_name,
    c.play_level,
    c.mode,
    c.total_notes,
    c.initial_bpm,
    COALESCE(c.min_bpm, c.initial_bpm),
    COALESCE(c.max_bpm, c.initial_bpm),
    c.length_ms,
    c.folder_path,
    c.stage_file,
    c.banner_file,
    c.backbmp_file,
    c.preview_file,
    COALESCE(c.has_document, 0),
    c.has_long_notes,
    c.has_mines,
    c.judge_rank,
    c.has_undefined_ln,
    c.has_defined_ln,
    c.has_defined_cn,
    c.has_defined_hcn,
    c.subartist,
    c.genre,
    COALESCE(c.bms_total, 0),
    c.undefined_ln_pairs,
    c.defined_ln_pairs,
    c.defined_cn_pairs,
    c.defined_hcn_pairs,
    c.has_bga,
    c.has_bms_random,
    c.has_defined_hln,
    c.defined_hln_pairs";

pub(super) fn chart_list_item_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChartListItem> {
    let md5_hex: String = row.get(1)?;
    let md5 = hex_to_hash::<16>(&md5_hex)?;
    let sha256_hex: String = row.get(2)?;
    let sha256 = hex_to_hash::<32>(&sha256_hex)?;

    Ok(ChartListItem {
        chart_id: row.get(0)?,
        md5,
        sha256,
        title: row.get(3)?,
        subtitle: row.get(4)?,
        artist: row.get(5)?,
        difficulty_name: row.get(6)?,
        play_level: row.get(7)?,
        mode: row.get(8)?,
        total_notes: row.get(9)?,
        initial_bpm: row.get(10)?,
        min_bpm: row.get(11)?,
        max_bpm: row.get(12)?,
        length_ms: row.get(13)?,
        folder_path: row.get(14)?,
        stage_file: row.get(15)?,
        banner_file: row.get(16)?,
        backbmp_file: row.get(17)?,
        preview_file: row.get(18)?,
        has_document: row.get(19)?,
        has_long_notes: row.get(20)?,
        has_mines: row.get(21)?,
        judge_rank: row.get(22)?,
        ln_profile: ChartLnProfile {
            has_undefined_ln: row.get(23)?,
            has_defined_ln: row.get(24)?,
            has_defined_cn: row.get(25)?,
            has_defined_hcn: row.get(26)?,
            has_defined_hln: row.get(36)?,
        },
        subartist: row.get(27)?,
        genre: row.get(28)?,
        bms_total: row.get(29)?,
        ln_counts: ChartLnCounts {
            undefined_ln_pairs: row.get(30)?,
            defined_ln_pairs: row.get(31)?,
            defined_cn_pairs: row.get(32)?,
            defined_hcn_pairs: row.get(33)?,
            defined_hln_pairs: row.get(37)?,
        },
        has_bga: row.get(34)?,
        has_bms_random: row.get(35)?,
    })
}

pub(super) fn chart_bms_total(metadata_total: Option<f64>) -> f64 {
    metadata_total.unwrap_or(0.0)
}

pub(super) fn upsert_chart_file(conn: &Connection, record: &ChartImportRecord<'_>) -> Result<i64> {
    conn.prepare_cached(
        "INSERT INTO chart_files (
            root_id,
            path,
            file_size,
            modified_at,
            md5,
            sha256,
            scanned_at,
            first_seen_at,
            parse_status
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, 'Parsed')
        ON CONFLICT(path) DO UPDATE SET
            root_id = excluded.root_id,
            file_size = excluded.file_size,
            modified_at = excluded.modified_at,
            md5 = excluded.md5,
            sha256 = excluded.sha256,
            scanned_at = excluded.scanned_at,
            parse_status = excluded.parse_status
        RETURNING id",
    )?
    .query_row(
        params![
            record.root_id,
            path_key(record.file_path),
            record.file_size as i64,
            record.modified_at,
            hash_to_hex(&record.chart.identity.file_md5),
            hash_to_hex(&record.chart.identity.file_sha256),
            record.scanned_at,
        ],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn insert_chart(conn: &Connection, record: &ChartImportRecord<'_>) -> Result<i64> {
    let chart = record.chart;
    let stats = ChartStats::from_chart(chart);
    conn.prepare_cached(
        "INSERT INTO charts (
            sha256, md5, title, subtitle, artist, subartist, genre,
            difficulty_name, play_level, mode, total_notes, initial_bpm,
            min_bpm, max_bpm, length_ms, ln_type, has_bga, has_long_notes,
            has_mines, folder_path, stage_file, preview_file,
            banner_file, backbmp_file, judge_rank, gauge_total, bms_total,
            has_undefined_ln, has_defined_ln, has_defined_cn, has_defined_hcn,
            undefined_ln_pairs, defined_ln_pairs, defined_cn_pairs, defined_hcn_pairs,
            has_bms_random, source_url, append_url, headers_json, import_version,
            has_defined_hln, defined_hln_pairs
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
            ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27,
            ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35, ?36, ?37, ?38, ?39, ?40, ?41, ?42
        )",
    )?
    .execute(params![
        hash_to_hex(&chart.identity.file_sha256),
        hash_to_hex(&chart.identity.file_md5),
        chart.metadata.title.as_str(),
        chart.metadata.subtitle.as_str(),
        chart.metadata.artist.as_str(),
        chart.metadata.subartist.as_str(),
        chart.metadata.genre.as_str(),
        chart.metadata.difficulty_name.as_str(),
        chart.metadata.play_level.as_str(),
        chart.metadata.key_mode.as_str(),
        chart.total_notes,
        chart.metadata.initial_bpm,
        stats.min_bpm,
        stats.max_bpm,
        chart.end_time.0 / 1_000,
        stats.ln_type,
        chart.metadata.has_bga,
        stats.has_long_notes,
        stats.has_mines,
        folder_path(record.file_path),
        chart.metadata.stage_file.as_str(),
        chart.metadata.preview_file.as_str(),
        chart.metadata.banner_file.as_str(),
        chart.metadata.backbmp_file.as_str(),
        chart.metadata.judge_rank,
        gauge_total_for_chart(
            chart.metadata.total,
            stats.ln_counts.canonical_total_notes(chart.total_notes),
        ),
        chart_bms_total(chart.metadata.total),
        stats.ln_profile.has_undefined_ln,
        stats.ln_profile.has_defined_ln,
        stats.ln_profile.has_defined_cn,
        stats.ln_profile.has_defined_hcn,
        stats.ln_counts.undefined_ln_pairs,
        stats.ln_counts.defined_ln_pairs,
        stats.ln_counts.defined_cn_pairs,
        stats.ln_counts.defined_hcn_pairs,
        chart.metadata.has_bms_random,
        chart.metadata.source_url.as_str(),
        chart.metadata.append_url.as_str(),
        chart_headers_json(),
        CHART_IMPORT_VERSION,
        stats.ln_profile.has_defined_hln,
        stats.ln_counts.defined_hln_pairs,
    ])?;
    Ok(conn.last_insert_rowid())
}

pub(super) fn chart_headers_json() -> &'static str {
    // Header values needed by the library have dedicated columns.  Do not
    // persist the raw header map: some BMS channel identifiers use Base62 and
    // were historically mistaken for headers, retaining complete note lines.
    "{}"
}

pub(super) fn update_chart(
    conn: &Connection,
    chart_id: i64,
    record: &ChartImportRecord<'_>,
) -> Result<()> {
    let chart = record.chart;
    let stats = ChartStats::from_chart(chart);
    conn.prepare_cached(
        "UPDATE charts SET
            sha256 = ?1, md5 = ?2, title = ?3, subtitle = ?4, artist = ?5,
            subartist = ?6, genre = ?7, difficulty_name = ?8, play_level = ?9,
            mode = ?10, total_notes = ?11, initial_bpm = ?12, min_bpm = ?13, max_bpm = ?14,
            length_ms = ?15, ln_type = ?16, has_bga = ?17, has_long_notes = ?18,
            has_mines = ?19, folder_path = ?20, stage_file = ?21, preview_file = ?22,
            banner_file = ?23, backbmp_file = ?24, judge_rank = ?25, gauge_total = ?26,
            bms_total = ?27, has_undefined_ln = ?28, has_defined_ln = ?29,
            has_defined_cn = ?30, has_defined_hcn = ?31,
            undefined_ln_pairs = ?32, defined_ln_pairs = ?33,
            defined_cn_pairs = ?34, defined_hcn_pairs = ?35, has_bms_random = ?36,
            source_url = ?37, append_url = ?38, headers_json = ?39,
            import_version = ?40, has_defined_hln = ?42, defined_hln_pairs = ?43
         WHERE id = ?41",
    )?
    .execute(params![
        hash_to_hex(&chart.identity.file_sha256),
        hash_to_hex(&chart.identity.file_md5),
        chart.metadata.title.as_str(),
        chart.metadata.subtitle.as_str(),
        chart.metadata.artist.as_str(),
        chart.metadata.subartist.as_str(),
        chart.metadata.genre.as_str(),
        chart.metadata.difficulty_name.as_str(),
        chart.metadata.play_level.as_str(),
        chart.metadata.key_mode.as_str(),
        chart.total_notes,
        chart.metadata.initial_bpm,
        stats.min_bpm,
        stats.max_bpm,
        chart.end_time.0 / 1_000,
        stats.ln_type,
        chart.metadata.has_bga,
        stats.has_long_notes,
        stats.has_mines,
        folder_path(record.file_path),
        chart.metadata.stage_file.as_str(),
        chart.metadata.preview_file.as_str(),
        chart.metadata.banner_file.as_str(),
        chart.metadata.backbmp_file.as_str(),
        chart.metadata.judge_rank,
        gauge_total_for_chart(
            chart.metadata.total,
            stats.ln_counts.canonical_total_notes(chart.total_notes),
        ),
        chart_bms_total(chart.metadata.total),
        stats.ln_profile.has_undefined_ln,
        stats.ln_profile.has_defined_ln,
        stats.ln_profile.has_defined_cn,
        stats.ln_profile.has_defined_hcn,
        stats.ln_counts.undefined_ln_pairs,
        stats.ln_counts.defined_ln_pairs,
        stats.ln_counts.defined_cn_pairs,
        stats.ln_counts.defined_hcn_pairs,
        chart.metadata.has_bms_random,
        chart.metadata.source_url.as_str(),
        chart.metadata.append_url.as_str(),
        chart_headers_json(),
        CHART_IMPORT_VERSION,
        chart_id,
        stats.ln_profile.has_defined_hln,
        stats.ln_counts.defined_hln_pairs,
    ])?;
    Ok(())
}

pub(super) fn write_chart_analysis(
    conn: &Connection,
    chart_id: i64,
    chart: &PlayableChart,
) -> Result<()> {
    let analysis = ChartAnalysis::from_chart(chart);
    conn.prepare_cached(
        "INSERT INTO chart_analysis (
            chart_id, normal_notes, long_notes, scratch_notes, long_scratch_notes,
            density, peak_density, end_density, total_gauge, main_bpm,
            distribution_json, speed_changes_json, lane_notes_json, analysis_version
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14
        )
        ON CONFLICT(chart_id) DO UPDATE SET
            normal_notes = excluded.normal_notes,
            long_notes = excluded.long_notes,
            scratch_notes = excluded.scratch_notes,
            long_scratch_notes = excluded.long_scratch_notes,
            density = excluded.density,
            peak_density = excluded.peak_density,
            end_density = excluded.end_density,
            total_gauge = excluded.total_gauge,
            main_bpm = excluded.main_bpm,
            distribution_json = excluded.distribution_json,
            speed_changes_json = excluded.speed_changes_json,
            lane_notes_json = excluded.lane_notes_json,
            loudness_lufs = NULL,
            loudness_analysis_version = 0,
            analysis_version = excluded.analysis_version",
    )?
    .execute(params![
        chart_id,
        analysis.normal_notes,
        analysis.long_notes,
        analysis.scratch_notes,
        analysis.long_scratch_notes,
        analysis.density,
        analysis.peak_density,
        analysis.end_density,
        analysis.total_gauge,
        analysis.main_bpm,
        encode_distribution_compact(&analysis.distribution),
        serde_json::to_string(&analysis.speed_changes)?,
        serde_json::to_string(&analysis.lane_notes)?,
        CHART_IMPORT_VERSION,
    ])?;
    Ok(())
}
