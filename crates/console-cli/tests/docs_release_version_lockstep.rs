//! Version-scoped claims in `docs/installing.md` must be re-read whenever a new
//! release ships.
//!
//! # Why this gate exists
//!
//! The B8 acceptance run documented a real limitation — a console started
//! outside a repository observed nothing — with measured evidence. The fix
//! landed the same day (`7110eca`), and the doc became a confident, false
//! statement within hours. `docs/installing.md` now carries the corrected
//! wording plus a caveat scoped to the PUBLISHED asset (`v0.2.0`), because the
//! published asset and `master` disagree until the next release.
//!
//! That is the rot this gate targets, and it is a class the repo's other doc
//! gates structurally cannot catch. `docs_status_hint_lockstep` and
//! `docs_release_asset_lockstep` both bind a doc claim to SOURCE — they fire
//! when the repo becomes internally inconsistent. Here nothing is
//! inconsistent: the doc accurately describes a released artifact while master
//! moves on. A claim scoped to a release acquires a second lifetime,
//! independent of the working tree, and no source-binding assertion sees it.
//!
//! # What it actually asserts
//!
//! Only that the version those claims were last REVIEWED against is still the
//! version release-please has released. When release-please bumps
//! `.release-please-manifest.json`, this test fails; you re-read the claims
//! enumerated below, correct any that the new release has invalidated, and
//! bump `DOCS_REVIEWED_AGAINST` deliberately.
//!
//! This is the same pinned-ground-truth idiom as `console-spec-check`'s
//! normative-clause counts, which the repo treats as intentional friction: it
//! forces a conscious update whenever the thing it pins moves.
//!
//! Deliberately NOT "every version mentioned must equal the current release".
//! Some of these claims are HISTORICAL and must not be rewritten on each
//! release — the acceptance notice records which asset was actually
//! downloaded and run, and mechanically bumping that would turn a true
//! statement about a real test run into a false one.

use std::path::{Path, PathBuf};

/// The doc carrying version-scoped claims.
const INSTALL_DOC: &str = "docs/installing.md";
/// release-please's record of the current released version.
const MANIFEST: &str = ".release-please-manifest.json";

/// The released version `INSTALL_DOC`'s version-scoped claims were last read
/// against.
///
/// # Bump procedure
///
/// When this test fails, a new version has been released. Re-read each claim
/// below IN THE DOC before bumping this constant:
///
/// 1. **The acceptance notice** ("the published `vX` asset was downloaded …").
///    HISTORICAL — records which asset the B8 acceptance run actually
///    exercised. Do NOT retarget it at the new release; a newer asset has not
///    been through that run. Re-scope it only if a fresh acceptance run
///    happens.
/// 2. **The "each launched from inside its own checkout, which `vX` required"
///    clause.** Also historical, and tied to (1).
/// 3. **The "Requires a build newer than `vX`" caveat** on the cross-repo
///    invocation. This one EXPIRES: once a release contains `7110eca` ("run
///    backing CLIs from selected repo"), the published asset no longer needs
///    the `cd` workaround and the caveat should be DELETED, not renumbered.
///    Check with `git log <new-tag> --oneline | grep 'run backing CLIs'`.
const DOCS_REVIEWED_AGAINST: &str = "0.2.0";

fn repo_root() -> std::io::Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
}

fn read(relative: &str) -> std::io::Result<String> {
    std::fs::read_to_string(repo_root()?.join(relative))
}

/// The released version recorded in the release-please manifest.
///
/// Parsed without a JSON dependency: the manifest is a flat object whose `"."`
/// key holds the root package's version.
fn released_version(manifest: &str) -> Option<String> {
    let after_key = manifest.split_once("\".\"")?.1;
    let after_colon = after_key.split_once(':')?.1;
    let opening = after_colon.find('"')?;
    let rest = &after_colon[opening + 1..];
    let closing = rest.find('"')?;
    Some(rest[..closing].to_string())
}

#[test]
fn version_scoped_install_claims_were_reviewed_against_the_current_release() -> std::io::Result<()>
{
    let manifest = read(MANIFEST)?;
    let released = released_version(&manifest).unwrap_or_default();
    assert!(
        !released.is_empty(),
        "could not read the released version from {MANIFEST}; the `\".\"` key this gate \
         reads must still be present"
    );

    assert_eq!(
        released, DOCS_REVIEWED_AGAINST,
        "\n\
         A new version ({released}) has been released, but {INSTALL_DOC}'s version-scoped \
         claims were last read against {DOCS_REVIEWED_AGAINST}.\n\n\
         Those claims do not update themselves, and nothing else in this repo will notice: \
         they describe a PUBLISHED ARTIFACT, so the working tree stays internally consistent \
         while they go stale.\n\n\
         Re-read them in the doc (the `DOCS_REVIEWED_AGAINST` doc comment enumerates each one \
         and says which are historical and which EXPIRE), correct what the new release \
         invalidated, then set DOCS_REVIEWED_AGAINST = \"{released}\".\n"
    );
    Ok(())
}

/// The doc must actually still carry version-scoped claims.
///
/// If every `vX.Y.Z` mention disappears, the pin above is silently guarding
/// nothing — the gate would keep passing while the thing it protects is gone.
#[test]
fn the_install_doc_still_carries_the_claims_this_gate_pins() -> std::io::Result<()> {
    let doc = read(INSTALL_DOC)?;
    let needle = format!("v{DOCS_REVIEWED_AGAINST}");
    let mentions = doc.matches(needle.as_str()).count();

    assert!(
        mentions > 0,
        "{INSTALL_DOC} no longer mentions `{needle}`, so the DOCS_REVIEWED_AGAINST pin is \
         guarding nothing.\nEither the version-scoped claims were removed — in which case \
         delete this gate — or they were retargeted without bumping the pin."
    );
    Ok(())
}

#[cfg(test)]
mod manifest_parsing {
    use super::released_version;

    #[test]
    fn reads_the_root_package_version() {
        assert_eq!(
            released_version("{\n  \".\": \"0.2.0\"\n}"),
            Some("0.2.0".to_string())
        );
    }

    #[test]
    fn tolerates_compact_and_multi_package_forms() {
        assert_eq!(
            released_version(r#"{".":"1.4.7"}"#),
            Some("1.4.7".to_string())
        );
        assert_eq!(
            released_version(r#"{".": "2.0.0", "crates/other": "0.1.0"}"#),
            Some("2.0.0".to_string())
        );
    }

    #[test]
    fn returns_none_when_the_root_key_is_absent() {
        assert_eq!(released_version(r#"{"crates/other": "0.1.0"}"#), None);
        assert_eq!(released_version("not json at all"), None);
    }
}
