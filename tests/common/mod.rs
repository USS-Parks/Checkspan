//! Helpers shared by the contract test suites.
#![allow(dead_code)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use checkspan::contracts::registry;
use jsonschema::{Draft, Retrieve, Uri, Validator};
use serde_json::Value;

/// Serves `$ref` targets from the bundled registry only.
pub struct Bundled;

impl Retrieve for Bundled {
    fn retrieve(&self, uri: &Uri<String>) -> Result<Value, Box<dyn Error + Send + Sync>> {
        registry::bundled(uri.as_str())
            .map(|source| serde_json::from_str(source).expect("bundled schema is JSON"))
            .ok_or_else(|| format!("schema {uri} is not bundled").into())
    }
}

/// Compile a bundled schema with offline resolution and format assertions.
pub fn validator_for(schema_id: &str) -> Validator {
    let root: Value = serde_json::from_str(registry::bundled(schema_id).unwrap()).unwrap();
    jsonschema::options()
        .with_draft(Draft::Draft202012)
        .with_retriever(Bundled)
        .should_validate_formats(true)
        .build(&root)
        .expect("bundled schema compiles offline")
}

/// Path of a file under `tests/fixtures`.
pub fn fixture_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(rel)
}

/// Contents of a file under `tests/fixtures`.
pub fn fixture(rel: &str) -> String {
    let path = fixture_path(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Sorted file names in a directory under `tests/fixtures`.
pub fn fixture_names(dir: &str) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(fixture_path(dir))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Every schema violation of `text` against the bundled schema, with paths.
pub fn schema_errors(schema_id: &str, text: &str) -> Vec<String> {
    let instance: Value = serde_json::from_str(text).unwrap();
    validator_for(schema_id)
        .iter_errors(&instance)
        .map(|e| format!("{e} at {}", e.instance_path()))
        .collect()
}
