#!/usr/bin/env bash
set -euo pipefail

install_if_missing() {
    local binary="$1"
    local crate="$2"

    if command -v "${binary}" >/dev/null 2>&1; then
        return
    fi

    cargo install --locked "${crate}"
}

mode="${1:-core}"

case "${mode}" in
    core)
        install_if_missing cargo-nextest cargo-nextest
        install_if_missing cargo-llvm-cov cargo-llvm-cov
        install_if_missing cargo-deny cargo-deny
        install_if_missing cargo-machete cargo-machete
        ;;
    fuzz)
        install_if_missing cargo-fuzz cargo-fuzz
        rustup toolchain install nightly --profile minimal
        ;;
    mutants)
        install_if_missing cargo-mutants cargo-mutants
        ;;
    *)
        echo "unknown Rust quality tooling group: ${mode}" >&2
        exit 2
        ;;
esac
