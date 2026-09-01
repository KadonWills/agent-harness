#!/usr/bin/env bash
# Mirror of the CI gate — if this passes, CI passes.
# Usage: ./scripts/check.sh [--fix]
set -euo pipefail
cd "$(dirname "$0")/.."

FIX="${1:-}"

step() { printf '\n\033[1;34m==> %s\033[0m\n' "$*"; }

if [ "$FIX" = "--fix" ]; then
  step "cargo fmt (writing)"
  cargo fmt --all
  step "cargo clippy --fix"
  cargo clippy --workspace --all-targets --all-features --fix --allow-dirty --allow-staged -- -D warnings
else
  step "cargo fmt --check"
  cargo fmt --all --check
  step "cargo clippy (-D warnings)"
  cargo clippy --workspace --all-targets --all-features -- -D warnings
fi

step "cargo test"
cargo test --workspace --all-features

step "cargo doc (broken links are errors)"
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

if command -v cargo-deny >/dev/null 2>&1; then
  step "cargo deny (licenses/advisories)"
  cargo deny check advisories bans licenses sources
else
  printf '\n(skipping cargo-deny — install with: cargo install cargo-deny)\n'
fi

if command -v gitleaks >/dev/null 2>&1; then
  step "gitleaks (secret scan)"
  gitleaks detect --source . --no-banner
else
  printf '(skipping gitleaks — install from https://github.com/gitleaks/gitleaks)\n'
fi

printf '\n\033[1;32mAll local gate checks passed.\033[0m\n'
