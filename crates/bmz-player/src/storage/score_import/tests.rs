use std::collections::HashMap;
use std::path::Path;

use bmz_chart::hash::compute_chart_identity;
use bmz_chart::model::{ChartMetadata, LongNotePair, LongNoteStyle, PlayableChart};
use bmz_core::ids::NoteId;
use bmz_core::lane::Lane;
use bmz_core::time::{ChartTick, TimeUs};
use rusqlite::params;

use super::*;
#[path = "tests/regressions.rs"]
mod regressions;
use crate::select_options::DoubleOptionScoreBucket;
use crate::storage::common::hash_to_hex;
use crate::storage::library_db::{ChartImportRecord, LibraryDatabase};
use crate::storage::migration::{LIBRARY_MIGRATIONS, SCORE_MIGRATIONS, run_migrations};
use crate::storage::score_db::ScoreKey;
use bmz_gameplay::rule::RuleMode;

fn open_test_databases() -> (LibraryDatabase, ScoreDatabase, [u8; 32], [u8; 16]) {
    open_test_databases_with_chart(chart())
}

fn open_test_databases_with_chart(
    chart: PlayableChart,
) -> (LibraryDatabase, ScoreDatabase, [u8; 32], [u8; 16]) {
    let mut library_conn = Connection::open_in_memory().unwrap();
    super::super::common::configure_connection(&library_conn).unwrap();
    run_migrations(&mut library_conn, LIBRARY_MIGRATIONS).unwrap();
    let mut library_db = LibraryDatabase::from_connection(library_conn);
    let sha256 = chart.identity.file_sha256;
    let md5 = chart.identity.file_md5;
    library_db
        .upsert_chart_import(&ChartImportRecord {
            root_id: None,
            file_path: Path::new("/songs/import.bms"),
            file_size: 10,
            modified_at: 1,
            scanned_at: 1,
            chart: &chart,
        })
        .unwrap();

    let mut score_conn = Connection::open_in_memory().unwrap();
    super::super::common::configure_connection(&score_conn).unwrap();
    run_migrations(&mut score_conn, SCORE_MIGRATIONS).unwrap();
    (library_db, ScoreDatabase::from_connection(score_conn), sha256, md5)
}

fn chart() -> PlayableChart {
    let mut chart = PlayableChart {
        identity: compute_chart_identity(b"score import test"),
        metadata: ChartMetadata {
            title: "Import Target".to_string(),
            artist: "artist".to_string(),
            initial_bpm: 128.0,
            ..Default::default()
        },
        lane_notes: std::array::from_fn(|_| Vec::new()),
        long_notes: Vec::new(),
        bgm_events: Vec::new(),
        bga_events: Vec::new(),
        timing_events: Vec::new(),
        scroll_events: Vec::new(),
        speed_events: Vec::new(),
        judge_rank_events: Vec::new(),
        bgm_volume_events: Vec::new(),
        key_volume_events: Vec::new(),
        text_events: Vec::new(),
        bga_opacity_events: Vec::new(),
        bga_argb_events: Vec::new(),
        swbga_definitions: Vec::new(),
        bga_keybound_events: Vec::new(),
        bga_asset_by_bmp_key: HashMap::new(),
        bar_lines: Vec::new(),
        sounds: Vec::new(),
        bga_assets: Vec::new(),
        total_notes: 128,
        end_time: TimeUs(10_000_000),
    };
    chart.identity.file_md5 = [1; 16];
    chart.identity.file_sha256 = [2; 32];
    chart
}

fn undefined_ln_chart(total_notes: u32, long_pairs: u32) -> PlayableChart {
    let mut chart = chart();
    chart.total_notes = total_notes;
    chart.long_notes = (0..long_pairs)
        .map(|index| LongNotePair {
            lane: Lane::Key1,
            style: LongNoteStyle::ChannelPair,
            mode: None,
            start_note_id: NoteId(index * 2 + 1),
            end_note_id: NoteId(index * 2 + 2),
            start_tick: ChartTick(0),
            end_tick: ChartTick(192),
            start_time: TimeUs(0),
            end_time: TimeUs(1_000_000),
            sound: None,
        })
        .collect();
    chart
}

fn create_lr2_source(conn: &Connection, md5: &[u8; 16]) {
    create_lr2_source_with_hash(conn, &hash_to_hex(md5));
}

fn create_lr2_source_with_hash(conn: &Connection, hash: &str) {
    // `poor` includes Empty Poor in LR2 and may make the judge sum exceed totalnotes.
    create_lr2_source_with_score(
        conn,
        hash,
        Lr2ScoreFixture {
            total_notes: 128,
            max_combo: 64,
            perfect: 100,
            great: 22,
            good: 3,
            bad: 2,
            poor: 10,
        },
    );
}

#[derive(Debug, Clone, Copy)]
struct Lr2ScoreFixture {
    total_notes: u32,
    max_combo: u32,
    perfect: u32,
    great: u32,
    good: u32,
    bad: u32,
    poor: u32,
}

fn create_lr2_source_with_score(conn: &Connection, hash: &str, score: Lr2ScoreFixture) {
    conn.execute_batch(
        "CREATE TABLE score (
                hash TEXT, clear INTEGER, perfect INTEGER, great INTEGER,
                good INTEGER, bad INTEGER, poor INTEGER, totalnotes INTEGER,
                maxcombo INTEGER, minbp INTEGER, playcount INTEGER, clearcount INTEGER,
                ghost TEXT, rseed INTEGER, op_best INTEGER
            );",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO score VALUES (?1, 4, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 3, 2, 1, '', 123, 0)",
        params![
            hash,
            score.perfect,
            score.great,
            score.good,
            score.bad,
            score.poor,
            score.total_notes,
            score.max_combo
        ],
    )
    .unwrap();
}

fn create_beatoraja_source(conn: &Connection, sha256: &[u8; 32], date: i64, mode: i64) {
    create_beatoraja_source_with_sha256(conn, &hash_to_hex(sha256), date, mode);
}

fn create_beatoraja_source_with_sha256(conn: &Connection, sha256: &str, date: i64, mode: i64) {
    // Default no-LN chart expects 128 scored notes.
    create_beatoraja_source_with_score(
        conn,
        sha256,
        BeatorajaScoreFixture {
            date,
            mode,
            clear: 7,
            total_notes: 128,
            judged: 128,
            max_combo: 80,
        },
    );
}

#[derive(Debug, Clone, Copy)]
struct BeatorajaScoreFixture {
    date: i64,
    mode: i64,
    clear: i64,
    total_notes: u32,
    judged: u32,
    max_combo: u32,
}

fn create_beatoraja_source_with_score(
    conn: &Connection,
    sha256: &str,
    score: BeatorajaScoreFixture,
) {
    // Split judged across fast/slow buckets for schema coverage; empty poor
    // (ems/lms) is excluded from the import note-count check.
    let epg = score.judged.saturating_sub(28).min(score.judged);
    let rem = score.judged.saturating_sub(epg);
    let lpg = rem.min(10);
    let rem = rem.saturating_sub(lpg);
    let egr = rem.min(5);
    let rem = rem.saturating_sub(egr);
    let lgr = rem.min(3);
    let rem = rem.saturating_sub(lgr);
    let egd = rem.min(2);
    let rem = rem.saturating_sub(egd);
    let lgd = rem.min(1);
    let rem = rem.saturating_sub(lgd);
    let ebd = rem.min(2);
    let rem = rem.saturating_sub(ebd);
    let lbd = rem.min(1);
    let rem = rem.saturating_sub(lbd);
    let epr = rem.min(3);
    let lpr = rem.saturating_sub(epr);

    if conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='score'",
            [],
            |_| Ok(()),
        )
        .is_err()
    {
        conn.execute_batch(
            "CREATE TABLE score (
                    sha256 TEXT, mode INTEGER, clear INTEGER, epg INTEGER, lpg INTEGER,
                    egr INTEGER, lgr INTEGER, egd INTEGER, lgd INTEGER,
                    ebd INTEGER, lbd INTEGER, epr INTEGER, lpr INTEGER,
                    ems INTEGER, lms INTEGER, notes INTEGER, combo INTEGER,
                    minbp INTEGER, ghost TEXT, seed INTEGER, date INTEGER, option INTEGER
                );",
        )
        .unwrap();
    }
    conn.execute(
            "INSERT INTO score VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 3, 1, ?14, ?15, 2, '', 456, ?16, 0
            )",
            params![
                sha256,
                score.mode,
                score.clear,
                epg,
                lpg,
                egr,
                lgr,
                egd,
                lgd,
                ebd,
                lbd,
                epr,
                lpr,
                score.total_notes,
                score.max_combo,
                score.date
            ],
        )
        .unwrap();
}

#[path = "tests/cases_01.rs"]
mod cases_01;
#[path = "tests/cases_02.rs"]
mod cases_02;
