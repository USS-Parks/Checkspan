//! Offline JSON Schema compilation with a frozen keyword set.
//!
//! Every `$ref` is served from the bundled registry. No network client or
//! filesystem resolver is compiled into this crate, and a schema that uses a
//! keyword outside the frozen set is refused before it is compiled.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::sync::OnceLock;

use jsonschema::{Draft, Retrieve, Uri, Validator};
use serde_json::Value;

use crate::contracts::registry;

/// The Draft 2020-12 keywords Checkspan schemas may use. Anything else is an
/// unsupported feature and is rejected at compile time.
pub const SUPPORTED_KEYWORDS: &[&str] = &[
    "$schema",
    "$id",
    "$ref",
    "$defs",
    "title",
    "description",
    "type",
    "properties",
    "required",
    "additionalProperties",
    "enum",
    "const",
    "allOf",
    "if",
    "then",
    "else",
    "not",
    "items",
    "minItems",
    "contains",
    "minContains",
    "maxContains",
    "minimum",
    "maximum",
    "minLength",
    "maxLength",
    "pattern",
    "format",
];

/// Why a schema could not be compiled.
#[derive(Debug)]
pub enum SchemaError {
    /// The schema uses keywords outside [`SUPPORTED_KEYWORDS`].
    UnsupportedKeywords(Vec<String>),
    /// The schema or one of its references failed to compile offline.
    Compile(String),
    /// The requested schema is not bundled.
    NotBundled(String),
}

impl fmt::Display for SchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SchemaError::UnsupportedKeywords(list) => {
                write!(f, "schema uses unsupported keywords: {}", list.join(", "))
            }
            SchemaError::Compile(msg) => write!(f, "schema failed to compile offline: {msg}"),
            SchemaError::NotBundled(id) => write!(f, "schema {id} is not bundled"),
        }
    }
}

impl Error for SchemaError {}

/// Serves `$ref` targets from the bundled registry only.
struct Bundled;

impl Retrieve for Bundled {
    fn retrieve(&self, uri: &Uri<String>) -> Result<Value, Box<dyn Error + Send + Sync>> {
        match registry::bundled(uri.as_str()) {
            Some(source) => serde_json::from_str(source).map_err(Into::into),
            None => Err(format!("schema {uri} is not bundled; nothing is fetched").into()),
        }
    }
}

/// Keywords found in `schema` that are outside the frozen set, each as
/// `<json pointer>:<keyword>`.
pub fn unsupported_keywords(schema: &Value) -> Vec<String> {
    let mut found = Vec::new();
    walk(schema, String::new(), &mut found);
    found
}

fn walk(node: &Value, path: String, found: &mut Vec<String>) {
    let Value::Object(map) = node else {
        return;
    };
    for (key, value) in map {
        let here = super::parse::pointer_push(&path, key);
        match key.as_str() {
            "properties" | "$defs" => {
                if let Value::Object(children) = value {
                    for (name, child) in children {
                        walk(child, super::parse::pointer_push(&here, name), found);
                    }
                }
            }
            "allOf" => {
                if let Value::Array(items) = value {
                    for (i, child) in items.iter().enumerate() {
                        walk(
                            child,
                            super::parse::pointer_push(&here, &i.to_string()),
                            found,
                        );
                    }
                }
            }
            "items" | "contains" | "not" | "if" | "then" | "else" | "additionalProperties" => {
                walk(value, here, found);
            }
            other if SUPPORTED_KEYWORDS.contains(&other) => {}
            other => found.push(format!("{here}:{other}")),
        }
    }
}

/// Compile `schema` with offline resolution and format assertions, refusing
/// unsupported keywords first.
pub fn compile(schema: &Value) -> Result<Validator, SchemaError> {
    let unsupported = unsupported_keywords(schema);
    if !unsupported.is_empty() {
        return Err(SchemaError::UnsupportedKeywords(unsupported));
    }
    jsonschema::options()
        .with_draft(Draft::Draft202012)
        .with_retriever(Bundled)
        .should_validate_formats(true)
        .build(schema)
        .map_err(|e| SchemaError::Compile(e.to_string()))
}

fn compile_all() -> Result<HashMap<&'static str, Validator>, SchemaError> {
    let mut validators = HashMap::new();
    for (_, _, id) in registry::SUPPORTED {
        let source =
            registry::bundled(id).ok_or_else(|| SchemaError::NotBundled(id.to_string()))?;
        let schema: Value =
            serde_json::from_str(source).map_err(|e| SchemaError::Compile(e.to_string()))?;
        validators.insert(*id, compile(&schema)?);
    }
    Ok(validators)
}

/// The compiled validator for a supported record schema `$id`. All record
/// schemas are compiled together on first use and cached for the process.
pub fn validator_for(schema_id: &str) -> Result<&'static Validator, SchemaError> {
    static VALIDATORS: OnceLock<Result<HashMap<&'static str, Validator>, SchemaError>> =
        OnceLock::new();
    match VALIDATORS.get_or_init(compile_all) {
        Ok(map) => map
            .get(schema_id)
            .ok_or_else(|| SchemaError::NotBundled(schema_id.to_string())),
        Err(e) => Err(SchemaError::Compile(e.to_string())),
    }
}
