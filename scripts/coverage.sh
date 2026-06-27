#!/usr/bin/env bash
#
# Generate a code-coverage report for media-rs using cargo-llvm-cov.
#
# Usage:
#   scripts/coverage.sh           # build an HTML report and open it in a browser
#   scripts/coverage.sh --lcov    # emit target/coverage/lcov.info instead (for tooling/CI)
#
# Coverage runs the full test suite (unit + integration). The integration tests
# exercise assets/video*.mp4 when present; they skip cleanly when absent. The
# FFmpeg static binaries are downloaded automatically by build.rs.
set -euo pipefail

cd "$(dirname "$0")/.."

# Ensure the LLVM coverage tooling is available.
if ! cargo llvm-cov --version >/dev/null 2>&1; then
    echo "cargo-llvm-cov not found; installing it (one-time setup)..." >&2
    rustup component add llvm-tools-preview
    cargo install cargo-llvm-cov
fi

if [[ "${1:-}" == "--lcov" ]]; then
    mkdir -p target/coverage
    cargo llvm-cov --all-features --lcov --output-path target/coverage/lcov.info
    echo "Wrote target/coverage/lcov.info"
else
    cargo llvm-cov --all-features --html --open
    echo "HTML report written to target/llvm-cov/html/index.html"
fi
