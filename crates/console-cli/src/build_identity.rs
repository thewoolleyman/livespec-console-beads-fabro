//! The running binary's build identity, embedded at COMPILE time.
//!
//! `livespec-console-beads-fabro-mx9u.13`. [`BUILD_GIT_SHA`] and
//! [`BUILD_TIMESTAMP`] are `env!()`-embedded constants, stamped by
//! `build.rs` from the BUILD HOST's own `git`/`date` -- never read here at
//! runtime. [`embedded_build_identity`] takes no arguments at all, which is
//! the structural half of that guarantee: with no repo path, no probe, and
//! no working directory to consult, it has nothing to read even if it
//! wanted to.

/// The short commit sha the running binary was compiled from. `"unknown"`
/// when the build host had no usable `git` (a source tarball, most likely).
pub const BUILD_GIT_SHA: &str = env!("CONSOLE_BUILD_GIT_SHA");

/// The build's wall-clock timestamp (UTC, ISO-8601). `"unknown"` when the
/// build host had no usable `date`.
pub const BUILD_TIMESTAMP: &str = env!("CONSOLE_BUILD_TIMESTAMP");

#[must_use]
/// The running binary's build identity, read from the compile-time-embedded
/// constants above.
pub fn embedded_build_identity() -> console_application::build_identity::BuildIdentity {
    console_application::build_identity::BuildIdentity::new(BUILD_GIT_SHA, BUILD_TIMESTAMP)
}

#[cfg(test)]
mod tests {
    use super::{BUILD_GIT_SHA, BUILD_TIMESTAMP, embedded_build_identity};

    #[test]
    fn build_identity_constants_are_compile_time_not_a_runtime_read() {
        // `env!()` resolves at COMPILE time, so binding it to a `const` is
        // only valid when the value is truly compile-time-known. Had this
        // been written as a runtime `std::env::var("...")` call instead, the
        // crate would fail to COMPILE here (a non-const fn cannot initialize
        // a const) -- so this passing is a structural proof about WHERE the
        // value comes from, not a behavioural inference from what it
        // happens to equal.
        const _SHA_IS_A_COMPILE_TIME_CONSTANT: &str = BUILD_GIT_SHA;
        const _TIMESTAMP_IS_A_COMPILE_TIME_CONSTANT: &str = BUILD_TIMESTAMP;
        assert!(!BUILD_GIT_SHA.is_empty());
        assert!(!BUILD_TIMESTAMP.is_empty());
    }

    #[test]
    fn embedded_build_identity_takes_no_repo_or_probe_to_read() {
        // The whole point: a binary copied to another host, or a checkout
        // that moves on underneath a long-running session, must keep
        // reporting the commit it was actually built from. A function with
        // no path/probe parameter cannot consult either at call time --
        // this test pins the signature's arity as much as its content.
        let identity = embedded_build_identity();
        assert_eq!(identity.sha(), BUILD_GIT_SHA);
        assert_eq!(identity.built_at(), BUILD_TIMESTAMP);
    }

    #[test]
    fn this_dev_checkout_actually_resolved_a_real_git_sha() {
        // A stronger-than-structural sanity check for the common case (this
        // repo, built from a real git checkout): `build.rs` should have
        // resolved a genuine short sha, not fallen back to "unknown". `git
        // rev-parse --short=7 HEAD` returns AT LEAST 7 hex characters (more
        // only if a repo is large enough to need it for uniqueness).
        assert!(BUILD_GIT_SHA.len() >= 7);
        assert!(
            BUILD_GIT_SHA
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        );
    }
}
