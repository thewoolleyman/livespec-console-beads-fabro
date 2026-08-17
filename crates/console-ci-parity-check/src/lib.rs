//! Bidirectional parity gate between `just check` and CI check jobs.
//!
//! The important property is set parity, not a one-way subset test. Work-item
//! `livespec-console-beads-fabro-3r6` records the matching blind spot in another
//! gate: it caught upstream movement but stayed green when our own committed copy
//! moved. This checker therefore accounts for both directions explicitly.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

pub const JUSTFILE_PATH: &str = "justfile";
pub const CI_WORKFLOW_PATH: &str = ".github/workflows/ci.yml";
pub const FIXTURE_PATH: &str = "tests/fixtures/ci-check-parity.json";

pub struct ParityInput {
    pub just_targets: BTreeSet<String>,
    pub just_recipes: BTreeSet<String>,
    pub ci_jobs: BTreeSet<String>,
    pub fixture_entries: Vec<FixtureEntry>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct FixtureEntry {
    pub name: String,
    pub present_in_just_check: bool,
    pub present_in_ci: bool,
    pub reason: String,
}

pub enum Finding {
    MissingReason {
        name: String,
    },
    DuplicateFixtureEntry {
        name: String,
    },
    FixtureEntryNotDivergent {
        name: String,
    },
    FixtureEntryDoesNotMatchReality {
        name: String,
        expected_just: bool,
        actual_just: bool,
        expected_ci: bool,
        actual_ci: bool,
    },
    CiJobUnaccounted {
        name: String,
    },
    JustTargetUnaccounted {
        name: String,
    },
    JustTargetRecipeMissing {
        name: String,
    },
}

impl Finding {
    #[must_use]
    pub fn render(&self) -> String {
        match self {
            Self::MissingReason { name } => {
                format!("{name}: parity fixture entry carries an empty `reason`")
            }
            Self::DuplicateFixtureEntry { name } => {
                format!("{name}: parity fixture declares this entry more than once")
            }
            Self::FixtureEntryNotDivergent { name } => format!(
                "{name}: parity fixture entry records no omission; delete it or set \
                 `present_in_just_check: false` / `present_in_ci: false` with a reason"
            ),
            Self::FixtureEntryDoesNotMatchReality {
                name,
                expected_just,
                actual_just,
                expected_ci,
                actual_ci,
            } => format!(
                "{name}: parity fixture no longer matches reality \
                 (fixture present_in_just_check={expected_just}, actual={actual_just}; \
                 fixture present_in_ci={expected_ci}, actual={actual_ci})"
            ),
            Self::CiJobUnaccounted { name } => format!(
                "{name}: CI job is not run by `just check` and is not recorded in {FIXTURE_PATH} \
                 with `present_in_just_check: false` and a non-empty reason"
            ),
            Self::JustTargetUnaccounted { name } => format!(
                "{name}: `just check` target has no CI job and is not recorded in {FIXTURE_PATH} \
                 with `present_in_ci: false` and a non-empty reason"
            ),
            Self::JustTargetRecipeMissing { name } => {
                format!("{name}: listed in `just check` but no `{name}:` recipe exists")
            }
        }
    }
}

/// Parse the target names in the root `justfile` `check:` recipe's
/// `targets=(...)` array.
///
/// # Errors
/// Returns a message when the `check` recipe or target array cannot be found.
pub fn parse_just_check_targets(justfile: &str) -> Result<BTreeSet<String>, String> {
    let mut in_check = false;
    let mut in_targets = false;
    let mut targets = BTreeSet::new();

    for line in justfile.lines() {
        let trimmed = line.trim();
        if !line.starts_with(char::is_whitespace) && trimmed.ends_with(':') {
            if in_targets {
                return Err("justfile: `check` targets array was not closed".to_owned());
            }
            in_check = trimmed == "check:";
            continue;
        }

        if !in_check {
            continue;
        }
        if trimmed == "targets=(" {
            in_targets = true;
            continue;
        }
        if in_targets && trimmed == ")" {
            return Ok(targets);
        }
        if in_targets && !trimmed.is_empty() && !trimmed.starts_with('#') {
            let target = trimmed.split('#').next().unwrap_or_default().trim();
            targets.insert(target.to_owned());
        }
    }

    if in_check {
        Err("justfile: `check` recipe has no `targets=(...)` array".to_owned())
    } else {
        Err("justfile: no `check:` recipe found".to_owned())
    }
}

#[must_use]
pub fn parse_just_recipes(justfile: &str) -> BTreeSet<String> {
    justfile
        .lines()
        .filter(|line| !line.starts_with(char::is_whitespace))
        .filter_map(|line| line.trim().strip_suffix(':'))
        .filter(|name| {
            !name.is_empty()
                && name
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        })
        .map(str::to_owned)
        .collect()
}

/// Parse effective CI check names from `.github/workflows/ci.yml`.
///
/// The matrix job names checks as `${{ matrix.target }}`, so this expands the
/// matrix target list and then includes the explicit names of non-matrix jobs.
///
/// # Errors
/// Returns a message when the workflow YAML is malformed or does not carry
/// `jobs`.
pub fn parse_ci_jobs(ci_yaml: &str) -> Result<BTreeSet<String>, String> {
    let root: serde_yaml::Value =
        serde_yaml::from_str(ci_yaml).map_err(|err| format!("{CI_WORKFLOW_PATH}: {err}"))?;
    let jobs = root
        .get("jobs")
        .and_then(serde_yaml::Value::as_mapping)
        .ok_or_else(|| format!("{CI_WORKFLOW_PATH}: missing `jobs` mapping"))?;
    let mut names = BTreeSet::new();

    for (job_id, job) in jobs {
        let job_id = job_id
            .as_str()
            .ok_or_else(|| format!("{CI_WORKFLOW_PATH}: job id is not a string"))?;
        let Some(job_map) = job.as_mapping() else {
            continue;
        };
        let name = job_map
            .get("name")
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or(job_id);
        if name == "${{ matrix.target }}" {
            for target in matrix_targets(job_map)? {
                names.insert(target);
            }
        } else {
            names.insert(name.to_owned());
        }
    }

    Ok(names)
}

fn matrix_targets(job_map: &serde_yaml::Mapping) -> Result<Vec<String>, String> {
    let Some(targets) = job_map
        .get("strategy")
        .and_then(serde_yaml::Value::as_mapping)
        .and_then(|strategy| strategy.get("matrix"))
        .and_then(serde_yaml::Value::as_mapping)
        .and_then(|matrix| matrix.get("target"))
        .and_then(serde_yaml::Value::as_sequence)
    else {
        return Err(format!(
            "{CI_WORKFLOW_PATH}: `${{ matrix.target }}` job has no `strategy.matrix.target` list"
        ));
    };

    targets
        .iter()
        .map(|target| {
            target.as_str().map(str::to_owned).ok_or_else(|| {
                format!("{CI_WORKFLOW_PATH}: a `strategy.matrix.target` entry is not a string")
            })
        })
        .collect()
}

/// Parse the justified-divergence fixture.
///
/// # Errors
/// Returns a message when the JSON is malformed or missing `divergences`.
pub fn parse_fixture(fixture_json: &str) -> Result<Vec<FixtureEntry>, String> {
    let root: serde_json::Value =
        serde_json::from_str(fixture_json).map_err(|err| format!("{FIXTURE_PATH}: {err}"))?;
    let divergences = root
        .get("divergences")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("{FIXTURE_PATH}: missing a `divergences` array"))?;
    divergences
        .iter()
        .map(|entry| {
            let name = entry
                .get("name")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| format!("{FIXTURE_PATH}: a divergence has no string `name`"))?
                .to_owned();
            let present_in_just_check = entry
                .get("present_in_just_check")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true);
            let present_in_ci = entry
                .get("present_in_ci")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true);
            let reason = entry
                .get("reason")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned();
            Ok(FixtureEntry {
                name,
                present_in_just_check,
                present_in_ci,
                reason,
            })
        })
        .collect()
}

#[must_use]
pub fn check_parity(input: &ParityInput) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut entries = BTreeMap::new();

    for entry in &input.fixture_entries {
        if entry.reason.trim().is_empty() {
            findings.push(Finding::MissingReason {
                name: entry.name.clone(),
            });
        }
        if entry.present_in_just_check && entry.present_in_ci {
            findings.push(Finding::FixtureEntryNotDivergent {
                name: entry.name.clone(),
            });
        }
        if entries.insert(entry.name.as_str(), entry).is_some() {
            findings.push(Finding::DuplicateFixtureEntry {
                name: entry.name.clone(),
            });
        }
    }

    for target in &input.just_targets {
        if !input.just_recipes.contains(target) {
            findings.push(Finding::JustTargetRecipeMissing {
                name: target.clone(),
            });
        }
    }

    for entry in entries.values() {
        let actual_just = input.just_targets.contains(&entry.name);
        let actual_ci = input.ci_jobs.contains(&entry.name);
        if entry.present_in_just_check != actual_just || entry.present_in_ci != actual_ci {
            findings.push(Finding::FixtureEntryDoesNotMatchReality {
                name: entry.name.clone(),
                expected_just: entry.present_in_just_check,
                actual_just,
                expected_ci: entry.present_in_ci,
                actual_ci,
            });
        }
    }

    for job in &input.ci_jobs {
        if !input.just_targets.contains(job)
            && !entries.get(job.as_str()).is_some_and(|entry| {
                !entry.present_in_just_check && !entry.reason.trim().is_empty()
            })
        {
            findings.push(Finding::CiJobUnaccounted { name: job.clone() });
        }
    }

    for target in &input.just_targets {
        if !input.ci_jobs.contains(target)
            && !entries
                .get(target.as_str())
                .is_some_and(|entry| !entry.present_in_ci && !entry.reason.trim().is_empty())
        {
            findings.push(Finding::JustTargetUnaccounted {
                name: target.clone(),
            });
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::{
        CI_WORKFLOW_PATH, FIXTURE_PATH, FixtureEntry, ParityInput, check_parity, parse_ci_jobs,
        parse_fixture, parse_just_check_targets, parse_just_recipes,
    };
    use std::collections::BTreeSet;

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    fn assert_err_contains<T>(result: Result<T, String>, needle: &str) {
        let message = result.err().unwrap_or_default();
        assert!(message.contains(needle));
    }

    #[test]
    fn parses_just_check_targets_from_array_only() {
        let justfile = "\
check:
    targets=(
        check-format
        check-test
    )

check-format:
    cargo fmt
";
        assert_eq!(
            parse_just_check_targets(justfile),
            Ok(set(&["check-format", "check-test"]))
        );
    }

    #[test]
    fn expands_ci_matrix_target_names() {
        let ci = "\
jobs:
  check:
    name: ${{ matrix.target }}
    strategy:
      matrix:
        target:
          - check-format
          - check-test
  ci-green:
    name: ci-green
";
        assert_eq!(
            parse_ci_jobs(ci),
            Ok(set(&["check-format", "check-test", "ci-green"]))
        );
    }

    #[test]
    fn parses_fixture_defaults_presence_to_true() {
        let parsed = parse_fixture(
            r#"{
  "divergences": [
    {
      "name": "ci-green",
      "present_in_just_check": false,
      "reason": "rollup"
    }
  ]
}"#,
        );
        assert_eq!(
            parsed,
            Ok(vec![FixtureEntry {
                name: "ci-green".to_owned(),
                present_in_just_check: false,
                present_in_ci: true,
                reason: "rollup".to_owned(),
            }])
        );
    }

    #[test]
    fn parser_errors_name_the_missing_shapes() {
        assert_err_contains(
            parse_just_check_targets("other:\n    true\n"),
            "no `check:`",
        );
        assert_err_contains(
            parse_just_check_targets("check:\n    true\n"),
            "no `targets=(",
        );
        assert_err_contains(
            parse_just_check_targets("check:\n    targets=(\nnext:\n"),
            "not closed",
        );
        assert_eq!(
            parse_just_recipes(":\nbad target:\nvalid-target:\n"),
            set(&["valid-target"])
        );
        assert_err_contains(parse_ci_jobs("name: CI\n"), "missing `jobs`");
        assert_err_contains(parse_ci_jobs("jobs: ["), CI_WORKFLOW_PATH);
        assert_err_contains(parse_ci_jobs("jobs:\n  ? [bad]\n  : {}\n"), "job id");
        assert_err_contains(
            parse_ci_jobs(
                "\
jobs:
  check:
    name: ${{ matrix.target }}
",
            ),
            "strategy.matrix.target",
        );
        assert_err_contains(
            parse_ci_jobs(
                "\
jobs:
  check:
    name: ${{ matrix.target }}
    strategy:
      matrix:
        target:
          - 7
",
            ),
            "not a string",
        );
        assert_err_contains(parse_fixture("{"), FIXTURE_PATH);
        assert_err_contains(parse_fixture(r#"{"files":[]}"#), "missing a `divergences`");
        assert_err_contains(
            parse_fixture(r#"{"divergences":[{"reason":"missing name"}]}"#),
            "string `name`",
        );
    }

    #[test]
    fn ci_parser_skips_non_mapping_jobs() {
        let ci = "\
jobs:
  check-format:
    name: check-format
  generated:
";
        assert_eq!(parse_ci_jobs(ci), Ok(set(&["check-format"])));
    }

    #[test]
    fn records_red_in_both_directions() {
        let input = ParityInput {
            just_targets: set(&["check-format", "check-test", "check-local-only"]),
            just_recipes: set(&["check-format", "check-test", "check-local-only"]),
            ci_jobs: set(&["check-format", "check-test", "check-ci-only"]),
            fixture_entries: Vec::new(),
        };
        let rendered: Vec<String> = check_parity(&input)
            .iter()
            .map(super::Finding::render)
            .collect();
        assert!(rendered.iter().any(|line| line.contains("check-ci-only")));
        assert!(
            rendered
                .iter()
                .any(|line| line.contains("check-local-only"))
        );
    }

    #[test]
    fn requires_non_empty_reasons_for_recorded_divergence() {
        let input = ParityInput {
            just_targets: set(&["check-format"]),
            just_recipes: set(&["check-format"]),
            ci_jobs: set(&["check-format", "check-ci-only"]),
            fixture_entries: vec![FixtureEntry {
                name: "check-ci-only".to_owned(),
                present_in_just_check: false,
                present_in_ci: true,
                reason: "   ".to_owned(),
            }],
        };
        let rendered: Vec<String> = check_parity(&input)
            .iter()
            .map(super::Finding::render)
            .collect();
        assert!(rendered.iter().any(|line| line.contains("empty `reason`")));
        assert!(rendered.iter().any(|line| line.contains("check-ci-only")));
    }

    #[test]
    fn rejects_targets_without_recipes() {
        let justfile = "\
check:
    targets=(
        check-missing
    )
";
        let targets = parse_just_check_targets(justfile);
        assert_eq!(targets, Ok(set(&["check-missing"])));
        let input = ParityInput {
            just_targets: targets.unwrap_or_default(),
            just_recipes: parse_just_recipes(justfile),
            ci_jobs: BTreeSet::new(),
            fixture_entries: Vec::new(),
        };
        let rendered: Vec<String> = check_parity(&input)
            .iter()
            .map(super::Finding::render)
            .collect();
        assert!(
            rendered
                .iter()
                .any(|line| line.contains("no `check-missing:` recipe exists"))
        );
    }

    #[test]
    fn records_fixture_integrity_failures() {
        let input = ParityInput {
            just_targets: set(&["check-format", "check-stale"]),
            just_recipes: set(&["check-format", "check-stale"]),
            ci_jobs: set(&["check-format"]),
            fixture_entries: vec![
                FixtureEntry {
                    name: "check-format".to_owned(),
                    present_in_just_check: true,
                    present_in_ci: true,
                    reason: "not divergent".to_owned(),
                },
                FixtureEntry {
                    name: "check-stale".to_owned(),
                    present_in_just_check: false,
                    present_in_ci: true,
                    reason: "stale".to_owned(),
                },
                FixtureEntry {
                    name: "duplicate".to_owned(),
                    present_in_just_check: false,
                    present_in_ci: true,
                    reason: "first".to_owned(),
                },
                FixtureEntry {
                    name: "duplicate".to_owned(),
                    present_in_just_check: false,
                    present_in_ci: true,
                    reason: "second".to_owned(),
                },
            ],
        };
        let rendered: Vec<String> = check_parity(&input)
            .iter()
            .map(super::Finding::render)
            .collect();
        assert!(
            rendered
                .iter()
                .any(|line| line.contains("records no omission"))
        );
        assert!(rendered.iter().any(|line| line.contains("more than once")));
        assert!(
            rendered
                .iter()
                .any(|line| line.contains("no longer matches reality"))
        );
    }

    #[test]
    fn accepts_reasoned_just_only_targets() {
        let input = ParityInput {
            just_targets: set(&["check-format", "check-local-only"]),
            just_recipes: set(&["check-format", "check-local-only"]),
            ci_jobs: set(&["check-format"]),
            fixture_entries: vec![FixtureEntry {
                name: "check-local-only".to_owned(),
                present_in_just_check: true,
                present_in_ci: false,
                reason: "local guard".to_owned(),
            }],
        };
        assert!(check_parity(&input).is_empty());
    }
}
