use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("examples/fixtures")
}

fn run(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_oc-sidememory"))
        .args(arguments)
        .output()
        .expect("CLI starts")
}

fn common(schema: &str) -> Vec<String> {
    let fixtures = fixtures();
    vec![
        "--schema".into(),
        fixtures.join(schema).display().to_string(),
        "--vocabulary".into(),
        fixtures.join("byte-vocabulary.json").display().to_string(),
    ]
}

fn strings(values: &[String]) -> Vec<&str> {
    values.iter().map(String::as_str).collect()
}

#[test]
fn capabilities_compile_tokens_and_mask_emit_versioned_json() {
    let output = run(&["capabilities"]);
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON output");
    assert_eq!(value["formatVersion"], 1);
    assert_eq!(value["liveGuideSerialization"], false);

    let mut arguments = vec!["compile".into()];
    arguments.extend(common("unique.schema.json"));
    let output = run(&strings(&arguments));
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON output");
    assert_eq!(value["compiled"], true);
    assert_eq!(value["semanticConstraints"], 1);

    for command in ["tokens", "mask"] {
        let mut arguments = vec![command.into()];
        arguments.extend(common("unique.schema.json"));
        let output = run(&strings(&arguments));
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON output");
        assert_eq!(value["formatVersion"], 1);
    }
}

#[test]
fn check_reports_success_and_structured_semantic_rejection() {
    let fixtures = fixtures();
    let mut accepted = vec!["check".into()];
    accepted.extend(common("unique.schema.json"));
    accepted.extend([
        "--tokens".into(),
        fixtures
            .join("unique-valid.tokens.json")
            .display()
            .to_string(),
        "--finish".into(),
    ]);
    let output = run(&strings(&accepted));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON output");
    assert_eq!(value["accepted"], true);
    assert_eq!(value["terminated"], true);

    let mut rejected = vec!["check".into()];
    rejected.extend(common("unique.schema.json"));
    rejected.extend([
        "--tokens".into(),
        fixtures
            .join("unique-duplicate.tokens.json")
            .display()
            .to_string(),
    ]);
    let output = run(&strings(&rejected));
    assert_eq!(output.status.code(), Some(5));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON output");
    assert_eq!(value["category"], "duplicate_array_item");
    assert_eq!(value["itemIndex"], 1);
}

#[test]
fn check_enforces_contains_capture_and_membership() {
    let fixtures = fixtures();
    for (schema, extension, imports, valid, invalid, category) in [
        (
            "contains.schema.json",
            None,
            None,
            "contains-valid.tokens.json",
            "contains-invalid.tokens.json",
            "contains_minimum_unreachable",
        ),
        (
            "capture.schema.json",
            Some("capture.extensions.json"),
            None,
            "capture-valid.tokens.json",
            "capture-invalid.tokens.json",
            "equality_mismatch",
        ),
        (
            "membership.schema.json",
            Some("membership.extensions.json"),
            Some("membership.imports.json"),
            "membership-valid.tokens.json",
            "membership-invalid.tokens.json",
            "imported_member_required",
        ),
    ] {
        for (tokens, expected_code) in [(valid, 0), (invalid, 5)] {
            let mut arguments = vec!["check".into()];
            arguments.extend(common(schema));
            if let Some(extension) = extension {
                arguments.extend([
                    "--extensions".into(),
                    fixtures.join(extension).display().to_string(),
                ]);
            }
            if let Some(imports) = imports {
                arguments.extend([
                    "--imports".into(),
                    fixtures.join(imports).display().to_string(),
                    "--import-identity".into(),
                    "example-catalog".into(),
                    "--import-version".into(),
                    "1".into(),
                ]);
            }
            arguments.extend([
                "--tokens".into(),
                fixtures.join(tokens).display().to_string(),
                "--finish".into(),
            ]);
            let output = run(&strings(&arguments));
            assert_eq!(output.status.code().unwrap_or_default(), expected_code);
            if expected_code == 5 {
                let value: serde_json::Value =
                    serde_json::from_slice(&output.stdout).expect("JSON output");
                assert_eq!(value["category"], category);
            }
        }
    }
}

#[test]
fn malformed_inputs_and_structural_rejections_have_distinct_exit_codes() {
    let output = run(&["unknown"]);
    assert_eq!(output.status.code(), Some(2));

    let fixtures = fixtures();
    let mut malformed = vec!["compile".into()];
    malformed.extend(common("missing.schema.json"));
    let output = run(&strings(&malformed));
    assert_eq!(output.status.code(), Some(2));

    let mut structural = vec!["check".into()];
    structural.extend(common("unique.schema.json"));
    structural.extend([
        "--tokens".into(),
        fixtures
            .join("contains-valid.tokens.json")
            .display()
            .to_string(),
    ]);
    let output = run(&strings(&structural));
    assert_eq!(output.status.code(), Some(4));
}

#[test]
fn inspect_imports_reports_identity_version_and_count() {
    let fixtures = fixtures();
    let output = run(&[
        "inspect-imports",
        "--imports",
        &fixtures
            .join("membership.imports.json")
            .display()
            .to_string(),
        "--import-identity",
        "customers",
        "--import-version",
        "2026-09-10",
    ]);
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON output");
    assert_eq!(value["identity"], "customers");
    assert_eq!(value["version"], "2026-09-10");
    assert_eq!(value["setCount"], 1);
}
