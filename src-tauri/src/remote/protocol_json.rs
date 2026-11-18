//! Reject ambiguous duplicate object keys before method-specific deserialization.
use serde::de::{Error, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use std::fmt;

pub(super) struct StrictValue(pub Value);
impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct JsonVisitor;
        impl<'de> Visitor<'de> for JsonVisitor {
            type Value = StrictValue;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("unambiguous JSON")
            }
            fn visit_bool<E: Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Bool(v)))
            }
            fn visit_i64<E: Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(StrictValue(v.into()))
            }
            fn visit_u64<E: Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(StrictValue(v.into()))
            }
            fn visit_f64<E: Error>(self, v: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| StrictValue(Value::Number(n)))
                    .ok_or_else(|| E::custom("non-finite number"))
            }
            fn visit_str<E: Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(StrictValue(v.into()))
            }
            fn visit_string<E: Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(StrictValue(v.into()))
            }
            fn visit_unit<E: Error>(self) -> Result<Self::Value, E> {
                Ok(StrictValue(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut input: A) -> Result<Self::Value, A::Error> {
                let mut result = Vec::new();
                while let Some(StrictValue(value)) = input.next_element()? {
                    result.push(value);
                }
                Ok(StrictValue(Value::Array(result)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut input: A) -> Result<Self::Value, A::Error> {
                let mut result = serde_json::Map::new();
                while let Some((key, StrictValue(value))) =
                    input.next_entry::<String, StrictValue>()?
                {
                    if result.insert(key, value).is_some() {
                        return Err(A::Error::custom("duplicate object key"));
                    }
                }
                Ok(StrictValue(Value::Object(result)))
            }
        }
        d.deserialize_any(JsonVisitor)
    }
}
