use std::fmt;
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use encoding_rs::GB18030;
use reqwest::blocking::Client;
use rust_decimal::Decimal;

use crate::tencent_quote::{ParseError, parse_hf_xau_response};

const ENDPOINT: &str = "https://qt.gtimg.cn/q=hf_XAU";
const RESPONSE_LIMIT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Quote {
    pub(crate) symbol: String,
    pub(crate) display_name: String,
    pub(crate) price: Decimal,
    pub(crate) percentage_change: Decimal,
    pub(crate) previous_close: Decimal,
    pub(crate) quote_date: String,
    pub(crate) quote_time: String,
    pub(crate) fetched_at_unix_secs: u64,
    pub(crate) provider: String,
}

#[derive(Debug)]
pub(crate) enum ProviderError {
    Client(reqwest::Error),
    ResponseTooLarge,
    InvalidEncoding,
    Parse(ParseError),
    InvalidDecimal { field: &'static str, value: String },
    ClockBeforeUnixEpoch,
}

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Client(error) => write!(formatter, "Tencent quote request failed: {error}"),
            Self::ResponseTooLarge => write!(formatter, "Tencent quote response exceeded 64 KiB"),
            Self::InvalidEncoding => {
                write!(formatter, "Tencent quote response was not valid GB18030")
            }
            Self::Parse(error) => write!(
                formatter,
                "Tencent quote response could not be parsed: {error:?}"
            ),
            Self::InvalidDecimal { field, value } => {
                write!(
                    formatter,
                    "Tencent quote field {field} was not a decimal: {value}"
                )
            }
            Self::ClockBeforeUnixEpoch => {
                write!(formatter, "system clock was before the Unix epoch")
            }
        }
    }
}

impl std::error::Error for ProviderError {}

impl From<reqwest::Error> for ProviderError {
    fn from(error: reqwest::Error) -> Self {
        Self::Client(error)
    }
}

#[derive(Clone)]
pub(crate) struct TencentGoldProvider {
    client: Client,
}

impl TencentGoldProvider {
    pub(crate) fn new(timeout: Duration) -> Result<Self, ProviderError> {
        let client = Client::builder()
            .timeout(timeout)
            .user_agent("GoldTicker/0.1 (macOS)")
            .build()?;
        Ok(Self { client })
    }

    pub(crate) fn fetch_quote(&self) -> Result<Quote, ProviderError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ProviderError::ClockBeforeUnixEpoch)?;
        let timestamp_millis = now.as_millis();
        let response = self
            .client
            .get(format!("{ENDPOINT}?_={timestamp_millis}"))
            .send()?
            .error_for_status()?;

        let bytes = response.bytes()?;
        if bytes.len() > RESPONSE_LIMIT_BYTES {
            return Err(ProviderError::ResponseTooLarge);
        }

        let (decoded, _, had_errors) = GB18030.decode(&bytes);
        if had_errors {
            return Err(ProviderError::InvalidEncoding);
        }

        Self::parse_decoded_response(&decoded, now.as_secs())
    }

    pub(crate) fn parse_decoded_response(
        decoded: &str,
        fetched_at_unix_secs: u64,
    ) -> Result<Quote, ProviderError> {
        let raw = parse_hf_xau_response(decoded).map_err(ProviderError::Parse)?;

        Ok(Quote {
            symbol: "hf_XAU".to_owned(),
            display_name: raw.display_name,
            price: parse_decimal("last_price", raw.last_price)?,
            percentage_change: parse_decimal("percentage_change", raw.percentage_change)?,
            previous_close: parse_decimal("previous_close", raw.previous_close)?,
            quote_date: raw.quote_date,
            quote_time: raw.quote_time,
            fetched_at_unix_secs,
            provider: "腾讯行情".to_owned(),
        })
    }
}

fn parse_decimal(field: &'static str, value: String) -> Result<Decimal, ProviderError> {
    Decimal::from_str(&value).map_err(|_| ProviderError::InvalidDecimal { field, value })
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::{ProviderError, TencentGoldProvider};

    const FIXTURE: &str = include_str!("../tests/fixtures/tencent_hf_xau.txt");

    #[test]
    fn maps_confirmed_fixture_to_normalized_quote() {
        let quote = TencentGoldProvider::parse_decoded_response(FIXTURE, 1_725_000_000)
            .expect("fixture must parse into quote");

        assert_eq!(quote.symbol, "hf_XAU");
        assert_eq!(quote.display_name, "伦敦金（现货黄金）");
        assert_eq!(quote.price, Decimal::new(436_990, 2));
        assert_eq!(quote.percentage_change, Decimal::new(33, 2));
        assert_eq!(quote.previous_close, Decimal::new(435_541, 2));
        assert_eq!(quote.quote_date, "2026-09-09");
        assert_eq!(quote.quote_time, "10:33:00");
        assert_eq!(quote.fetched_at_unix_secs, 1_725_000_000);
        assert_eq!(quote.provider, "腾讯行情");
    }

    #[test]
    fn rejects_non_decimal_price() {
        let response = FIXTURE.replacen("4369.90", "not-a-price", 1);
        let error = TencentGoldProvider::parse_decoded_response(&response, 0).unwrap_err();

        assert!(matches!(
            error,
            ProviderError::InvalidDecimal {
                field: "last_price",
                value
            } if value == "not-a-price"
        ));
    }
}
