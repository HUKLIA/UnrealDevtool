//! Close policy is independent of rendering so background-work protection is testable.
use crate::config::CloseBehavior;

#[derive(Debug, PartialEq, Eq)]
pub enum CloseAction { Prompt, Minimize, Close }

pub fn close_action(preference: CloseBehavior, working: bool) -> CloseAction {
    match preference {
        CloseBehavior::Minimize => CloseAction::Minimize,
        CloseBehavior::Close if !working => CloseAction::Close,
        _ => CloseAction::Prompt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn running_tasks_always_require_exit_confirmation() {
        assert_eq!(close_action(CloseBehavior::Close, true), CloseAction::Prompt);
        assert_eq!(close_action(CloseBehavior::Ask, true), CloseAction::Prompt);
        assert_eq!(close_action(CloseBehavior::Minimize, true), CloseAction::Minimize);
    }
    #[test]
    fn idle_close_preferences_are_respected() {
        assert_eq!(close_action(CloseBehavior::Ask, false), CloseAction::Prompt);
        assert_eq!(close_action(CloseBehavior::Close, false), CloseAction::Close);
        assert_eq!(close_action(CloseBehavior::Minimize, false), CloseAction::Minimize);
    }
    #[test]
    fn preferences_round_trip_and_unknown_values_fail_safely() {
        for p in [CloseBehavior::Ask, CloseBehavior::Minimize, CloseBehavior::Close] {
            let json = serde_json::to_string(&p).unwrap();
            assert_eq!(serde_json::from_str::<CloseBehavior>(&json).unwrap(), p);
        }
        assert!(serde_json::from_str::<CloseBehavior>("\"unknown\"").is_err());
    }
}
