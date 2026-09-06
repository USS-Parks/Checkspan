//! Bounded, strict JSON parsing: depth limit, duplicate-key rejection, no
//! trailing content, and JSON-pointer paths on every failure.

use std::cell::RefCell;
use std::fmt;

use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

/// Why parsing stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseErrorKind {
    /// The text is not JSON, or has content after the document.
    Syntax,
    /// Containers nest deeper than the limit.
    Depth,
    /// An object repeats a key.
    DuplicateKey,
}

/// A parse failure with the JSON pointer of the offending value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// What went wrong.
    pub kind: ParseErrorKind,
    /// JSON pointer to the container or key involved (`""` is the root).
    pub path: String,
    /// Human-readable detail.
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {:?}", self.message, self.path)
    }
}

impl std::error::Error for ParseError {}

/// Parse `text` into a JSON value, rejecting duplicate keys, nesting deeper
/// than `max_depth` containers, and trailing content.
pub fn parse_strict(text: &str, max_depth: usize) -> Result<Value, ParseError> {
    let slot = RefCell::new(None);
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let seed = StrictValue {
        depth: 0,
        max_depth,
        path: String::new(),
        slot: &slot,
    };
    let result = seed.deserialize(&mut deserializer);
    if let Some(structured) = slot.into_inner() {
        return Err(structured);
    }
    let value = result.map_err(syntax)?;
    deserializer.end().map_err(syntax)?;
    Ok(value)
}

fn syntax(err: serde_json::Error) -> ParseError {
    ParseError {
        kind: ParseErrorKind::Syntax,
        path: String::new(),
        message: err.to_string(),
    }
}

/// Append one JSON pointer segment with RFC 6901 escaping.
pub fn pointer_push(base: &str, segment: &str) -> String {
    let escaped = segment.replace('~', "~0").replace('/', "~1");
    format!("{base}/{escaped}")
}

struct StrictValue<'a> {
    depth: usize,
    max_depth: usize,
    path: String,
    slot: &'a RefCell<Option<ParseError>>,
}

impl<'a> StrictValue<'a> {
    fn child(&self, segment: &str) -> StrictValue<'a> {
        StrictValue {
            depth: self.depth + 1,
            max_depth: self.max_depth,
            path: pointer_push(&self.path, segment),
            slot: self.slot,
        }
    }

    fn fail<E: de::Error>(&self, kind: ParseErrorKind, message: String) -> E {
        let error = E::custom(&message);
        *self.slot.borrow_mut() = Some(ParseError {
            kind,
            path: self.path.clone(),
            message,
        });
        error
    }

    fn enter_container<E: de::Error>(&self) -> Result<(), E> {
        if self.depth + 1 > self.max_depth {
            return Err(self.fail(
                ParseErrorKind::Depth,
                format!("nesting exceeds the limit of {} levels", self.max_depth),
            ));
        }
        Ok(())
    }
}

impl<'de, 'a> DeserializeSeed<'de> for StrictValue<'a> {
    type Value = Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de, 'a> Visitor<'de> for StrictValue<'a> {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any JSON value")
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        Number::from_f64(v).map(Value::Number).ok_or_else(|| {
            self.fail(
                ParseErrorKind::Syntax,
                format!("number {v} cannot be represented"),
            )
        })
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(self)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        self.enter_container()?;
        let mut items = Vec::new();
        let mut index = 0usize;
        while let Some(item) = seq.next_element_seed(self.child(&index.to_string()))? {
            items.push(item);
            index += 1;
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        self.enter_container()?;
        let mut object = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if object.contains_key(&key) {
                let child = self.child(&key);
                return Err(child.fail(
                    ParseErrorKind::DuplicateKey,
                    format!("key {key:?} appears more than once in one object"),
                ));
            }
            let value = map.next_value_seed(self.child(&key))?;
            object.insert(key, value);
        }
        Ok(Value::Object(object))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_documents() {
        let v = parse_strict(r#"{"a":[1,2,{"b":null}],"c":"d"}"#, 8).unwrap();
        assert_eq!(v["a"][2]["b"], Value::Null);
    }

    #[test]
    fn rejects_duplicate_keys_with_path() {
        let err = parse_strict(r#"{"a":{"x":1,"y":2,"x":3}}"#, 8).unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::DuplicateKey);
        assert_eq!(err.path, "/a/x");
    }

    #[test]
    fn rejects_depth_beyond_limit() {
        assert!(parse_strict("[[[1]]]", 3).is_ok());
        let err = parse_strict("[[[[1]]]]", 3).unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::Depth);
        assert_eq!(err.path, "/0/0/0");
    }

    #[test]
    fn rejects_trailing_content_and_syntax() {
        assert_eq!(
            parse_strict("{} {}", 8).unwrap_err().kind,
            ParseErrorKind::Syntax
        );
        assert_eq!(
            parse_strict("{\"a\":}", 8).unwrap_err().kind,
            ParseErrorKind::Syntax
        );
        assert_eq!(
            parse_strict("", 8).unwrap_err().kind,
            ParseErrorKind::Syntax
        );
    }

    #[test]
    fn escapes_pointer_segments() {
        assert_eq!(pointer_push("", "a/b~c"), "/a~1b~0c");
    }
}
