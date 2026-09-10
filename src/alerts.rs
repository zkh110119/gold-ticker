use crate::domain::ThresholdStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Transition {
    NoAlert,
    CrossedBelow,
}

pub(crate) fn transition(
    previous: Option<ThresholdStatus>,
    current: ThresholdStatus,
) -> Transition {
    match (previous, current) {
        (Some(ThresholdStatus::AtOrAbove), ThresholdStatus::Below) => Transition::CrossedBelow,
        _ => Transition::NoAlert,
    }
}

#[cfg(test)]
mod tests {
    use super::{Transition, transition};
    use crate::domain::ThresholdStatus;

    #[test]
    fn alerts_only_for_a_fresh_downward_crossing() {
        assert_eq!(
            transition(Some(ThresholdStatus::AtOrAbove), ThresholdStatus::Below),
            Transition::CrossedBelow
        );
        assert_eq!(
            transition(Some(ThresholdStatus::Below), ThresholdStatus::Below),
            Transition::NoAlert
        );
        assert_eq!(
            transition(None, ThresholdStatus::Below),
            Transition::NoAlert
        );
        assert_eq!(
            transition(Some(ThresholdStatus::Below), ThresholdStatus::AtOrAbove),
            Transition::NoAlert
        );
    }

    #[test]
    fn equality_recovers_the_alert_state_for_a_future_crossing() {
        let threshold = ThresholdStatus::AtOrAbove;
        assert_eq!(
            transition(Some(threshold), ThresholdStatus::Below),
            Transition::CrossedBelow
        );
    }
}
