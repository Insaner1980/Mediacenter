//! Validated LAN request boundary. Never forward a method name to Tauri or a shell.
use serde::{de::Error as _, Deserialize, Deserializer, Serialize};
use std::fmt;

pub use super::protocol_methods::*;
pub const MAX_REQUEST_BYTES: usize = 64 * 1024;
pub const PROTOCOL_MAJOR: u8 = 1;

pub use WireErrorCode as ErrorCode;
pub type RemoteError = ErrorPayload;
impl RemoteError {
    pub fn invalid() -> Self {
        Self::new(ErrorCode::InvalidRequest, "Invalid remote request.")
    }
    pub fn new(code: ErrorCode, message: &str) -> Self {
        Self {
            code,
            message: BoundedText(message.into()),
            retryable: false,
            refresh: BoundedList(vec![]),
            replacement_preview: None,
        }
    }
}
impl fmt::Display for RemoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message.as_str())
    }
}
impl std::error::Error for RemoteError {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Id(String);
impl Id {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl<'de> Deserialize<'de> for Id {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        if s.is_empty()
            || s.len() > 128
            || !s
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        {
            return Err(D::Error::custom("invalid opaque identifier"));
        }
        Ok(Self(s))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Decimal(String);
impl Decimal {
    pub fn value(&self) -> u64 {
        self.0.parse().expect("validated decimal")
    }
}
impl From<u64> for Decimal {
    fn from(value: u64) -> Self {
        Self(value.to_string())
    }
}
impl<'de> Deserialize<'de> for Decimal {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        if s.is_empty()
            || (s.len() > 1 && s.starts_with('0'))
            || !s.bytes().all(|c| c.is_ascii_digit())
            || s.parse::<u64>().is_err()
        {
            return Err(D::Error::custom("invalid decimal counter"));
        }
        Ok(Self(s))
    }
}

/// Text deliberately redacts Debug output, including search and browser secrets.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct BoundedText<const MIN: usize, const MAX: usize>(String);
impl<const MIN: usize, const MAX: usize> BoundedText<MIN, MAX> {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl<const MIN: usize, const MAX: usize> fmt::Debug for BoundedText<MIN, MAX> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}
impl<'de, const MIN: usize, const MAX: usize> Deserialize<'de> for BoundedText<MIN, MAX> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        if !(MIN..=MAX).contains(&s.chars().count()) {
            return Err(D::Error::custom("text length out of bounds"));
        }
        Ok(Self(s))
    }
}
pub type Text = BoundedText<0, 4096>;
pub type Query = BoundedText<0, 200>;
pub type Label = BoundedText<1, 64>;
pub type Bool = bool;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Cursor(String);
impl<'de> Deserialize<'de> for Cursor {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        if s.len() > 2048 {
            return Err(D::Error::custom("cursor too large"));
        }
        Ok(Self(s))
    }
}
impl Cursor {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub struct BoundedNumber<const MIN: i64, const MAX: i64>(f64);
impl<const MIN: i64, const MAX: i64> BoundedNumber<MIN, MAX> {
    pub fn value(&self) -> f64 {
        self.0
    }
}
impl<'de, const MIN: i64, const MAX: i64> Deserialize<'de> for BoundedNumber<MIN, MAX> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let n = f64::deserialize(d)?;
        if !n.is_finite() || n < MIN as f64 || n > MAX as f64 {
            return Err(D::Error::custom("number out of bounds"));
        }
        Ok(Self(n))
    }
}
pub type Volume = BoundedNumber<0, 100>;
pub type Position = BoundedNumber<0, 9_007_199_254_740_991>;
pub type UnitCoordinate = BoundedNumber<0, 1>;
pub type ScrollDelta = BoundedNumber<-4096, 4096>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct BoundedInteger<const MIN: u32, const MAX: u32>(u32);
impl<const MIN: u32, const MAX: u32> BoundedInteger<MIN, MAX> {
    pub fn value(&self) -> u32 {
        self.0
    }
}
impl<'de, const MIN: u32, const MAX: u32> Deserialize<'de> for BoundedInteger<MIN, MAX> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let n = u32::deserialize(d)?;
        if !(MIN..=MAX).contains(&n) {
            return Err(D::Error::custom("integer out of bounds"));
        }
        Ok(Self(n))
    }
}
pub type PageLimit = BoundedInteger<1, 100>;
pub type Interval = BoundedInteger<1, 60>;
pub type Version = BoundedInteger<1, 1>;
pub type MinorVersion = BoundedInteger<0, 65535>;
pub type DisplayText = BoundedText<0, 2000>;
pub type PlaylistName = BoundedText<1, 200>;
pub type ConfirmationTtlMs = BoundedInteger<0, 60000>;
pub type PairingTtlMs = BoundedInteger<0, 120000>;
pub type Port = BoundedInteger<1, 65535>;

macro_rules! validated_string {
    ($name:ident, $valid:expr) => {
        #[derive(Clone, PartialEq, Eq, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);
        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("[redacted]")
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                if !($valid)(&s) {
                    return Err(D::Error::custom("invalid wire string"));
                }
                Ok(Self(s))
            }
        }
    };
}
validated_string!(LocalAddress, |s: &str| (2..=64).contains(&s.len())
    && s.bytes()
        .all(|c| c.is_ascii_alphanumeric() || b":.%_-".contains(&c)));
validated_string!(Fingerprint, |s: &str| s.len() == 64
    && s.bytes()
        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)));
validated_string!(PairingSecret, |s: &str| s.len() == 43
    && s.bytes()
        .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c)));
validated_string!(BearerCredential, |s: &str| s.len() == 43
    && s.bytes()
        .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c)));
validated_string!(ManualCode, |s: &str| s.len() == 12
    && s.bytes()
        .all(|c| c.is_ascii_uppercase() || (b'2'..=b'7').contains(&c)));

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct TimestampMs(u64);
impl<'de> Deserialize<'de> for TimestampMs {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = u64::deserialize(d)?;
        if value > 9_007_199_254_740_991 {
            return Err(D::Error::custom("timestamp exceeds JSON integer precision"));
        }
        Ok(Self(value))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub struct BoundedList<T, const MIN: usize, const MAX: usize, const UNIQUE: bool>(Vec<T>);
impl<T, const MIN: usize, const MAX: usize, const UNIQUE: bool> BoundedList<T, MIN, MAX, UNIQUE> {
    pub fn as_slice(&self) -> &[T] {
        &self.0
    }
}
impl<
        'de,
        T: Deserialize<'de> + PartialEq,
        const MIN: usize,
        const MAX: usize,
        const UNIQUE: bool,
    > Deserialize<'de> for BoundedList<T, MIN, MAX, UNIQUE>
{
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let items = Vec::<T>::deserialize(d)?;
        if !(MIN..=MAX).contains(&items.len())
            || (UNIQUE
                && items
                    .iter()
                    .enumerate()
                    .any(|(i, item)| items[..i].contains(item)))
        {
            return Err(D::Error::custom("invalid bounded list"));
        }
        Ok(Self(items))
    }
}
pub type IdBatch = BoundedList<Id, 1, 100, false>;
pub type MediaKinds = BoundedList<MediaKind, 1, 3, true>;
pub type ScopeSet = BoundedList<Scope, 0, 7, true>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    Read,
    Durable,
    Transient,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RemoteRequest {
    pub v: u8,
    pub id: Id,
    #[serde(flatten)]
    pub command: RemoteCommand,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<Expected>,
}

pub(super) fn validate_document(
    input: &str,
    max_bytes: usize,
) -> Result<serde_json::Value, RemoteError> {
    if input.len() > max_bytes {
        return Err(RemoteError::invalid());
    }
    serde_json::from_str::<super::protocol_json::StrictValue>(input)
        .map(|v| v.0)
        .map_err(|_| RemoteError::invalid())
}

pub fn parse_request(input: &str) -> Result<RemoteRequest, RemoteError> {
    if input.len() > MAX_REQUEST_BYTES {
        return Err(RemoteError::invalid());
    }
    // The preliminary envelope only selects errors; the original JSON is then
    // deserialized into the allowlisted enum with validated method-specific fields.
    let header = validate_document(input, MAX_REQUEST_BYTES)?;
    if header.get("v").and_then(|v| v.as_u64()) != Some(PROTOCOL_MAJOR.into()) {
        return Err(RemoteError::new(
            ErrorCode::VersionMismatch,
            "Incompatible remote protocol version.",
        ));
    }
    let method = header
        .get("method")
        .and_then(|value| value.as_str())
        .ok_or_else(RemoteError::invalid)?;
    serde_json::from_value::<MethodName>(serde_json::Value::String(method.into()))
        .map_err(|_| RemoteError::new(ErrorCode::Unsupported, "Unsupported remote method."))?;
    let request: RemoteRequest = serde_json::from_str(input).map_err(|error| {
        if error.to_string().starts_with("unknown variant") {
            RemoteError::new(ErrorCode::Unsupported, "Unsupported remote action.")
        } else {
            RemoteError::invalid()
        }
    })?;
    request.command.validate(request.expected.as_ref())?;
    Ok(request)
}

/// The success flag and payload cannot disagree, including on deserialization.
#[derive(Clone, Debug, PartialEq)]
pub enum RemoteReply<T = RemoteResult> {
    Success { id: Id, result: T },
    Failure { id: Id, error: RemoteError },
}

impl<T> RemoteReply<T> {
    pub fn map_result<U>(self, map: impl FnOnce(T) -> U) -> RemoteReply<U> {
        match self {
            Self::Success { id, result } => RemoteReply::Success {
                id,
                result: map(result),
            },
            Self::Failure { id, error } => RemoteReply::Failure { id, error },
        }
    }
}

impl<T: Serialize> Serialize for RemoteReply<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("RemoteReply", 4)?;
        state.serialize_field("v", &PROTOCOL_MAJOR)?;
        match self {
            Self::Success { id, result } => {
                state.serialize_field("id", id)?;
                state.serialize_field("ok", &true)?;
                state.serialize_field("result", result)?;
            }
            Self::Failure { id, error } => {
                state.serialize_field("id", id)?;
                state.serialize_field("ok", &false)?;
                state.serialize_field("error", error)?;
            }
        }
        state.end()
    }
}

#[derive(Default)]
enum Present<T> {
    #[default]
    Missing,
    Value(T),
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Present<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        T::deserialize(d).map(Self::Value)
    }
}
#[derive(Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
struct ReplyWire<T> {
    #[serde(rename = "v")]
    _version: Version,
    id: Id,
    ok: bool,
    #[serde(default)]
    result: Present<T>,
    #[serde(default)]
    error: Present<RemoteError>,
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for RemoteReply<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let reply = ReplyWire::<T>::deserialize(deserializer)?;
        match (reply.ok, reply.result, reply.error) {
            (true, Present::Value(result), Present::Missing) => Ok(Self::Success {
                id: reply.id,
                result,
            }),
            (false, Present::Missing, Present::Value(error)) => Ok(Self::Failure {
                id: reply.id,
                error,
            }),
            _ => Err(D::Error::custom(
                "reply requires exactly one matching result or error",
            )),
        }
    }
}
