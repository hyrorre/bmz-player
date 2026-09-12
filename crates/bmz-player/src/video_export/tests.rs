use super::*;
use crate::config::profile_config::ProfileConfig;
use std::fs;

struct Fixture {
    root: PathBuf,
    paths: AppPaths,
    options: VideoExportOptions,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("bmz-export-test-{}", random_id().unwrap()));
        fs::create_dir_all(root.join("profiles/default")).unwrap();
        let resources =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data").canonicalize().unwrap();
        let paths =
            AppPaths::from_dirs(resources, root.clone(), root.join("cache"), root.join("logs"));
        let mut profile = ProfileConfig::new_default("default", "Export Test", 0);
        profile.skin = Default::default();
        profile.system_sound.bgm_dir.clear();
        profile.system_sound.se_dir.clear();
        profile.system_sound.default_sound_dir.clear();
        fs::write(root.join("profiles/default/profile.toml"), toml::to_string(&profile).unwrap())
            .unwrap();
        fs::write(root.join("chart.bms"), "#TITLE Offline fixture\n#BPM 240\n#TOTAL 1000\n#WAV01 click.wav\n#STOP01 48\n#BPM01 180\n#00011:00010000\n#00051:00000101\n#00009:00010000\n#00008:00000100\n#00112:0100\n").unwrap();
        let mut wav = Vec::new();
        wav.extend(b"RIFF");
        wav.extend(40_u32.to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16_u32.to_le_bytes());
        wav.extend(1_u16.to_le_bytes());
        wav.extend(2_u16.to_le_bytes());
        wav.extend(48000_u32.to_le_bytes());
        wav.extend(192000_u32.to_le_bytes());
        wav.extend(4_u16.to_le_bytes());
        wav.extend(16_u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(4_u32.to_le_bytes());
        wav.extend(16000_i16.to_le_bytes());
        wav.extend(16000_i16.to_le_bytes());
        fs::write(root.join("click.wav"), wav).unwrap();
        let options = VideoExportOptions {
            chart: root.join("chart.bms"),
            output: root.join("out.mp4"),
            width: 320,
            height: 180,
            fps: crate::cli::FrameRate::parse("60").unwrap(),
            replay_slot: None,
            overwrite: false,
            ffmpeg: std::env::var_os("BMZ_TEST_FFMPEG")
                .map(PathBuf::from)
                .unwrap_or_else(|| "ffmpeg".into()),
            seed: Some(42),
        };
        Self { root, paths, options }
    }
    fn simulation(&self) -> simulation::Simulation {
        let prepared = prepare::prepare(&self.options, &self.paths, Some("default")).unwrap();
        let engine =
            bmz_audio::command::AudioEngineHandle::new(bmz_audio::engine::AudioEngine::new(48000));
        let document =
            serde_json::from_str("{\"type\":0,\"name\":\"test\",\"w\":320,\"h\":180}").unwrap();
        let skin_audio =
            crate::skin_audio::SkinAudioRuntime::install(engine.clone(), &document, Vec::new());
        simulation::Simulation::new(
            prepared,
            simulation::SceneTiming {
                ready_us: 200_000,
                chart_zero_us: 400_000,
                finishmargin_us: 100_000,
                close_us: 200_000,
            },
            300,
            0,
            engine,
            skin_audio,
            &self.paths,
        )
        .unwrap()
    }
    fn save_replay(&mut self) {
        use crate::storage::{
            migration::{SCORE_MIGRATIONS, run_migrations},
            replay::ReplayFile,
            score_db::{ReplaySlotRecord, ScoreDatabase, ScoreSourceKind},
        };
        use bmz_core::{input::InputKind, replay::ReplayEvent};
        let prepared = prepare::prepare(&self.options, &self.paths, Some("default")).unwrap();
        let key = prepared.play.score_key;
        let chart = &prepared.play.session.chart;
        let mut events = Vec::new();
        for lane in bmz_core::lane::Lane::ALL {
            for note in chart.notes_for_lane(lane) {
                use bmz_chart::model::NoteKind;
                match note.kind {
                    NoteKind::Tap => {
                        events.push(ReplayEvent {
                            lane,
                            kind: InputKind::Press,
                            time: note.time,
                            device_kind: Default::default(),
                            scratch_direction: None,
                        });
                        events.push(ReplayEvent {
                            lane,
                            kind: InputKind::Release,
                            time: TimeUs(note.time.0 + 10000),
                            device_kind: Default::default(),
                            scratch_direction: None,
                        });
                    }
                    NoteKind::LongStart | NoteKind::LongEnd => events.push(ReplayEvent {
                        lane,
                        kind: if note.kind == NoteKind::LongStart {
                            InputKind::Press
                        } else {
                            InputKind::Release
                        },
                        time: note.time,
                        device_kind: Default::default(),
                        scratch_direction: None,
                    }),
                    _ => {}
                }
            }
        }
        let arrange = &prepared.play.applied_arrange;
        let file = ReplayFile::new_with_policy(
            chart.identity.file_sha256,
            key.ln_policy,
            key.double_option,
            0,
            None,
            arrange.arrange,
            arrange.arrange_2p,
            arrange.seed,
            arrange.pattern.clone(),
            events,
        )
        .with_randomization(
            arrange.seed_2p,
            arrange.bms_random_choices.clone(),
            arrange.bms_switch_choices.clone(),
        );
        let replay_root = self.root.join("profiles/default/replay");
        fs::create_dir_all(&replay_root).unwrap();
        crate::storage::replay::save_replay(&replay_root.join("fixture.json"), &file).unwrap();
        let mut db = ScoreDatabase::open(&self.root.join("profiles/default/score.db")).unwrap();
        run_migrations(db.conn_mut(), SCORE_MIGRATIONS).unwrap();
        db.upsert_replay_slot(&ReplaySlotRecord {
            chart_sha256: chart.identity.file_sha256,
            ln_policy: key.ln_policy,
            double_option: key.double_option,
            rule_mode: key.rule_mode,
            slot: 1,
            rule: crate::config::profile_config::ReplaySlotRule::Always,
            replay_path: "replay/fixture.json".into(),
            played_at: 0,
            ex_score: None,
            bp: None,
            cb: None,
            max_combo: None,
            clear_rank: None,
            source_kind: ScoreSourceKind::Local,
            source_path: String::new(),
            source_fingerprint: String::new(),
        })
        .unwrap();
        self.options.replay_slot = Some(1);
        self.options.seed = None;
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn fps_and_consumer_stalls_do_not_change_pcm_or_judgements() {
    let fixture = Fixture::new();
    let profile_before = fs::read(fixture.root.join("profiles/default/profile.toml")).unwrap();
    let mut reference = None;
    for fps in [30, 60, 120] {
        let mut simulation = fixture.simulation();
        let mut pcm = Vec::new();
        for frame in 0..=fps * 12 {
            simulation
                .advance_to(frame * 48000 / fps, |block| {
                    pcm.extend_from_slice(block);
                    Ok(())
                })
                .unwrap();
            let snapshot =
                simulation.snapshot((frame * 1_000_000 / fps) as i64, &Default::default());
            if frame == 0 {
                assert_eq!(snapshot.ready_elapsed_time, None);
            }
            if frame == fps {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            simulation.acknowledge_frame();
        }
        assert!(pcm.iter().any(|value| value.abs() > 0.1), "fixture must generate audible PCM");
        let signature =
            (pcm, format!("{:?}", simulation.prepared.play.session.score), simulation.exit_us);
        if let Some(reference) = &reference {
            assert_eq!(reference, &signature);
        } else {
            reference = Some(signature);
        }
        assert!(!fixture.paths.library_db.exists());
        assert!(!fixture.root.join("profiles/default/score.db").exists());
    }
    assert_eq!(
        profile_before,
        fs::read(fixture.root.join("profiles/default/profile.toml")).unwrap()
    );
}

#[test]
fn scene_timing_includes_entry_and_exit_animation() {
    let fixture = Fixture::new();
    let mut simulation = fixture.simulation();
    simulation.advance_to(4800, |_| Ok(())).unwrap();
    assert!(simulation.snapshot(100000, &Default::default()).ready_elapsed_time.is_none());
    simulation.advance_to(9600, |_| Ok(())).unwrap();
    assert_eq!(
        simulation.snapshot(200000, &Default::default()).ready_elapsed_time,
        Some(TimeUs(0))
    );
    simulation.advance_to(48000 * 12, |_| Ok(())).unwrap();
    let end = simulation.exit_us.expect("must finish");
    let fading = simulation.snapshot(end - 100000, &Default::default());
    assert_eq!(fading.fadeout_elapsed_ms, Some(200));
}

#[test]
fn saved_replay_reproduces_judgements_without_modifying_score_database() {
    let mut fixture = Fixture::new();
    fixture.save_replay();
    let score_path = fixture.root.join("profiles/default/score.db");
    let before = fs::read(&score_path).unwrap();
    let mut reference = None;
    for fps in [30, 120] {
        let mut simulation = fixture.simulation();
        assert!(simulation.prepared.play.session.replay_player.is_some());
        let mut pcm = Vec::new();
        for frame in 0..=fps * 12 {
            simulation
                .advance_to(frame * 48000 / fps, |block| {
                    pcm.extend_from_slice(block);
                    Ok(())
                })
                .unwrap();
            simulation.acknowledge_frame();
        }
        let expected =
            bmz_gameplay::score::scored_note_count(&simulation.prepared.play.session.chart) * 2;
        assert_eq!(simulation.prepared.play.session.score.ex_score(), expected);
        let result = (pcm, format!("{:?}", simulation.prepared.play.session.score));
        if let Some(reference) = &reference {
            assert_eq!(reference, &result);
        } else {
            reference = Some(result);
        }
    }
    assert_eq!(before, fs::read(score_path).unwrap());
}

#[test]
#[ignore = "requires a working GPU and BMZ_TEST_FFMPEG with libx264/AAC"]
fn generates_complete_mp4_and_metadata_without_persistence() {
    let fixture = Fixture::new();
    // Exercise actual asynchronous BGA decoding, including one movie bound to
    // two layers with different start times.
    let bga = encoder::command(&fixture.options.ffmpeg)
        .args([
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=64x64:rate=30",
            "-t",
            "0.5",
            "-an",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(fixture.root.join("bga.mp4"))
        .output()
        .unwrap();
    assert!(bga.status.success(), "{}", String::from_utf8_lossy(&bga.stderr));
    use std::io::Write;
    let mut chart_file = fs::OpenOptions::new().append(true).open(&fixture.options.chart).unwrap();
    writeln!(chart_file, "#BMP01 bga.mp4\n#00004:01\n#00007:0001").unwrap();
    drop(chart_file);
    run(fixture.options.clone(), &fixture.paths, Some("default")).unwrap();
    let metadata: serde_json::Value = serde_json::from_slice(
        &fs::read(fixture.options.output.with_extension("export.json")).unwrap(),
    )
    .unwrap();
    assert!(metadata["frames"].as_u64().unwrap() > 60);
    assert_eq!(
        metadata["audio_samples"].as_u64().unwrap(),
        metadata["frames"].as_u64().unwrap() * 800
    );
    assert!(!fixture.paths.library_db.exists());
    assert!(!fixture.root.join("profiles/default/score.db").exists());
    let decoded = encoder::command(&fixture.options.ffmpeg)
        .arg("-i")
        .arg(&fixture.options.output)
        .args(["-f", "null", "-"])
        .output()
        .unwrap();
    assert!(decoded.status.success(), "{}", String::from_utf8_lossy(&decoded.stderr));
    let probe = fixture.options.ffmpeg.with_file_name(if cfg!(windows) {
        "ffprobe.exe"
    } else {
        "ffprobe"
    });
    let streams = std::process::Command::new(probe)
        .args(["-v", "error", "-count_frames", "-show_streams", "-of", "json"])
        .arg(&fixture.options.output)
        .output()
        .unwrap();
    assert!(streams.status.success());
    let streams: serde_json::Value = serde_json::from_slice(&streams.stdout).unwrap();
    let video = &streams["streams"][0];
    let audio = &streams["streams"][1];
    assert_eq!(
        video["nb_read_frames"].as_str().unwrap().parse::<u64>().unwrap(),
        metadata["frames"].as_u64().unwrap()
    );
    assert_eq!(video["r_frame_rate"], "60/1");
    assert_eq!(audio["sample_rate"], "48000");
    assert_eq!(video["start_time"], "0.000000");
    assert_eq!(audio["start_time"], "0.000000");
    let decoded_audio = encoder::command(&fixture.options.ffmpeg)
        .arg("-i")
        .arg(&fixture.options.output)
        .args(["-vn", "-f", "f32le", "-acodec", "pcm_f32le", "pipe:1"])
        .output()
        .unwrap();
    assert!(decoded_audio.status.success());
    let pcm = decoded_audio
        .stdout
        .chunks_exact(8)
        .map(|sample| f32::from_le_bytes(sample[..4].try_into().unwrap()))
        .collect::<Vec<_>>();
    let prepared = prepare::prepare(&fixture.options, &fixture.paths, Some("default")).unwrap();
    let first_note = prepared
        .play
        .session
        .chart
        .lane_notes
        .iter()
        .flatten()
        .filter(|n| n.sound.is_some())
        .map(|n| n.time.0)
        .min()
        .unwrap();
    let doc: bmz_render::skin::SkinDocument = serde_json::from_str("{}").unwrap();
    let start = simulation::SceneTiming::from_document(&doc).chart_zero_us;
    let expected_sample = ((first_note + start) as u64 * 48000 / 1_000_000) as usize;
    let peak = pcm
        .iter()
        .enumerate()
        .filter(|(index, _)| index.abs_diff(expected_sample) < 200)
        .max_by(|(_, a), (_, b)| a.abs().total_cmp(&b.abs()))
        .unwrap()
        .0;
    assert!(
        peak.abs_diff(expected_sample) <= 2,
        "AAC must compensate encoder delay: expected {expected_sample}, peak {peak}"
    );
    let before = fs::read(&fixture.options.output).unwrap();
    assert!(run(fixture.options.clone(), &fixture.paths, Some("default")).is_err());
    assert_eq!(before, fs::read(&fixture.options.output).unwrap());
    if let Some(directory) = std::env::var_os("BMZ_TEST_EXPORT_DIR") {
        let directory = PathBuf::from(directory);
        fs::create_dir_all(&directory).unwrap();
        fs::copy(&fixture.options.output, directory.join("sample.mp4")).unwrap();
        fs::copy(
            fixture.options.output.with_extension("export.json"),
            directory.join("sample.export.json"),
        )
        .unwrap();
    }
}
