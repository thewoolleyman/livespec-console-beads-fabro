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

ensure_cargo_binstall() {
    if command -v cargo-binstall >/dev/null 2>&1; then
        return
    fi

    if ! command -v curl >/dev/null 2>&1; then
        echo "curl is required to install cargo-binstall from a prebuilt release" >&2
        exit 1
    fi

    curl -L --proto '=https' --tlsv1.2 -sSf \
        https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh |
        bash
    hash -r
}

install_prebuilt_if_missing() {
    local binary="$1"
    local crate="$2"

    if command -v "${binary}" >/dev/null 2>&1; then
        return
    fi

    ensure_cargo_binstall
    cargo binstall --no-confirm --force --disable-strategies compile "${crate}"
    hash -r

    if ! command -v "${binary}" >/dev/null 2>&1; then
        echo "cargo-binstall completed but ${binary} is still not on PATH" >&2
        exit 1
    fi
}

mode="${1:-core}"

case "${mode}" in
    core)
        install_prebuilt_if_missing cargo-nextest cargo-nextest
        install_prebuilt_if_missing cargo-llvm-cov cargo-llvm-cov
        install_prebuilt_if_missing cargo-deny cargo-deny
        install_prebuilt_if_missing cargo-machete cargo-machete
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
