//! Identity primitives shared by every contract record.

use std::fmt;

use serde::{Deserialize, Serialize};

const MAX_IDENTIFIER_LEN: usize = 128;

/// A primitive value that violates its identity rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidValue {
    /// Which primitive was being built (`graph_id`, `revision`, ...).
    pub kind: &'static str,
    /// Why the value was rejected.
    pub reason: String,
}

impl fmt::Display for InvalidValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid {}: {}", self.kind, self.reason)
    }
}

impl std::error::Error for InvalidValue {}

fn check_identifier(kind: &'static str, value: &str) -> Result<(), InvalidValue> {
    let reject = |reason: String| Err(InvalidValue { kind, reason });
    let Some(first) = value.chars().next() else {
        return reject("must not be empty".into());
    };
    if value.len() > MAX_IDENTIFIER_LEN {
        return reject(format!("must be at most {MAX_IDENTIFIER_LEN} bytes"));
    }
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return reject("must start with a lowercase ASCII letter or digit".into());
    }
    if let Some(bad) = value
        .chars()
        .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '.' | '-')))
    {
        return reject(format!(
            "contains {bad:?}; only lowercase ASCII letters, digits, '_', '.', '-' are allowed"
        ));
    }
    Ok(())
}

macro_rules! identifier {
    ($name:ident, $kind:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(try_from = "String")]
        pub struct $name(String);

        impl $name {
            /// Build the identifier, rejecting values outside the identifier grammar.
            pub fn new(value: impl Into<String>) -> Result<Self, InvalidValue> {
                let value = value.into();
                check_identifier($kind, &value)?;
                Ok(Self(value))
            }

            /// The identifier text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = InvalidValue;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

identifier!(
    GraphId,
    "graph_id",
    "Identifies a graph across all of its revisions."
);
identifier!(
    NodeId,
    "node_id",
    "Graph-local node name. Unique only within one graph; a full identity needs a `NodeRef`."
);
identifier!(
    RunId,
    "run_id",
    "Identifies one execution of one graph revision."
);
identifier!(
    Ident,
    "identifier",
    "General machine identifier: port, check, resource, schema, policy, and verifier names."
);

macro_rules! positive_u32 {
    ($name:ident, $kind:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        #[serde(try_from = "u32")]
        pub struct $name(u32);

        impl $name {
            /// Build the value; zero is rejected.
            pub fn new(value: u32) -> Result<Self, InvalidValue> {
                if value == 0 {
                    return Err(InvalidValue {
                        kind: $kind,
                        reason: "must be at least 1".into(),
                    });
                }
                Ok(Self(value))
            }

            /// The number.
            pub fn get(self) -> u32 {
                self.0
            }
        }

        impl TryFrom<u32> for $name {
            type Error = InvalidValue;

            fn try_from(value: u32) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

positive_u32!(
    Revision,
    "revision",
    "Monotonic contract revision, starting at 1."
);
positive_u32!(
    Version,
    "version",
    "Version of a schema, policy, or verifier, starting at 1."
);
positive_u32!(
    AttemptNumber,
    "attempt number",
    "Attempt number of one node within one run, starting at 1. Every started attempt counts."
);

/// Content digest: `sha256:` followed by 64 lowercase hex digits.
///
/// A digest identifies bytes. It does not establish who produced them.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct Digest(String);

impl Digest {
    /// Build a digest from its `sha256:<hex>` text form.
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidValue> {
        let value = value.into();
        let hex = value.strip_prefix("sha256:").unwrap_or("");
        let well_formed = hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if well_formed {
            Ok(Self(value))
        } else {
            Err(InvalidValue {
                kind: "digest",
                reason: format!("{value:?} is not 'sha256:' followed by 64 lowercase hex digits"),
            })
        }
    }

    /// The `sha256:<hex>` text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Build a digest from raw SHA-256 output.
    pub fn from_sha256(hash: &[u8; 32]) -> Self {
        let mut text = String::with_capacity(7 + 64);
        text.push_str("sha256:");
        for byte in hash {
            text.push_str(&format!("{byte:02x}"));
        }
        Self(text)
    }
}

impl TryFrom<String> for Digest {
    type Error = InvalidValue;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A field that must carry exactly the integer `V`.
///
/// Used for inline policy versions: a document written for a policy version
/// this build does not understand is rejected while parsing, before any of
/// its other fields are interpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Exactly<const V: u32>;

impl<const V: u32> Serialize for Exactly<V> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(V)
    }
}

impl<'de, const V: u32> Deserialize<'de> for Exactly<V> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let found = u32::deserialize(deserializer)?;
        if found == V {
            Ok(Self)
        } else {
            Err(serde::de::Error::custom(format!(
                "version {found} is not supported; this build supports {V}"
            )))
        }
    }
}

/// RFC 3339 date-time with an explicit offset, kept as its original text.
///
/// Only the shape and field ranges are checked here; no calendar arithmetic
/// or clock is involved.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String")]
pub struct Timestamp(String);

impl Timestamp {
    /// Build a timestamp from RFC 3339 text such as `2026-09-06T12:00:00Z`.
    pub fn new(value: impl Into<String>) -> Result<Self, InvalidValue> {
        let value = value.into();
        if rfc3339_shape(value.as_bytes()) {
            Ok(Self(value))
        } else {
            Err(InvalidValue {
                kind: "timestamp",
                reason: format!("{value:?} is not an RFC 3339 date-time with an explicit offset"),
            })
        }
    }

    /// The original RFC 3339 text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The instant this timestamp names, as nanoseconds since the Unix
    /// epoch in UTC. Two texts with different offsets that name the same
    /// instant compare equal here even though they are different strings.
    pub fn unix_nanos(&self) -> i128 {
        let b = self.0.as_bytes();
        let num = |range: std::ops::Range<usize>| -> i64 {
            b[range]
                .iter()
                .fold(0i64, |acc, d| acc * 10 + i64::from(d - b'0'))
        };
        let (year, month, day) = (num(0..4), num(5..7), num(8..10));
        let (hour, minute, second) = (num(11..13), num(14..16), num(17..19));
        let mut i = 19;
        let mut nanos: i128 = 0;
        if b.get(i) == Some(&b'.') {
            i += 1;
            let start = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            let digits = &self.0[start..i];
            let mut fraction: i128 = digits.parse().unwrap_or(0);
            for _ in digits.len()..9 {
                fraction *= 10;
            }
            nanos = fraction;
        }
        let offset_seconds: i64 = match b[i] {
            b'Z' | b'z' => 0,
            sign => {
                let magnitude = num(i + 1..i + 3) * 3600 + num(i + 4..i + 6) * 60;
                if sign == b'-' { -magnitude } else { magnitude }
            }
        };
        // Days from civil date (Howard Hinnant's algorithm).
        let y = if month <= 2 { year - 1 } else { year };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = y - era * 400;
        let mp = (month + 9) % 12;
        let doy = (153 * mp + 2) / 5 + day - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = era * 146_097 + doe - 719_468;
        let seconds = days * 86_400 + hour * 3_600 + minute * 60 + second - offset_seconds;
        i128::from(seconds) * 1_000_000_000 + nanos
    }

    /// Whether this instant is strictly before `other`.
    pub fn is_before(&self, other: &Timestamp) -> bool {
        self.unix_nanos() < other.unix_nanos()
    }
}

impl TryFrom<String> for Timestamp {
    type Error = InvalidValue;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn two_digits(bytes: &[u8], at: usize, min: u32, max: u32) -> bool {
    match bytes.get(at..at + 2) {
        Some([a, b]) if a.is_ascii_digit() && b.is_ascii_digit() => {
            let n = u32::from(a - b'0') * 10 + u32::from(b - b'0');
            (min..=max).contains(&n)
        }
        _ => false,
    }
}

fn rfc3339_shape(b: &[u8]) -> bool {
    // YYYY-MM-DDTHH:MM:SS[.fraction](Z|+HH:MM|-HH:MM)
    let date_time = b.len() >= 20
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && two_digits(b, 5, 1, 12)
        && b[7] == b'-'
        && two_digits(b, 8, 1, 31)
        && (b[10] == b'T' || b[10] == b't')
        && two_digits(b, 11, 0, 23)
        && b[13] == b':'
        && two_digits(b, 14, 0, 59)
        && b[16] == b':'
        && two_digits(b, 17, 0, 60);
    if !date_time {
        return false;
    }
    let mut i = 19;
    if b.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start || i - start > 9 {
            return false;
        }
    }
    let tail = &b[i..];
    match tail {
        [b'Z'] | [b'z'] => true,
        [b'+' | b'-', ..] => {
            tail.len() == 6
                && two_digits(tail, 1, 0, 23)
                && tail[3] == b':'
                && two_digits(tail, 4, 0, 59)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_grammar() {
        assert!(GraphId::new("cs_patch.v1-a").is_ok());
        assert!(GraphId::new("").is_err());
        assert!(GraphId::new("Graph").is_err());
        assert!(GraphId::new("_x").is_err());
        assert!(GraphId::new("a b").is_err());
        assert!(GraphId::new("a".repeat(128)).is_ok());
        assert!(GraphId::new("a".repeat(129)).is_err());
    }

    #[test]
    fn revision_starts_at_one() {
        assert!(Revision::new(0).is_err());
        assert_eq!(Revision::new(1).unwrap().get(), 1);
    }

    #[test]
    fn timestamp_instants_order_across_offsets() {
        let t = |s: &str| Timestamp::new(s).unwrap();
        assert_eq!(t("1970-01-01T00:00:00Z").unix_nanos(), 0);
        assert_eq!(t("1970-01-01T00:00:01.5Z").unix_nanos(), 1_500_000_000);
        assert_eq!(
            t("2026-09-06T12:00:00+02:00").unix_nanos(),
            t("2026-09-06T10:00:00Z").unix_nanos()
        );
        assert_eq!(
            t("2026-09-06T00:30:00-05:00").unix_nanos(),
            t("2026-09-06T05:30:00Z").unix_nanos()
        );
        assert_eq!(
            t("2000-03-01T00:00:00Z").unix_nanos(),
            951_868_800 * 1_000_000_000
        );
        assert!(t("2026-09-06T10:00:00Z").is_before(&t("2026-09-06T10:00:00.000000001Z")));
        assert!(!t("2026-09-06T12:00:00+02:00").is_before(&t("2026-09-06T10:00:00Z")));
        assert!(t("1969-12-31T23:59:59Z").unix_nanos() < 0);
    }

    #[test]
    fn timestamp_shape() {
        for ok in [
            "2026-09-06T12:00:00Z",
            "2026-09-06T12:00:00.5Z",
            "2026-09-06T12:00:00.123456789+05:30",
            "2026-12-31T23:59:60-08:00",
        ] {
            assert!(Timestamp::new(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "2026-09-06",
            "2026-09-06T12:00:00",
            "2026-09-06 12:00:00Z",
            "2026-13-06T12:00:00Z",
            "2026-09-06T24:00:00Z",
            "2026-09-06T12:00:00.Z",
            "2026-09-06T12:00:00.1234567890Z",
            "2026-09-06T12:00:00+0530",
            "2026-09-06T12:00:00+24:00",
        ] {
            assert!(Timestamp::new(bad).is_err(), "{bad}");
        }
    }
}
