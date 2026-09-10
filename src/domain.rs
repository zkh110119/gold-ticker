use std::time::Duration;

use rust_decimal::Decimal;

use crate::provider::Quote;

pub(crate) const STALE_AFTER: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Up,
    Down,
    Flat,
}

impl Direction {
    pub(crate) fn from_decimal(value: Decimal) -> Self {
        if value.is_zero() {
            Self::Flat
        } else if value.is_sign_positive() {
            Self::Up
        } else {
            Self::Down
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PriceChange {
    pub(crate) absolute: Decimal,
    pub(crate) percentage: Decimal,
    pub(crate) direction: Direction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ThresholdStatus {
    AtOrAbove,
    Below,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Freshness {
    Fresh,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QuoteState {
    pub(crate) quote: Quote,
    pub(crate) change: Option<PriceChange>,
    pub(crate) threshold_status: ThresholdStatus,
    pub(crate) freshness: Freshness,
}

pub(crate) fn price_change(current: &Quote, previous: Option<&Quote>) -> Option<PriceChange> {
    let previous = previous?;
    if !same_quote_identity(current, previous) || previous.price.is_zero() {
        return None;
    }

    let absolute = current.price - previous.price;
    Some(PriceChange {
        percentage: absolute / previous.price * Decimal::ONE_HUNDRED,
        direction: Direction::from_decimal(absolute),
        absolute,
    })
}

pub(crate) fn threshold_status(price: Decimal, threshold: Decimal) -> ThresholdStatus {
    if price >= threshold {
        ThresholdStatus::AtOrAbove
    } else {
        ThresholdStatus::Below
    }
}

pub(crate) fn freshness(fetched_at_unix_secs: u64, now_unix_secs: u64) -> Freshness {
    if now_unix_secs.saturating_sub(fetched_at_unix_secs) > STALE_AFTER.as_secs() {
        Freshness::Stale
    } else {
        Freshness::Fresh
    }
}

pub(crate) fn quote_state(
    current: Quote,
    previous: Option<&Quote>,
    threshold: Decimal,
    now_unix_secs: u64,
) -> QuoteState {
    QuoteState {
        change: price_change(&current, previous),
        threshold_status: threshold_status(current.price, threshold),
        freshness: freshness(current.fetched_at_unix_secs, now_unix_secs),
        quote: current,
    }
}

fn same_quote_identity(left: &Quote, right: &Quote) -> bool {
    left.symbol == right.symbol && left.display_name == right.display_name
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::{Direction, Freshness, STALE_AFTER, freshness, price_change, threshold_status};
    use crate::provider::Quote;

    fn quote(price: Decimal) -> Quote {
        Quote {
            symbol: "hf_XAU".to_owned(),
            display_name: "伦敦金".to_owned(),
            price,
            percentage_change: Decimal::ZERO,
            previous_close: Decimal::ZERO,
            quote_date: "2026-09-09".to_owned(),
            quote_time: "10:33:00".to_owned(),
            fetched_at_unix_secs: 1_000,
            provider: "腾讯行情".to_owned(),
        }
    }

    #[test]
    fn calculates_up_down_and_flat_changes_with_decimal_precision() {
        let previous = quote(Decimal::new(100, 2));
        let up = price_change(&quote(Decimal::new(125, 2)), Some(&previous)).unwrap();
        let down = price_change(&quote(Decimal::new(75, 2)), Some(&previous)).unwrap();
        let flat = price_change(&previous, Some(&previous)).unwrap();

        assert_eq!(up.absolute, Decimal::new(25, 2));
        assert_eq!(up.percentage, Decimal::new(25, 0));
        assert_eq!(up.direction, Direction::Up);
        assert_eq!(down.direction, Direction::Down);
        assert_eq!(flat.direction, Direction::Flat);
    }

    #[test]
    fn omits_change_for_zero_baseline_or_different_quote_identity() {
        assert!(price_change(&quote(Decimal::ONE), Some(&quote(Decimal::ZERO))).is_none());
        let mut different = quote(Decimal::ONE);
        different.symbol = "hf_SI".to_owned();
        assert!(price_change(&quote(Decimal::ONE), Some(&different)).is_none());
    }

    #[test]
    fn treats_threshold_equality_as_at_or_above() {
        let threshold = Decimal::new(436_990, 2);
        assert_eq!(
            threshold_status(threshold, threshold),
            super::ThresholdStatus::AtOrAbove
        );
        assert_eq!(
            threshold_status(threshold - Decimal::ONE, threshold),
            super::ThresholdStatus::Below
        );
    }

    #[test]
    fn marks_data_stale_only_after_the_cutoff() {
        assert_eq!(
            freshness(1_000, 1_000 + STALE_AFTER.as_secs()),
            Freshness::Fresh
        );
        assert_eq!(
            freshness(1_000, 1_001 + STALE_AFTER.as_secs()),
            Freshness::Stale
        );
    }
}
