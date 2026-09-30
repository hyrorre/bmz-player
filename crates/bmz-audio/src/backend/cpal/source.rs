use super::*;

impl CpalOutput {
    pub fn play(&mut self, chart_zero_time: TimeUs) -> Result<(), CpalBackendError> {
        self.source.play(chart_zero_time);
        Ok(())
    }

    pub fn pause(&mut self) -> Result<(), ::cpal::Error> {
        self.source.pause();
        Ok(())
    }

    pub fn clock(&self) -> AudioClock {
        self.source.clock()
    }
}

impl CpalSharedOutput {
    pub fn stream_info(&self) -> &CpalStreamInfo {
        &self.inner.info
    }
    pub fn play(&self) -> Result<(), CpalBackendError> {
        match &self.inner.stream {
            CpalOutputStream::Cpal(stream) => {
                stream.play().map_err(CpalBackendError::PlayStream)?;
            }
            #[cfg(windows)]
            CpalOutputStream::WasapiExclusive(stream) => {
                stream
                    .play()
                    .map_err(|error| CpalBackendError::WasapiExclusive(error.to_string()))?;
            }
        }
        Ok(())
    }

    pub fn uses_pulseaudio_host(&self) -> bool {
        #[cfg(all(
            any(
                target_os = "linux",
                target_os = "dragonfly",
                target_os = "freebsd",
                target_os = "netbsd"
            ),
            feature = "pulseaudio"
        ))]
        {
            matches!(self.inner.host_id, ::cpal::HostId::PulseAudio)
        }
        #[cfg(not(all(
            any(
                target_os = "linux",
                target_os = "dragonfly",
                target_os = "freebsd",
                target_os = "netbsd"
            ),
            feature = "pulseaudio"
        )))]
        {
            let _ = self.inner.host_id;
            false
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.inner.sample_rate
    }

    /// IAudioClient3 の共有エンジン周期要求が現在のストリームで有効なら、
    /// 実効 engine period を CPAL client sample-rate の frames へ換算して返す。
    pub fn low_latency_shared_period_frames(&self) -> Option<u32> {
        #[cfg(windows)]
        {
            self.inner._low_latency_guard.as_ref().map(|guard| guard.info().client_period_frames)
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    /// WASAPI 排他モードの実効 endpoint buffer frames。
    pub fn exclusive_buffer_frames(&self) -> Option<u32> {
        #[cfg(windows)]
        {
            match &self.inner.stream {
                CpalOutputStream::WasapiExclusive(stream) => Some(stream.info().buffer_frames),
                CpalOutputStream::Cpal(_) => None,
            }
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    pub fn take_diagnostics(&self) -> CpalOutputDiagnostics {
        self.inner.diagnostics.take_snapshot()
    }

    /// callback から退避した source を app thread で破棄する。
    ///
    /// source が最後に保持する chart sample bank は大きくなり得るため、callback
    /// 中での `Drop` を避ける。callback と競合中でも app thread は待機してよい。
    pub fn reap_retired_sources(&self) {
        let Ok(mut sources) = self.inner.retired_sources.lock() else {
            return;
        };
        sources.clear();
    }

    pub fn add_source(&self, engine: SharedAudioEngine) -> CpalOutputSource {
        self.add_source_with_kind(engine, CpalOutputSourceKind::Other)
    }

    pub fn add_source_with_kind(
        &self,
        engine: SharedAudioEngine,
        kind: CpalOutputSourceKind,
    ) -> CpalOutputSource {
        if let Ok(mut engine) = engine.lock() {
            // 実ストリームレートへ揃える。既に読込済みのサンプルもここで再変換され、
            // ミキサーは等倍(補間なし)で再生できる。
            engine.set_output_sample_rate(self.inner.sample_rate);
        }

        let id = self.inner.next_source_id.fetch_add(1, Ordering::Relaxed);
        push_output_command(
            &self.inner.output_commands,
            CpalOutputCommand::AddLegacySource { id, kind, engine: Arc::clone(&engine) },
        );

        let clock = AudioClock::with_position(
            self.inner.sample_rate,
            0,
            0,
            Arc::clone(&self.inner.current_frame),
            false,
        );
        CpalOutputSource { id, inner: Rc::downgrade(&self.inner), kind, engine, clock }
    }

    pub fn add_commanded_source_with_kind(
        &self,
        handle: AudioEngineHandle,
        kind: CpalOutputSourceKind,
    ) -> CpalCommandedOutputSource {
        handle.set_output_sample_rate(self.inner.sample_rate);
        let id = self.inner.next_source_id.fetch_add(1, Ordering::Relaxed);
        push_output_command(
            &self.inner.output_commands,
            CpalOutputCommand::AddCommandedSource { id, kind, engine: handle.processor() },
        );

        let clock = AudioClock::with_position(
            self.inner.sample_rate,
            0,
            0,
            Arc::clone(&self.inner.current_frame),
            false,
        );
        CpalCommandedOutputSource { id, inner: Rc::downgrade(&self.inner), kind, handle, clock }
    }
}

impl CpalOutputSource {
    pub fn kind(&self) -> CpalOutputSourceKind {
        self.kind
    }

    pub fn set_kind(&mut self, kind: CpalOutputSourceKind) {
        self.kind = kind;
        let Some(inner) = self.inner.upgrade() else {
            return;
        };
        push_output_command(
            &inner.output_commands,
            CpalOutputCommand::SetSourceKind { id: self.id, kind },
        );
    }

    pub fn play(&mut self, chart_zero_time: TimeUs) {
        self.clock.start(chart_zero_time);
    }

    pub fn pause(&mut self) {
        self.clock.pause();
    }

    pub fn clock(&self) -> AudioClock {
        self.clock.clone()
    }
}

impl Drop for CpalOutputSource {
    fn drop(&mut self) {
        let Some(inner) = self.inner.upgrade() else {
            return;
        };
        push_output_command(
            &inner.output_commands,
            CpalOutputCommand::RemoveSource { id: self.id },
        );
    }
}

impl CpalCommandedOutputSource {
    pub fn kind(&self) -> CpalOutputSourceKind {
        self.kind
    }

    pub fn handle(&self) -> AudioEngineHandle {
        self.handle.clone()
    }

    pub fn set_kind(&mut self, kind: CpalOutputSourceKind) {
        self.kind = kind;
        let Some(inner) = self.inner.upgrade() else {
            return;
        };
        push_output_command(
            &inner.output_commands,
            CpalOutputCommand::SetSourceKind { id: self.id, kind },
        );
    }

    pub fn play(&mut self, chart_zero_time: TimeUs) {
        self.clock.start(chart_zero_time);
    }

    pub fn set_playback_rate_percent(
        &mut self,
        rate: u16,
    ) -> Option<crate::clock::PlaybackRateChange> {
        self.clock.set_playback_rate_percent(rate)
    }

    pub fn pause(&mut self) {
        self.clock.pause();
    }

    pub fn pause_at(&mut self, chart_time: TimeUs) {
        self.clock.pause_at(chart_time);
    }

    pub fn clock(&self) -> AudioClock {
        self.clock.clone()
    }
}

impl Drop for CpalCommandedOutputSource {
    fn drop(&mut self) {
        let Some(inner) = self.inner.upgrade() else {
            return;
        };
        push_output_command(
            &inner.output_commands,
            CpalOutputCommand::RemoveSource { id: self.id },
        );
    }
}
