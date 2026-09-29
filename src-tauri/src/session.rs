//! The foreground interaction has one state, independent of window visibility.
use serde::Serialize;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Phase {
    #[default]
    Idle,
    Activating,
    Listening,
    Transcribing,
    Thinking,
    AnalyzingScreen,
    Planning,
    Guiding,
    WaitingForUser,
    Verifying,
    Speaking,
    Completed,
    Error,
    Paused,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct Session {
    pub phase: Phase,
    pub message: String,
}
impl Session {
    pub fn active(&self) -> bool {
        !matches!(self.phase, Phase::Idle | Phase::Paused)
    }
    pub fn transition(&mut self, next: Phase, message: impl Into<String>) -> bool {
        use Phase::*;
        let allowed = next == Idle
            || next == Error
            || next == Paused
            || self.phase == next
            || matches!(
                (self.phase, next),
                (Idle, Activating)
                    | (Activating, Listening)
                    | (Activating, Thinking)
                    | (Listening, Transcribing)
                    | (Transcribing, Thinking)
                    | (Thinking, Planning)
                    | (Thinking, AnalyzingScreen)
                    | (Planning, AnalyzingScreen)
                    | (AnalyzingScreen, Planning)
                    | (AnalyzingScreen, Guiding)
                    | (Planning, Guiding)
                    | (Guiding, Speaking)
                    | (Guiding, WaitingForUser)
                    | (Speaking, WaitingForUser)
                    | (Speaking, Verifying)
                    | (WaitingForUser, Verifying)
                    | (Guiding, Verifying)
                    | (Verifying, AnalyzingScreen)
                    | (Verifying, Completed)
                    | (Completed, Speaking)
                    | (Speaking, Completed)
            );
        if !allowed {
            return false;
        }
        self.phase = next;
        self.message = message.into();
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn voice_session_and_cancellation() {
        let mut s = Session::default();
        for phase in [
            Phase::Activating,
            Phase::Listening,
            Phase::Transcribing,
            Phase::Thinking,
            Phase::AnalyzingScreen,
            Phase::Planning,
            Phase::Guiding,
            Phase::Speaking,
            Phase::WaitingForUser,
            Phase::Verifying,
            Phase::Completed,
            Phase::Speaking,
            Phase::Completed,
            Phase::Idle,
        ] {
            assert!(s.transition(phase, ""), "{phase:?}");
        }
        assert!(!s.active());
    }
    #[test]
    fn every_active_state_can_cancel() {
        for phase in [
            Phase::Listening,
            Phase::Transcribing,
            Phase::Speaking,
            Phase::Guiding,
            Phase::Error,
        ] {
            let mut s = Session {
                phase,
                message: String::new(),
            };
            assert!(s.transition(Phase::Idle, ""));
            assert!(!s.active());
        }
    }
    #[test]
    fn idle_cannot_jump_to_microphone_or_verification() {
        let mut s = Session::default();
        assert!(!s.transition(Phase::Listening, ""));
        assert!(!s.transition(Phase::Verifying, ""));
    }
}
