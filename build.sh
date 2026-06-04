#!/usr/bin/env bash
#
# Build helper for easy-move-resize-win.
#
# Runs the test suite and clippy lints, then produces the optimized release
# binary. Options:
#   --debug      build the unoptimized debug profile instead of release
#   --clean      remove target/ before building
#   --no-check   skip `cargo test` and `cargo clippy` (build only)
#   --run        launch the built executable when the build succeeds
#   -h, --help   show this help
#
# Examples:
#   ./build.sh
#   ./build.sh --run
#   ./build.sh --clean --debug

set -euo pipefail

# Build relative to this script so it works from any current directory.
cd "$(dirname "${BASH_SOURCE[0]}")"

debug=0
clean=0
no_check=0
run=0

usage() {
    # Print the contiguous comment header (skip the shebang, stop at the first
    # non-comment line), stripping the leading "# ".
    awk 'NR<3 { next } /^#/ { sub(/^# ?/, ""); print; next } { exit }' "${BASH_SOURCE[0]}"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --debug)    debug=1 ;;
        --clean)    clean=1 ;;
        --no-check) no_check=1 ;;
        --run)      run=1 ;;
        -h|--help)  usage; exit 0 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
    shift
done

step() {
    # step "name" cmd args...
    local name="$1"; shift
    printf '\033[36m==> %s\033[0m\n' "$name"
    "$@"
}

if ! command -v cargo >/dev/null 2>&1; then
    echo 'cargo not found on PATH. Install the Rust toolchain from https://rustup.rs/.' >&2
    exit 1
fi

if [[ $clean -eq 1 ]]; then
    step 'cargo clean' cargo clean
fi

if [[ $no_check -eq 0 ]]; then
    step 'cargo test' cargo test
    # Treat lint warnings as errors so a clean tree stays clean.
    step 'cargo clippy' cargo clippy --all-targets -- -D warnings
fi

if [[ $debug -eq 1 ]]; then
    profile_name='debug'
    step 'cargo build (debug)' cargo build
else
    profile_name='release'
    step 'cargo build (release)' cargo build --release
fi

exe="target/${profile_name}/easy-move-resize.exe"
printf '\033[32mBuild succeeded: %s\033[0m\n' "$exe"

if [[ $run -eq 1 ]]; then
    step 'launching easy-move-resize (tray app)' cmd.exe /c start "" "$(cygpath -w "$exe" 2>/dev/null || echo "$exe")"
fi
