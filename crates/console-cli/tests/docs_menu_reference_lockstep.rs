//! The committed operator key/action reference must be generated from the
//! action registry.
//!
//! This gate is equality-based on purpose: the reference is not prose that
//! quotes a subset of the action surface. It is the generated menu/action
//! catalog, so any registry taxonomy, label, accelerator, availability, or
//! staging change must update the committed markdown in the same change.

use std::path::{Path, PathBuf};

/// The generated operator reference.
const GENERATED_DOC: &str = "docs/reference/key-action-reference.md";

fn repo_root() -> std::io::Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
}

fn read(relative: &str) -> std::io::Result<String> {
    std::fs::read_to_string(repo_root()?.join(relative))
}

fn first_diff_line(expected: &str, actual: &str) -> Option<usize> {
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    let len = expected_lines.len().max(actual_lines.len());
    (0..len).find(|index| expected_lines.get(*index) != actual_lines.get(*index))
}

#[test]
fn generated_key_action_reference_matches_the_registry() -> std::io::Result<()> {
    let expected = console_application::action_registry::operator_key_action_reference_markdown();
    let actual = read(GENERATED_DOC)?;
    let diff = first_diff_line(&expected, &actual);

    assert!(
        diff.is_none(),
        "{GENERATED_DOC} is out of lockstep with ACTION_REGISTRY. Regenerate with \
         `just generate-key-action-reference`.\nFirst differing line: {}\nexpected: {:?}\nactual:   {:?}",
        diff.map_or(0, |index| index + 1),
        diff.and_then(|index| expected.lines().nth(index)),
        diff.and_then(|index| actual.lines().nth(index)),
    );
    Ok(())
}

#[cfg(test)]
mod extraction {
    use super::first_diff_line;

    #[test]
    fn names_the_first_differing_line() {
        assert_eq!(first_diff_line("a\nb\nc\n", "a\nx\nc\n"), Some(1));
    }

    #[test]
    fn names_an_added_or_removed_line() {
        assert_eq!(first_diff_line("a\nb\n", "a\n"), Some(1));
    }
}
