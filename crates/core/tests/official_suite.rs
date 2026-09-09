use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use oc_sidememory::json_schema::compile::{compile_ir, CompileOptions};
use oc_sidememory::json_schema::diagnostic::CompileError;
use serde_json::Value;

#[test]
#[ignore = "requires OC_JSON_SCHEMA_TEST_SUITE"]
fn classify_official_draft_2020_12_schemas() {
    let root = PathBuf::from(
        std::env::var_os("OC_JSON_SCHEMA_TEST_SUITE")
            .expect("OC_JSON_SCHEMA_TEST_SUITE must name the pinned suite checkout"),
    );
    let draft = root.join("tests").join("draft2020-12");
    let mut files = Vec::new();
    collect_json_files(&draft, &mut files);
    files.sort();
    assert!(
        !files.is_empty(),
        "no official suite files found at {}",
        draft.display()
    );

    let mut groups = 0_usize;
    let mut cases = 0_usize;
    let mut classifications = BTreeMap::<&'static str, usize>::new();
    for path in &files {
        let bytes = fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let document: Value = serde_json::from_slice(&bytes)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let suites = document
            .as_array()
            .unwrap_or_else(|| panic!("{}: expected suite array", path.display()));
        for suite in suites {
            groups += 1;
            let schema = suite
                .get("schema")
                .unwrap_or_else(|| panic!("{}: missing schema", path.display()));
            cases += suite
                .get("tests")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            let schema_bytes = serde_json::to_vec(schema).expect("serialize lossless schema value");
            let category = match compile_ir(&schema_bytes, &CompileOptions::default()) {
                Ok(_) => "supported",
                Err(CompileError::UnsupportedKeyword { .. })
                | Err(CompileError::UnsupportedDialect { .. }) => "unsupported",
                Err(CompileError::UnsatisfiableSchema { .. }) => "unsatisfiable",
                Err(CompileError::InvalidKeywordValue { .. })
                | Err(CompileError::MalformedSchema { .. }) => "invalid-profile-input",
                Err(CompileError::ResourceLimit(_)) => "resource-limit",
                Err(CompileError::StructuralLowering { .. }) => "unexpected-structural",
                Err(CompileError::InvalidVocabulary(_)) => "unexpected-vocabulary",
                Err(CompileError::InternalInvariant { .. }) => "unexpected-internal",
            };
            *classifications.entry(category).or_default() += 1;
        }
    }

    println!(
        "official draft2020-12 files: {files_count}",
        files_count = files.len()
    );
    println!("official draft2020-12 schema groups: {groups}");
    println!("official draft2020-12 instance cases: {cases}");
    for (category, count) in &classifications {
        println!("{category}: {count}");
    }
    for unexpected in [
        "unexpected-structural",
        "unexpected-vocabulary",
        "unexpected-internal",
    ] {
        assert_eq!(classifications.get(unexpected), None, "{unexpected}");
    }
}

fn collect_json_files(directory: &Path, output: &mut Vec<PathBuf>) {
    let entries =
        fs::read_dir(directory).unwrap_or_else(|error| panic!("{}: {error}", directory.display()));
    for entry in entries {
        let entry = entry.expect("read suite directory entry");
        let path = entry.path();
        if path.is_dir() {
            collect_json_files(&path, output);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            output.push(path);
        }
    }
}
