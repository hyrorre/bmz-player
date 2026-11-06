use std::sync::Arc;

use crate::mixer::{ActiveVoiceFadeOut, MixerState};
use crate::queue::{AudioScheduler, RestartPolicy, ScheduledSound, ScheduledSoundQueue};
use crate::sample::{DecodedSample, SampleBank};

#[derive(Debug, Default)]
pub struct AudioEngine {
    pub queue: ScheduledSoundQueue,
    pub mixer: MixerState,
    pub samples: SampleBank,
    paused_at_output_frame: Option<u64>,
    paused_at_time: Option<std::time::Instant>,
}

impl AudioEngine {
    pub fn new(output_sample_rate: u32) -> Self {
        Self {
            queue: ScheduledSoundQueue::new(),
            mixer: MixerState::new(output_sample_rate),
            samples: SampleBank::default(),
            paused_at_output_frame: None,
            paused_at_time: None,
        }
    }

    pub fn with_sample_bank(output_sample_rate: u32, mut samples: SampleBank) -> Self {
        samples.resample_all_to(output_sample_rate);
        Self {
            queue: ScheduledSoundQueue::new(),
            mixer: MixerState::new(output_sample_rate),
            samples,
            paused_at_output_frame: None,
            paused_at_time: None,
        }
    }

    pub fn insert_sample(&mut self, id: bmz_core::ids::SoundId, sample: DecodedSample) {
        // 読込時に出力レートへ揃えておき、ミキサーでの逐次リサンプルを避ける。
        // 一致済みのPCMはそのままSampleBankへ移動し、不要なframes cloneを避ける。
        let sample = if sample.sample_rate == self.mixer.output_sample_rate {
            sample
        } else {
            sample.resampled_to(self.mixer.output_sample_rate)
        };
        self.samples.insert(id, sample);
    }

    pub fn output_sample_rate(&self) -> u32 {
        self.mixer.output_sample_rate
    }

    pub fn reserve_sample_slot(&mut self, id: bmz_core::ids::SoundId) {
        self.samples.reserve_slot(id);
    }

    pub fn insert_prepared_sample(&mut self, id: bmz_core::ids::SoundId, sample: DecodedSample) {
        let sample = if sample.sample_rate == self.mixer.output_sample_rate {
            sample
        } else {
            sample.resampled_to(self.mixer.output_sample_rate)
        };
        self.samples.insert(id, sample);
    }

    /// 読込・VOLWAV適用・出力レート化済みの共有PCMの一部を登録する。
    /// BMSON `sound_channel` の slice を複数 `SoundId` から再生するために使う。
    pub fn insert_shared_sample_region(
        &mut self,
        id: bmz_core::ids::SoundId,
        source: Arc<DecodedSample>,
        start_frame: usize,
        end_frame: usize,
    ) {
        self.samples.insert_shared_region(id, source, start_frame, end_frame);
    }

    /// 出力サンプルレートを変更し、保持中の全サンプルを新レートへ揃える。
    /// cpal ストリームへ source 登録される際(実レート確定時)に呼ばれる。
    pub fn set_output_sample_rate(&mut self, rate: u32) {
        if rate == 0 || rate == self.mixer.output_sample_rate {
            return;
        }
        self.samples.resample_all_to(rate);
        self.mixer.output_sample_rate = rate;
    }

    /// 指定 sound_id のスケジュール済み音および再生中 voice をすべて停止する。
    /// BGM ループの停止等に使う。
    pub fn stop_sound(&mut self, id: bmz_core::ids::SoundId) {
        self.queue.retain(|sound| sound.sound_id != id);
        self.mixer.voices.retain(|voice| voice.sound.sound_id != id);
    }

    /// 指定 sound_id の再生待ち音を破棄し、再生中 voice は短い fade-out で停止する。
    /// プレビュー差し替えのように波形途中の即時停止が pop になりやすい場面で使う。
    pub fn stop_sound_with_fade_out(&mut self, id: bmz_core::ids::SoundId, fade_out_frames: u32) {
        self.queue.retain(|sound| sound.sound_id != id);
        self.mixer.stop_sound_with_fade_out(id, fade_out_frames);
    }

    /// デコード済み sample bank を維持したまま、再生待ちと再生中の音だけを破棄する。
    /// Viewer のシークでは filesystem / decode をやり直さず、同じ譜面音源を新しい
    /// output-frame 基準へ差し替えるために使う。
    pub fn clear_playback(&mut self) {
        self.queue.clear();
        self.mixer.voices.clear();
        self.mixer.master_gain = 1.0;
    }

    /// callbackの出力frameを基準に、予約音と再生中voiceを同じ位置で凍結する。
    /// 再開時は停止中に進んだhardware frame分だけ絶対frameを後ろへずらす。
    pub fn set_playback_paused(&mut self, paused: bool, output_frame: u64) {
        self.set_playback_paused_at(paused, output_frame, std::time::Instant::now());
    }

    pub fn set_playback_paused_at(
        &mut self,
        paused: bool,
        output_frame: u64,
        at: std::time::Instant,
    ) {
        match (paused, self.paused_at_output_frame) {
            (true, None) => {
                self.paused_at_output_frame = Some(output_frame);
                self.paused_at_time = Some(at);
            }
            (false, Some(paused_at)) => {
                let paused_frames = output_frame.saturating_sub(paused_at);
                self.queue.shift_output_frames(paused_frames);
                if let Some(start) = self.paused_at_time.take() {
                    self.queue.shift_times(at.saturating_duration_since(start));
                }
                self.mixer.shift_output_frames(paused_frames);
                self.paused_at_output_frame = None;
            }
            _ => {}
        }
    }

    pub fn playback_paused(&self) -> bool {
        self.paused_at_output_frame.is_some()
    }

    /// 出力全体に掛かるマスターゲインを設定する。リザルト退出時に残響を
    /// フェードアウトさせる用途で、毎フレーム 1.0 → 0.0 へ更新する。
    pub fn set_master_gain(&mut self, gain: f32) {
        self.mixer.master_gain = gain.clamp(0.0, 1.0);
    }

    pub fn set_playback_rate_percent(&mut self, rate: u16) {
        let rate = crate::clock::clamp_playback_rate_percent(rate);
        self.mixer.playback_rate = f64::from(rate) / 100.0;
    }

    pub fn apply_playback_rate_change(&mut self, mut change: crate::clock::PlaybackRateChange) {
        if let Some(paused_at) = self.paused_at_time {
            change.changed_at = Some(paused_at);
            change.anchor_output_frame =
                self.paused_at_output_frame.unwrap_or(change.anchor_output_frame);
        }
        self.queue.apply_playback_rate_change(change);
        self.mixer.apply_playback_rate_change(change);
    }

    /// 指定 sound_id のスケジュール済み音および再生中 voice の音量を更新する。
    pub fn set_sound_volume(&mut self, id: bmz_core::ids::SoundId, volume: f32) {
        let volume = volume.clamp(0.0, 1.0);
        self.queue.set_volume_for_sound(id, volume);
        self.mixer.set_volume_for_sound(id, volume);
    }

    /// `start_frame = 0`(=即時再生)で sound_id を 1 ショット再生する。
    /// 必要なら `loop_playback = true` でループ再生も可能。
    pub fn play_now(&mut self, sound_id: bmz_core::ids::SoundId, volume: f32, loop_playback: bool) {
        self.play_now_with_fade_in(sound_id, volume, loop_playback, 0);
    }

    pub fn play_now_with_fade_in(
        &mut self,
        sound_id: bmz_core::ids::SoundId,
        volume: f32,
        loop_playback: bool,
        fade_in_frames: u32,
    ) {
        self.schedule(ScheduledSound {
            start_time: None,
            start_frame: 0,
            sample_offset_frames: 0,
            sound_id,
            volume: volume.clamp(0.0, 1.0),
            pan: 0.0,
            loop_playback,
            fade_in_frames,
            catch_up: false,
            restart_policy: RestartPolicy::Overlap,
        });
    }

    pub fn play_now_with_fade_in_and_fade_out(
        &mut self,
        sound_id: bmz_core::ids::SoundId,
        volume: f32,
        loop_playback: bool,
        fade_in_frames: u32,
        fade_out_frames: u32,
    ) {
        let voice_start = self.mixer.voices.len();
        self.mixer.push_scheduled([ScheduledSound {
            start_time: None,
            start_frame: 0,
            sample_offset_frames: 0,
            sound_id,
            volume: volume.clamp(0.0, 1.0),
            pan: 0.0,
            loop_playback,
            fade_in_frames,
            catch_up: false,
            restart_policy: RestartPolicy::Overlap,
        }]);
        if fade_out_frames != 0 {
            for voice in &mut self.mixer.voices[voice_start..] {
                voice.fade_out =
                    Some(ActiveVoiceFadeOut { started_output_frames: 0, frames: fade_out_frames });
            }
        }
    }

    /// 再生待ちのスケジュール音も鳴っているボイスも無い、つまり出力を
    /// ドレインし終えた状態かどうか。リザルト遷移後の余韻再生の終了判定に使う。
    pub fn is_idle(&self) -> bool {
        self.queue.is_empty() && self.mixer.voices.is_empty()
    }

    pub fn render_stereo(&mut self, output_start_frame: u64, output: &mut [f32]) {
        self.render_stereo_at(output_start_frame, output, None);
    }

    pub(crate) fn render_stereo_at(
        &mut self,
        output_start_frame: u64,
        output: &mut [f32],
        buffer_time: Option<crate::clock::OutputBufferTime>,
    ) {
        output.fill(0.0);
        if self.playback_paused() {
            return;
        }
        let frame_count = output.len() / 2;
        if let Some(buffer_time) = buffer_time {
            self.queue.resolve_times(
                output_start_frame,
                buffer_time,
                frame_count,
                self.output_sample_rate(),
            );
        }
        let output_end_frame = output_start_frame + frame_count.saturating_sub(1) as u64;
        // 中間 Vec を確保せず、期日到来分を直接 mixer へ流し込む(RT 安全)。
        self.mixer.push_scheduled(self.queue.drain_due(output_end_frame));
        self.mixer.mix_stereo(&self.samples, output_start_frame, output);
    }

    pub fn schedule_all(&mut self, sounds: impl IntoIterator<Item = ScheduledSound>) {
        for sound in sounds {
            self.schedule(sound);
        }
    }
}

impl AudioScheduler for AudioEngine {
    fn schedule(&mut self, sound: ScheduledSound) {
        self.queue.schedule(sound);
    }
}

#[cfg(test)]
mod tests {
    use bmz_core::ids::SoundId;

    use super::*;

    #[test]
    fn system_deadlines_render_at_the_same_phase_after_ten_minutes_of_device_drift() {
        use std::time::{Duration, Instant};
        let base = Instant::now();
        for sample_rate in [44_100, 48_000, 96_000] {
            for ppm in [-200, -40, 0, 40, 200] {
                let mut engine = AudioEngine::new(sample_rate);
                engine.insert_sample(
                    SoundId(1),
                    DecodedSample { channels: 1, sample_rate, frames: vec![0.5] },
                );
                for second in [0, 60, 180, 300, 600] {
                    let playback = base + Duration::from_secs(second);
                    let frame = 10_000
                        + (i128::from(second)
                            * i128::from(sample_rate)
                            * i128::from(1_000_000 + ppm)
                            / 1_000_000) as u64;
                    let mut sound = ScheduledSound::one_shot(0, SoundId(1), 1.0, 0.0);
                    sound.start_time = Some(playback + Duration::from_millis(3));
                    engine.schedule(sound);
                    let mut pcm = vec![0.0; (sample_rate / 100) as usize * 2];
                    let time = crate::clock::OutputBufferTime::for_frames(
                        playback,
                        pcm.len() / 2,
                        sample_rate,
                    );
                    engine.render_stereo_at(frame, &mut pcm, Some(time));
                    let onset =
                        pcm.as_chunks::<2>().0.iter().position(|sample| sample[0] != 0.0).unwrap();
                    assert_eq!(
                        onset,
                        (sample_rate * 3 / 1000) as usize,
                        "{sample_rate} Hz, {ppm} ppm, {second} s"
                    );
                    assert!(engine.is_idle());
                }
            }
        }
    }

    #[test]
    fn timed_bgm_catches_up_but_keysound_keeps_its_attack_after_callback_delay() {
        use std::time::{Duration, Instant};
        let base = Instant::now();
        for catch_up in [true, false] {
            let mut engine = AudioEngine::new(48_000);
            engine.insert_sample(
                SoundId(1),
                DecodedSample {
                    channels: 1,
                    sample_rate: 48_000,
                    frames: (0..1000).map(|i| i as f32 / 1000.0).collect(),
                },
            );
            let mut sound = ScheduledSound::one_shot(0, SoundId(1), 1.0, 0.0);
            sound.start_time = Some(base);
            sound.catch_up = catch_up;
            engine.schedule(sound);
            let mut pcm = [0.0; 2];
            let start = base + Duration::from_millis(10);
            let time = crate::clock::OutputBufferTime::for_frames(start, 1, 48_000);
            engine.render_stereo_at(10_000, &mut pcm, Some(time));
            assert_eq!(pcm[0], if catch_up { 0.48 } else { 0.0 });
        }
    }

    #[test]
    fn paused_live_deadline_retimes_from_pause_then_moves_to_resume() {
        use std::time::{Duration, Instant};
        let base = Instant::now();
        let mut engine = AudioEngine::new(48_000);
        engine.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.5] },
        );
        let mut sound = ScheduledSound::one_shot(4_800, SoundId(1), 1.0, 0.0);
        sound.start_time = Some(base + Duration::from_millis(100));
        engine.schedule(sound);
        engine.set_playback_paused_at(true, 960, base + Duration::from_millis(20));
        engine.apply_playback_rate_change(crate::clock::PlaybackRateChange {
            changed_at: None,
            anchor_output_frame: 24_000,
            old_rate_percent: 100,
            new_rate_percent: 200,
        });
        let mut pcm = [0.0; 192];
        let start = base + Duration::from_millis(500);
        let time = crate::clock::OutputBufferTime::for_frames(start, 96, 48_000);
        engine.render_stereo_at(24_000, &mut pcm, Some(time));
        assert!(pcm.iter().all(|value| *value == 0.0));
        engine.set_playback_paused_at(false, 48_000, base + Duration::from_secs(1));
        let start = base + Duration::from_millis(1_039);
        let time = crate::clock::OutputBufferTime::for_frames(start, 96, 48_000);
        engine.render_stereo_at(49_872, &mut pcm, Some(time));
        assert_eq!(pcm.iter().position(|value| *value != 0.0), Some(48 * 2));
        assert!(engine.is_idle());
    }

    #[test]
    fn audio_engine_renders_queued_samples() {
        let mut engine = AudioEngine::default();
        engine.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.5, 0.25] },
        );
        engine.schedule(ScheduledSound {
            start_time: None,
            start_frame: 2,
            sample_offset_frames: 0,
            sound_id: SoundId(1),
            volume: 1.0,
            pan: 0.0,
            loop_playback: false,
            fade_in_frames: 0,
            catch_up: true,
            restart_policy: RestartPolicy::Overlap,
        });
        let mut output = vec![1.0; 8];

        engine.render_stereo(0, &mut output);

        assert_eq!(output, vec![0.0, 0.0, 0.0, 0.0, 0.5, 0.5, 0.25, 0.25]);
    }

    #[test]
    fn clear_playback_keeps_samples_but_discards_queue_and_voices() {
        let mut engine = AudioEngine::new(48_000);
        engine.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0; 16] },
        );
        engine.play_now(SoundId(1), 1.0, false);
        engine.schedule(ScheduledSound::one_shot(100, SoundId(1), 1.0, 0.0));
        let mut output = vec![0.0; 2];
        engine.render_stereo(0, &mut output);
        assert!(!engine.mixer.voices.is_empty());

        engine.clear_playback();

        assert!(engine.queue.is_empty());
        assert!(engine.mixer.voices.is_empty());
        assert!(engine.samples.get(SoundId(1)).is_some());
    }

    #[test]
    fn restart_policy_applies_within_render_buffer() {
        let mut engine = AudioEngine::default();
        engine.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 0.5, 0.25, 0.125] },
        );
        engine.schedule(ScheduledSound {
            start_time: None,
            start_frame: 0,
            sample_offset_frames: 0,
            sound_id: SoundId(1),
            volume: 1.0,
            pan: 0.0,
            loop_playback: false,
            fade_in_frames: 0,
            catch_up: true,
            restart_policy: RestartPolicy::StopSameSound,
        });
        engine.schedule(ScheduledSound {
            start_time: None,
            start_frame: 2,
            sample_offset_frames: 0,
            sound_id: SoundId(1),
            volume: 1.0,
            pan: 0.0,
            loop_playback: false,
            fade_in_frames: 0,
            catch_up: true,
            restart_policy: RestartPolicy::StopSameSound,
        });
        let mut output = vec![0.0; 12];

        engine.render_stereo(0, &mut output);

        assert_eq!(output, vec![1.0, 1.0, 0.5, 0.5, 1.0, 1.0, 0.5, 0.5, 0.25, 0.25, 0.125, 0.125]);
        assert!(engine.is_idle());
    }

    #[test]
    fn insert_sample_resamples_to_output_rate() {
        let mut engine = AudioEngine::new(48_000);
        engine.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 24_000, frames: vec![0.0, 1.0] },
        );

        let sample = engine.samples.get(SoundId(1)).unwrap();
        assert_eq!(sample.sample_rate(), 48_000);
        assert_eq!(
            (0..sample.frame_count())
                .map(|frame| sample.sample_stereo(frame).0)
                .collect::<Vec<_>>(),
            vec![0.0, 0.5, 1.0, 1.0]
        );
    }

    #[test]
    fn insert_sample_keeps_pcm_allocation_when_rate_already_matches() {
        let mut engine = AudioEngine::new(48_000);
        let sample = DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.0, 1.0] };
        let frames_ptr = sample.frames.as_ptr();

        engine.insert_sample(SoundId(1), sample);

        assert_eq!(engine.samples.get(SoundId(1)).unwrap().source_frames_ptr(), frames_ptr);
    }

    #[test]
    fn adjacent_shared_regions_play_without_crossing_boundaries() {
        let mut engine = AudioEngine::new(48_000);
        let source = Arc::new(DecodedSample {
            channels: 1,
            sample_rate: 48_000,
            frames: vec![0.25, 0.5, 0.75, 1.0],
        });
        engine.insert_shared_sample_region(SoundId(1), source.clone(), 0, 2);
        engine.insert_shared_sample_region(SoundId(2), source, 2, 4);
        engine.schedule_all([
            ScheduledSound {
                start_time: None,
                start_frame: 0,
                sample_offset_frames: 0,
                sound_id: SoundId(1),
                volume: 1.0,
                pan: 0.0,
                loop_playback: false,
                fade_in_frames: 0,
                catch_up: false,
                restart_policy: RestartPolicy::Overlap,
            },
            ScheduledSound {
                start_time: None,
                start_frame: 2,
                sample_offset_frames: 0,
                sound_id: SoundId(2),
                volume: 1.0,
                pan: 0.0,
                loop_playback: false,
                fade_in_frames: 0,
                catch_up: false,
                restart_policy: RestartPolicy::Overlap,
            },
        ]);

        let mut output = vec![0.0; 8];
        engine.render_stereo(0, &mut output);

        assert_eq!(output, vec![0.25, 0.25, 0.5, 0.5, 0.75, 0.75, 1.0, 1.0]);
    }

    #[test]
    fn insert_prepared_sample_keeps_matching_output_rate() {
        let mut engine = AudioEngine::new(48_000);
        engine.insert_prepared_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.0, 1.0] },
        );

        let sample = engine.samples.get(SoundId(1)).unwrap();
        assert_eq!(sample.sample_rate(), 48_000);
        assert_eq!(sample.sample_stereo(0), (0.0, 0.0));
        assert_eq!(sample.sample_stereo(1), (1.0, 1.0));
    }

    #[test]
    fn insert_prepared_sample_resamples_if_rate_changed() {
        let mut engine = AudioEngine::new(48_000);
        engine.insert_prepared_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 24_000, frames: vec![0.0, 1.0] },
        );

        let sample = engine.samples.get(SoundId(1)).unwrap();
        assert_eq!(sample.sample_rate(), 48_000);
        assert_eq!(
            (0..sample.frame_count())
                .map(|frame| sample.sample_stereo(frame).0)
                .collect::<Vec<_>>(),
            vec![0.0, 0.5, 1.0, 1.0]
        );
    }

    #[test]
    fn schedule_all_preserves_scheduled_sounds() {
        let mut engine = AudioEngine::default();

        engine.schedule_all([
            ScheduledSound {
                start_time: None,
                start_frame: 20,
                sample_offset_frames: 0,
                sound_id: SoundId(2),
                volume: 1.0,
                pan: 0.0,
                loop_playback: false,
                fade_in_frames: 0,
                catch_up: true,
                restart_policy: RestartPolicy::Overlap,
            },
            ScheduledSound {
                start_time: None,
                start_frame: 10,
                sample_offset_frames: 0,
                sound_id: SoundId(1),
                volume: 1.0,
                pan: 0.0,
                loop_playback: false,
                fade_in_frames: 0,
                catch_up: true,
                restart_policy: RestartPolicy::Overlap,
            },
        ]);

        assert_eq!(
            engine
                .queue
                .drain_until_frame(20)
                .iter()
                .map(|sound| sound.sound_id.0)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    #[test]
    fn set_output_sample_rate_resamples_loaded_samples() {
        let mut engine = AudioEngine::new(24_000);
        engine.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 24_000, frames: vec![0.0, 1.0] },
        );

        engine.set_output_sample_rate(48_000);

        assert_eq!(engine.mixer.output_sample_rate, 48_000);
        let sample = engine.samples.get(SoundId(1)).unwrap();
        assert_eq!(sample.sample_rate(), 48_000);
        assert_eq!(
            (0..sample.frame_count())
                .map(|frame| sample.sample_stereo(frame).0)
                .collect::<Vec<_>>(),
            vec![0.0, 0.5, 1.0, 1.0]
        );
    }

    #[test]
    fn with_sample_bank_reuses_samples_without_playback_state() {
        let mut source = AudioEngine::new(48_000);
        source.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.5, 0.25] },
        );
        source.schedule(ScheduledSound {
            start_time: None,
            start_frame: 0,
            sample_offset_frames: 0,
            sound_id: SoundId(1),
            volume: 1.0,
            pan: 0.0,
            loop_playback: false,
            fade_in_frames: 0,
            catch_up: false,
            restart_policy: RestartPolicy::Overlap,
        });

        let reused =
            AudioEngine::with_sample_bank(source.output_sample_rate(), source.samples.clone());

        assert!(reused.is_idle());
        assert_eq!(reused.samples.get(SoundId(1)).unwrap().frame_count(), 2);
    }

    #[test]
    fn stop_sound_drops_queue_and_voices_for_id() {
        let mut engine = AudioEngine::default();
        engine.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.5, 0.5, 0.5, 0.5] },
        );
        engine.play_now(SoundId(1), 1.0, true);
        // 1 サンプル分 render してループ voice が生きていることを確認。
        let mut output = vec![0.0; 4];
        engine.render_stereo(0, &mut output);
        assert!(!engine.is_idle());

        engine.stop_sound(SoundId(1));

        // stop 後はキューも voice も空。
        assert!(engine.is_idle());
    }

    #[test]
    fn stop_sound_with_fade_out_keeps_voice_until_ramped_down() {
        let mut engine = AudioEngine::default();
        engine.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 1.0, 1.0, 1.0] },
        );
        engine.play_now(SoundId(1), 1.0, true);
        let mut output = vec![0.0; 2];
        engine.render_stereo(0, &mut output);

        engine.stop_sound_with_fade_out(SoundId(1), 2);
        assert!(!engine.is_idle());
        let mut output = vec![0.0; 6];
        engine.render_stereo(1, &mut output);

        assert_eq!(output, vec![1.0, 1.0, 0.5, 0.5, 0.0, 0.0]);
        assert!(engine.is_idle());
    }

    #[test]
    fn play_now_with_fade_out_ramps_from_first_rendered_frame() {
        let mut engine = AudioEngine::default();
        engine.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 1.0, 1.0, 1.0] },
        );

        engine.play_now_with_fade_in_and_fade_out(SoundId(1), 1.0, false, 0, 2);
        let mut output = vec![0.0; 6];
        engine.render_stereo(100, &mut output);

        assert_eq!(output, vec![1.0, 1.0, 0.5, 0.5, 0.0, 0.0]);
        assert!(engine.is_idle());
    }

    #[test]
    fn set_sound_volume_updates_queue_and_active_voice() {
        let mut engine = AudioEngine::default();
        engine.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![1.0, 1.0] },
        );
        engine.schedule(ScheduledSound {
            start_time: None,
            start_frame: 2,
            sample_offset_frames: 0,
            sound_id: SoundId(1),
            volume: 1.0,
            pan: 0.0,
            loop_playback: false,
            fade_in_frames: 0,
            catch_up: true,
            restart_policy: RestartPolicy::Overlap,
        });
        engine.set_sound_volume(SoundId(1), 0.25);

        let mut output = vec![0.0; 8];
        engine.render_stereo(0, &mut output);

        assert_eq!(output, vec![0.0, 0.0, 0.0, 0.0, 0.25, 0.25, 0.25, 0.25]);

        engine.play_now(SoundId(1), 1.0, true);
        let mut output = vec![0.0; 2];
        engine.render_stereo(4, &mut output);
        engine.set_sound_volume(SoundId(1), 0.5);
        let mut output = vec![0.0; 2];
        engine.render_stereo(5, &mut output);

        assert_eq!(output, vec![0.5, 0.5]);
    }

    #[test]
    fn audio_engine_is_idle_until_drained() {
        let mut engine = AudioEngine::default();
        assert!(engine.is_idle());

        engine.insert_sample(
            SoundId(1),
            DecodedSample { channels: 1, sample_rate: 48_000, frames: vec![0.5] },
        );
        engine.schedule(ScheduledSound {
            start_time: None,
            start_frame: 0,
            sample_offset_frames: 0,
            sound_id: SoundId(1),
            volume: 1.0,
            pan: 0.0,
            loop_playback: false,
            fade_in_frames: 0,
            catch_up: true,
            restart_policy: RestartPolicy::Overlap,
        });
        // スケジュール済みの音が残っている間はアイドルではない。
        assert!(!engine.is_idle());

        // 1 フレームだけのサンプルを鳴らし切るとアイドルへ戻る。
        let mut output = vec![0.0; 4];
        engine.render_stereo(0, &mut output);
        assert!(engine.is_idle());
    }
}
