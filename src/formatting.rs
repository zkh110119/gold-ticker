use rust_decimal::Decimal;

use crate::domain::{Direction, PriceChange};

pub(crate) fn format_price(value: Decimal) -> String {
    let scale = value.scale();
    let normalized = value.normalize().to_string();
    let (integer, fraction) = normalized.split_once('.').unwrap_or((&normalized, ""));
    let (sign, digits) = integer
        .strip_prefix('-')
        .map_or(("", integer), |digits| ("-", digits));
    let grouped = digits
        .chars()
        .rev()
        .enumerate()
        .fold(String::new(), |mut output, (index, character)| {
            if index > 0 && index % 3 == 0 {
                output.push(',');
            }
            output.push(character);
            output
        })
        .chars()
        .rev()
        .collect::<String>();

    if scale == 0 {
        format!("{sign}{grouped}")
    } else {
        format!(
            "{sign}{grouped}.{fraction:0<width$}",
            width = scale as usize
        )
    }
}

pub(crate) fn format_change(change: Option<&PriceChange>) -> String {
    change.map_or_else(
        || "—".to_owned(),
        |change| {
            format!(
                "{} {} ({}%)",
                direction_symbol(change.direction),
                signed_decimal(change.absolute),
                signed_decimal(change.percentage)
            )
        },
    )
}

pub(crate) fn menu_bar_title(price: Decimal, change: Option<&PriceChange>, stale: bool) -> String {
    let suffix = if stale {
        "·".to_owned()
    } else {
        change.map_or_else(
            || "—".to_owned(),
            |change| {
                format!(
                    "{}{}",
                    direction_symbol(change.direction),
                    signed_decimal(change.absolute)
                )
            },
        )
    };
    format!("Au {} {suffix}", format_price(price))
}

pub(crate) fn direction_symbol(direction: Direction) -> &'static str {
    match direction {
        Direction::Up => "▲",
        Direction::Down => "▼",
        Direction::Flat => "—",
    }
}

pub(crate) fn signed_decimal(value: Decimal) -> String {
    if value.is_sign_positive() {
        format!("+{}", format_price(value))
    } else {
        format_price(value)
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::{format_change, format_price, menu_bar_title};
    use crate::domain::{Direction, PriceChange};

    #[test]
    fn groups_and_preserves_decimal_scale() {
        assert_eq!(format_price(Decimal::new(436_990, 2)), "4,369.90");
        assert_eq!(format_price(Decimal::new(-120_000, 3)), "-120.000");
        assert_eq!(format_price(Decimal::new(1_000_000, 0)), "1,000,000");
    }

    #[test]
    fn formats_signed_change_and_missing_change() {
        let change = PriceChange {
            absolute: Decimal::new(1_449, 2),
            percentage: Decimal::new(33, 2),
            direction: Direction::Up,
        };
        assert_eq!(format_change(Some(&change)), "▲ +14.49 (+0.33%)");
        assert_eq!(format_change(None), "—");
    }

    #[test]
    fn adds_only_neutral_symbols_to_menu_bar_titles() {
        let change = PriceChange {
            absolute: Decimal::new(-531, 2),
            percentage: Decimal::new(-21, 2),
            direction: Direction::Down,
        };
        assert_eq!(
            menu_bar_title(Decimal::new(247_282, 2), Some(&change), false),
            "Au 2,472.82 ▼-5.31"
        );
        assert_eq!(
            menu_bar_title(Decimal::new(247_282, 2), Some(&change), true),
            "Au 2,472.82 ·"
        );
    }
}
