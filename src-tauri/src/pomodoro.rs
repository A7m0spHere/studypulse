use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PomodoroState {
    pub status: String,
    pub total_seconds: i64,
    pub remaining_seconds: i64,
    pub completed_count: i64,
}

impl Default for PomodoroState {
    fn default() -> Self {
        Self {
            status: "idle".into(),
            total_seconds: 25 * 60,
            remaining_seconds: 25 * 60,
            completed_count: 0,
        }
    }
}

#[derive(Debug)]
pub struct PomodoroMachine {
    state: PomodoroState,
    token: u64,
}

impl PomodoroMachine {
    pub fn new() -> Self {
        Self {
            state: PomodoroState::default(),
            token: 0,
        }
    }

    pub fn start(&mut self, minutes: i64) -> (PomodoroState, u64) {
        let total_seconds = minutes.max(1) * 60;
        self.token = self.token.wrapping_add(1);
        self.state.status = "running".into();
        self.state.total_seconds = total_seconds;
        self.state.remaining_seconds = total_seconds;
        (self.state.clone(), self.token)
    }

    pub fn pause(&mut self) -> PomodoroState {
        if self.state.status == "running" {
            self.state.status = "paused".into();
        } else if self.state.status == "paused" {
            self.state.status = "running".into();
        }
        self.state.clone()
    }

    pub fn reset(&mut self) -> PomodoroState {
        self.token = self.token.wrapping_add(1);
        self.state = PomodoroState::default();
        self.state.clone()
    }

    pub fn snapshot(&self) -> PomodoroState {
        self.state.clone()
    }

    pub fn tick_one_second(&mut self, token: u64) -> TickResult {
        if token != self.token {
            return TickResult::Cancelled;
        }

        if self.state.status != "running" {
            return TickResult::Waiting;
        }

        if self.state.remaining_seconds > 1 {
            self.state.remaining_seconds -= 1;
            return TickResult::Running;
        }

        self.state.remaining_seconds = 0;
        self.state.status = "completed".into();
        self.state.completed_count += 1;
        TickResult::Completed(self.state.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TickResult {
    Running,
    Waiting,
    Completed(PomodoroState),
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_pomodoro() {
        let mut machine = PomodoroMachine::new();
        let (state, _) = machine.start(25);
        assert_eq!(state.status, "running");
        assert_eq!(state.total_seconds, 1500);
        assert_eq!(state.remaining_seconds, 1500);
    }

    #[test]
    fn pauses_and_resumes_pomodoro() {
        let mut machine = PomodoroMachine::new();
        machine.start(25);
        assert_eq!(machine.pause().status, "paused");
        assert_eq!(machine.pause().status, "running");
    }

    #[test]
    fn resets_pomodoro() {
        let mut machine = PomodoroMachine::new();
        machine.start(10);
        let state = machine.reset();
        assert_eq!(state.status, "idle");
        assert_eq!(state.total_seconds, 1500);
        assert_eq!(state.remaining_seconds, 1500);
    }

    #[test]
    fn completes_pomodoro() {
        let mut machine = PomodoroMachine::new();
        let (_, token) = machine.start(1);

        for _ in 0..59 {
            assert_eq!(machine.tick_one_second(token), TickResult::Running);
        }

        match machine.tick_one_second(token) {
            TickResult::Completed(state) => {
                assert_eq!(state.status, "completed");
                assert_eq!(state.remaining_seconds, 0);
                assert_eq!(state.completed_count, 1);
            }
            other => panic!("expected completed state, got {other:?}"),
        }
    }
}
