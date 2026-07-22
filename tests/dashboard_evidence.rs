use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracky::storage::apply_migrations;

const REQUIRED_COVERAGE: &[&str] = &[
    "finance",
    "investment",
    "filtering",
    "pagination",
    "empty",
    "stale",
    "unavailable",
    "incompatible",
    "overflow",
    "error",
    "cli-parity",
    "reconciliation",
    "refresh",
    "refresh-failure",
    "cursor-invalidation",
];

#[derive(Debug, Deserialize)]
struct CorpusManifest {
    corpus_version: u64,
    provenance: Provenance,
    scenarios: Vec<Scenario>,
}

#[derive(Debug, Deserialize)]
struct Provenance {
    source: String,
    expected_results: String,
    generated_by: String,
}

#[derive(Debug, Deserialize)]
struct Scenario {
    id: String,
    coverage: Vec<String>,
    seed: CorpusFile,
    oracle: CorpusFile,
    #[serde(default)]
    mutations: Vec<CorpusFile>,
    seed_counts: BTreeMap<String, u64>,
}

#[derive(Debug, Deserialize)]
struct CorpusFile {
    path: String,
    sha256: String,
}

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dashboard")
}

fn read(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn assert_exact_numeric_transport(value: &Value, path: &str) {
    match value {
        Value::Object(fields) => {
            for (name, child) in fields {
                let child_path = format!("{path}.{name}");
                if name.ends_with("_minor")
                    || matches!(name.as_str(), "quantity" | "observed_price")
                {
                    let exact = child.is_string()
                        || child.is_null()
                        || child
                            .as_array()
                            .is_some_and(|values| values.iter().all(Value::is_string));
                    assert!(
                        exact,
                        "{child_path} must use an exact base-10 string or null"
                    );
                }
                assert_exact_numeric_transport(child, &child_path);
            }
        }
        Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                assert_exact_numeric_transport(child, &format!("{path}[{index}]"));
            }
        }
        _ => {}
    }
}

fn run_cli(database: &Path, args: &[&str]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_tracky"))
        .args(args)
        .arg("--db")
        .arg(database)
        .arg("--json")
        .output()
        .expect("run public Tracky CLI");
    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("CLI emits JSON")
}

fn assert_canonical_cli_oracle(scenario: &str, database: &Path, oracle: &Value) {
    let expected = &oracle["canonical_cli"];
    match scenario {
        "finance-months" => {
            let actual = run_cli(
                database,
                &[
                    "reports",
                    "summary",
                    "--start-date",
                    "2026-01-01",
                    "--end-date",
                    "2026-03-31",
                ],
            );
            assert_eq!(actual, expected["expected_response"]);
        }
        "investment-fresh" => {
            let actual = run_cli(
                database,
                &[
                    "reports",
                    "investments",
                    "--from",
                    "2026-01-01",
                    "--to",
                    "2026-02-28",
                ],
            );
            assert_eq!(actual, expected["expected_response"]);
        }
        "reconciliation-difference" => {
            let actual = run_cli(
                database,
                &[
                    "snapshots",
                    "compare",
                    "--snapshot-id",
                    "snapshot-difference",
                    "--as-of",
                    "2026-02-28",
                ],
            );
            assert_eq!(actual, expected["expected_response"]);
        }
        _ => panic!("missing CLI oracle assertion for {scenario}"),
    }
}

#[test]
fn dashboard_conformance_corpus_is_reproducible_and_loadable() {
    let root = corpus_root();
    let manifest: CorpusManifest =
        serde_json::from_slice(&read(&root.join("manifest.json"))).expect("valid corpus manifest");

    assert_eq!(manifest.corpus_version, 1);
    assert_eq!(
        manifest.provenance.source,
        "synthetic hand-authored examples"
    );
    assert_eq!(
        manifest.provenance.expected_results,
        "manual literals independent of Tracky production calculations"
    );
    assert_eq!(manifest.provenance.generated_by, "none");

    let mut ids = BTreeSet::new();
    let mut covered = BTreeSet::new();
    for scenario in &manifest.scenarios {
        assert!(
            ids.insert(&scenario.id),
            "duplicate scenario {}",
            scenario.id
        );
        covered.extend(scenario.coverage.iter().map(String::as_str));

        let seed_path = root.join(&scenario.seed.path);
        let oracle_path = root.join(&scenario.oracle.path);
        let seed = read(&seed_path);
        let oracle_bytes = read(&oracle_path);
        assert_eq!(
            sha256(&seed),
            scenario.seed.sha256,
            "{} seed hash",
            scenario.id
        );
        assert_eq!(
            sha256(&oracle_bytes),
            scenario.oracle.sha256,
            "{} oracle hash",
            scenario.id
        );

        let oracle: Value = serde_json::from_slice(&oracle_bytes).expect("valid oracle JSON");
        assert_eq!(oracle["scenario"], scenario.id);
        assert_eq!(oracle["provenance"], "hand-authored");
        assert!(
            oracle.get("transport").is_some(),
            "{} transport oracle",
            scenario.id
        );
        assert!(
            oracle.get("expected").is_some(),
            "{} result oracle",
            scenario.id
        );
        assert_exact_numeric_transport(&oracle["expected"], &scenario.id);
        if scenario.coverage.iter().any(|item| item == "cli-parity") {
            assert!(
                oracle.get("canonical_cli").is_some(),
                "{} canonical CLI oracle",
                scenario.id
            );
        }
        if scenario
            .coverage
            .iter()
            .any(|item| item == "reconciliation")
        {
            assert_eq!(
                oracle["expected"]["alerts"][0]["kind"],
                "reconciliation_difference"
            );
        }
        if scenario
            .coverage
            .iter()
            .any(|item| item == "refresh-failure")
        {
            assert_eq!(
                oracle["expected"]["refresh_failure"]["retained_snapshot_id"],
                oracle["expected"]["refresh_success"]["snapshot_id"]
            );
        }

        let database = tempfile::NamedTempFile::new().expect("temporary database");
        let connection = Connection::open(database.path()).expect("open temporary SQLite database");
        apply_migrations(&connection).expect("apply public Tracky migrations");
        connection
            .execute_batch(std::str::from_utf8(&seed).expect("UTF-8 SQL seed"))
            .unwrap_or_else(|error| panic!("failed to apply {}: {error}", seed_path.display()));

        for (table, expected_count) in &scenario.seed_counts {
            let actual: u64 = connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap_or_else(|error| {
                    panic!("failed to count {table} for {}: {error}", scenario.id)
                });
            assert_eq!(actual, *expected_count, "{} {table} count", scenario.id);
        }
        for mutation in &scenario.mutations {
            let mutation_path = root.join(&mutation.path);
            let mutation_sql = read(&mutation_path);
            assert_eq!(
                sha256(&mutation_sql),
                mutation.sha256,
                "{} mutation hash",
                scenario.id
            );
            connection
                .execute_batch(std::str::from_utf8(&mutation_sql).expect("UTF-8 SQL mutation"))
                .unwrap_or_else(|error| {
                    panic!(
                        "failed to apply {} for {}: {error}",
                        mutation_path.display(),
                        scenario.id
                    )
                });
        }
        if scenario.id == "snapshot-flows" {
            let count: u64 = connection
                .query_row("SELECT COUNT(*) FROM canonical_transactions", [], |row| {
                    row.get(0)
                })
                .expect("count refreshed transactions");
            assert_eq!(count, 7, "refresh mutation is deterministic");
        }
        drop(connection);
        if scenario.coverage.iter().any(|item| item == "cli-parity") {
            assert_canonical_cli_oracle(&scenario.id, database.path(), &oracle);
        }
    }

    for required in REQUIRED_COVERAGE {
        assert!(
            covered.contains(required),
            "missing {required} scenario coverage"
        );
    }
}
