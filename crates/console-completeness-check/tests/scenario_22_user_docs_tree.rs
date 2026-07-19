//! Scenario 22 -- User-facing documentation lives in the docs/ tree with the
//! README as a pointer (`SPECIFICATION/scenarios.md`).
//!
//! Structural acceptance for the User Documentation Contract
//! (`SPECIFICATION/contracts.md`): user-facing documentation lives under a
//! `docs/` tree, the top-level `README.md` is a pointer carrying no user-facing
//! documentation of its own, `docs/README.md` is an overview plus a table of
//! contents linking each sub-document by relative path, the tree carries the
//! four named sub-documents, and the settings doc the completeness check reads
//! is `docs/detailed-usage.md` rather than the README.
//!
//! This test lives in the completeness-check crate because case 4 binds the
//! contract's settings-doc anchor to the constant this crate actually reads --
//! the one surface that would silently drift if the contract and the gate
//! disagreed.
//!
//! Each case returns `io::Result` and propagates with `?` rather than
//! unwrapping: the workspace denies `expect_used`, `unwrap_used`, and `panic`
//! in every target, tests included.

use std::io;
use std::path::{Path, PathBuf};

use console_completeness_check::SETTINGS_DOC;

/// The repository root, resolved from this crate's manifest directory.
fn repo_root() -> io::Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
}

/// Read a repository-root-relative file.
fn read(relative: &str) -> io::Result<String> {
    std::fs::read_to_string(repo_root()?.join(relative))
}

/// The four sub-documents the contract names, as `docs/`-relative filenames.
const REQUIRED_SUB_DOCUMENTS: [&str; 4] = [
    "installing.md",
    "overview-quickstart.md",
    "cli-options.md",
    "detailed-usage.md",
];

/// Headings that would mean the top-level README still carries user-facing
/// documentation of its own rather than pointing at the tree.
const USER_FACING_README_SECTIONS: [&str; 5] = [
    "## Installing",
    "## Running the console",
    "### Keys",
    "### The screen",
    "### Dispatcher settings",
];

/// Case 1 -- the top-level README is a pointer, not the documentation.
#[test]
fn readme_is_a_pointer_carrying_no_user_facing_documentation() -> io::Result<()> {
    let readme = read("README.md")?;

    assert!(
        readme.contains("docs/README.md"),
        "the top-level README must link the docs/ tree's index document"
    );

    for section in USER_FACING_README_SECTIONS {
        assert!(
            !readme.contains(section),
            "the top-level README still carries the user-facing section `{section}`; \
             user documentation belongs under docs/ per the User Documentation Contract"
        );
    }
    Ok(())
}

/// Case 1 (second clause) -- contributor material MAY remain in the README.
#[test]
fn readme_retains_contributor_facing_material() -> io::Result<()> {
    let readme = read("README.md")?;

    for section in ["## Developer build", "## Development"] {
        assert!(
            readme.contains(section),
            "the top-level README should retain the contributor-facing section \
             `{section}`; the contract leaves contributor material unconstrained"
        );
    }
    Ok(())
}

/// Case 2 -- the docs index is an overview and a table of contents only, and
/// every entry links a sub-document by a relative path.
#[test]
fn docs_index_is_an_overview_and_table_of_contents() -> io::Result<()> {
    let index = read("docs/README.md")?;

    for sub_document in REQUIRED_SUB_DOCUMENTS {
        assert!(
            index.contains(&format!("]({sub_document})")),
            "docs/README.md must link `{sub_document}` by a relative path"
        );
    }

    // The index points at the sub-documents; it does not itself carry the
    // substantive documentation. The per-pane material is the tell.
    assert!(
        !index.contains("### Header pane"),
        "docs/README.md must be an overview plus a table of contents only; \
         the substantive documentation belongs in the linked sub-documents"
    );
    Ok(())
}

/// Case 3 -- the tree carries the four required sub-documents.
#[test]
fn docs_tree_carries_the_four_required_sub_documents() -> io::Result<()> {
    let docs = repo_root()?.join("docs");
    for sub_document in REQUIRED_SUB_DOCUMENTS {
        assert!(
            docs.join(sub_document).is_file(),
            "the docs/ tree must carry `{sub_document}`"
        );
    }
    Ok(())
}

/// Case 3 -- the installation sub-document covers the download-install path and
/// running the console against a repository other than its own.
#[test]
fn installing_covers_download_install_and_another_repository() -> io::Result<()> {
    let installing = read("docs/installing.md")?;

    assert!(
        installing.contains("gh release download"),
        "docs/installing.md must cover the download-install path"
    );
    assert!(
        installing.contains("LIVESPEC_CONSOLE_REPO_PATH"),
        "docs/installing.md must cover running the console against a repository \
         other than its own, which is what LIVESPEC_CONSOLE_REPO_PATH selects"
    );
    Ok(())
}

/// Case 3 -- the detailed-usage sub-document carries a section per TUI pane.
#[test]
fn detailed_usage_carries_a_section_per_tui_pane() -> io::Result<()> {
    let detailed_usage = read("docs/detailed-usage.md")?;

    // Every focusable pane and every view the TUI renders.
    for pane in [
        "Header pane",
        "Views pane",
        "Attention pane",
        "Spec pane",
        "Lanes pane",
        "Events pane",
        "Repos pane",
        "Settings pane",
        "Detail pane",
        "Status pane",
    ] {
        assert!(
            detailed_usage.contains(&format!("### {pane}")),
            "docs/detailed-usage.md must carry a section for the {pane}"
        );
    }
    Ok(())
}

/// Case 4 -- the settings doc the completeness check reads is the detailed-usage
/// sub-document, not the top-level README.
#[test]
fn settings_doc_is_the_detailed_usage_sub_document() {
    assert_eq!(
        SETTINGS_DOC, "docs/detailed-usage.md",
        "the settings doc the completeness check reads MUST be \
         docs/detailed-usage.md per the User Documentation Contract"
    );
    assert_ne!(
        SETTINGS_DOC, "README.md",
        "the settings doc MUST NOT be the top-level README"
    );
}

/// Case 4 -- and the doc it names actually carries the Dispatcher-settings
/// section the check scopes its search to.
#[test]
fn settings_doc_carries_the_dispatcher_settings_section() -> io::Result<()> {
    let settings_doc = read(SETTINGS_DOC)?;

    assert!(
        settings_doc.contains("Dispatcher settings"),
        "{SETTINGS_DOC} must carry the heading the completeness check scopes to"
    );
    Ok(())
}
