//! Parser for the Tencent `hf_XAU` quote response.
//!
//! The endpoint returns a GB18030-encoded JavaScript variable assignment, for
//! example `v_hf_XAU="...";`. This module treats that value as data only.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TencentGoldQuote {
    pub(crate) last_price: String,
    pub(crate) percentage_change: String,
    pub(crate) intraday_high: String,
    pub(crate) intraday_low: String,
    pub(crate) quote_time: String,
    pub(crate) previous_close: String,
    pub(crate) quote_date: String,
    pub(crate) display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ParseError {
    UnexpectedVariable,
    MissingOpeningQuote,
    MissingClosingQuote,
    UnexpectedFieldCount { actual: usize },
    EmptyRequiredField { index: usize },
}

/// Parses the decoded `v_hf_XAU` assignment without evaluating JavaScript.
///
/// Confirmed R0 field mapping: index 0 last price; index 1 percentage change;
/// indices 4/5 intraday high/low; index 6 quote time; index 7 previous close;
/// index 12 quote date; index 13 display name.
pub(crate) fn parse_hf_xau_response(input: &str) -> Result<TencentGoldQuote, ParseError> {
    let assignment = input.trim();
    let payload = assignment
        .strip_prefix("v_hf_XAU=\"")
        .ok_or(ParseError::UnexpectedVariable)?;
    let payload = payload
        .strip_suffix("\";")
        .ok_or(ParseError::MissingClosingQuote)?;

    if payload.is_empty() {
        return Err(ParseError::MissingOpeningQuote);
    }

    let fields: Vec<&str> = payload.split(',').collect();
    if fields.len() != 14 {
        return Err(ParseError::UnexpectedFieldCount {
            actual: fields.len(),
        });
    }

    const REQUIRED_INDICES: [usize; 9] = [0, 1, 4, 5, 6, 7, 12, 13, 2];
    for index in REQUIRED_INDICES {
        if fields[index].trim().is_empty() {
            return Err(ParseError::EmptyRequiredField { index });
        }
    }

    Ok(TencentGoldQuote {
        last_price: fields[0].to_owned(),
        percentage_change: fields[1].to_owned(),
        intraday_high: fields[4].to_owned(),
        intraday_low: fields[5].to_owned(),
        quote_time: fields[6].to_owned(),
        previous_close: fields[7].to_owned(),
        quote_date: fields[12].to_owned(),
        display_name: fields[13].to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::{ParseError, parse_hf_xau_response};

    const FIXTURE: &str = include_str!("../tests/fixtures/tencent_hf_xau.txt");

    #[test]
    fn parses_confirmed_hf_xau_fixture() {
        let quote = parse_hf_xau_response(FIXTURE).expect("fixture must parse");

        assert_eq!(quote.last_price, "4369.90");
        assert_eq!(quote.percentage_change, "0.33");
        assert_eq!(quote.intraday_high, "4378.00");
        assert_eq!(quote.intraday_low, "4341.31");
        assert_eq!(quote.quote_time, "10:33:00");
        assert_eq!(quote.previous_close, "4355.41");
        assert_eq!(quote.quote_date, "2026-09-09");
        assert_eq!(quote.display_name, "伦敦金（现货黄金）");
    }

    #[test]
    fn rejects_unexpected_variable_name() {
        let error = parse_hf_xau_response("v_hf_SI=\"1,2,3\";").unwrap_err();

        assert_eq!(error, ParseError::UnexpectedVariable);
    }

    #[test]
    fn rejects_unexpected_field_count() {
        let error = parse_hf_xau_response("v_hf_XAU=\"1,2,3\";").unwrap_err();

        assert_eq!(error, ParseError::UnexpectedFieldCount { actual: 3 });
    }
}
