use super::*;

/// Tracks logical play attempts, including retries that do not change AppSceneKind.
#[derive(Default)]
pub(super) struct ObsLifecycle {
    pub(super) scene: Option<ObsEventKey>,
    play_ended: bool,
}

impl ObsLifecycle {
    pub(super) fn transition(&mut self, scene: ObsEventKey) -> Vec<ObsTrigger> {
        if scene == ObsEventKey::PlayEnded {
            return self.end_play();
        }
        if self.scene == Some(scene) {
            if scene == ObsEventKey::Play && self.play_ended {
                self.play_ended = false;
                return vec![ObsTrigger::PlayStart];
            }
            return Vec::new();
        }
        let mut events = self.end_scene();
        self.scene = Some(scene);
        self.play_ended = false;
        events.push(scene.start_trigger());
        events
    }

    pub(super) fn end_play(&mut self) -> Vec<ObsTrigger> {
        if self.scene == Some(ObsEventKey::Play) && !self.play_ended {
            self.play_ended = true;
            vec![ObsTrigger::PlayEnd]
        } else {
            Vec::new()
        }
    }

    pub(super) fn retry(&mut self) -> Vec<ObsTrigger> {
        let mut events = self.end_play();
        self.scene = Some(ObsEventKey::Play);
        self.play_ended = false;
        events.push(ObsTrigger::PlayStart);
        events
    }

    pub(super) fn end_scene(&mut self) -> Vec<ObsTrigger> {
        match self.scene {
            Some(ObsEventKey::Play) => self.end_play(),
            Some(scene) => vec![scene.end_trigger()],
            None => Vec::new(),
        }
    }
}

impl ObsEventKey {
    pub(super) fn start_trigger(self) -> ObsTrigger {
        match self {
            Self::MusicSelect => ObsTrigger::MusicSelectStart,
            Self::Decide => ObsTrigger::DecideStart,
            Self::Play => ObsTrigger::PlayStart,
            Self::PlayEnded => ObsTrigger::PlayEnd,
            Self::Result => ObsTrigger::ResultStart,
            Self::CourseResult => ObsTrigger::CourseResultStart,
        }
    }

    fn end_trigger(self) -> ObsTrigger {
        match self {
            Self::MusicSelect => ObsTrigger::MusicSelectEnd,
            Self::Decide => ObsTrigger::DecideEnd,
            Self::Play | Self::PlayEnded => ObsTrigger::PlayEnd,
            Self::Result => ObsTrigger::ResultEnd,
            Self::CourseResult => ObsTrigger::CourseResultEnd,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_orders_end_start_and_deduplicates_terminal_notifications() {
        let mut state = ObsLifecycle::default();
        assert_eq!(state.transition(ObsEventKey::MusicSelect), [ObsTrigger::MusicSelectStart]);
        assert_eq!(
            state.transition(ObsEventKey::Decide),
            [ObsTrigger::MusicSelectEnd, ObsTrigger::DecideStart]
        );
        assert_eq!(
            state.transition(ObsEventKey::Play),
            [ObsTrigger::DecideEnd, ObsTrigger::PlayStart]
        );
        assert_eq!(state.end_play(), [ObsTrigger::PlayEnd]);
        assert!(state.end_play().is_empty());
        assert_eq!(state.transition(ObsEventKey::Result), [ObsTrigger::ResultStart]);
        assert_eq!(
            state.transition(ObsEventKey::CourseResult),
            [ObsTrigger::ResultEnd, ObsTrigger::CourseResultStart]
        );
    }

    #[test]
    fn retries_manual_exit_and_course_stages_end_each_attempt_once() {
        for destination in [ObsEventKey::MusicSelect, ObsEventKey::Decide, ObsEventKey::Result] {
            let mut state = ObsLifecycle::default();
            state.transition(ObsEventKey::Play);
            assert_eq!(state.retry(), [ObsTrigger::PlayEnd, ObsTrigger::PlayStart]);
            assert_eq!(
                state.transition(destination),
                [ObsTrigger::PlayEnd, destination.start_trigger()]
            );
            assert!(state.end_play().is_empty());
        }
    }
}
