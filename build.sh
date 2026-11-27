#!/bin/sh
# Builds the SDK's commands into target/bin as juku-emu builds its Rust code
# (its spec §3.1-3.2): std built from source, the release profile's
# immediate-abort panics, no panic locations and no Debug formatting. A
# command is a crate in crates/<name> with a src/main.rs: juku-<name>.
#
#   ./build.sh           release build
#   ./build.sh debug     unoptimized build with debug info
#   ./build.sh install   release build of each command, installed into
#                        cargo's bin folder
#
# On a Mac, a release or debug build can name the processor to build for,
# arm64 (Apple silicon) or x86_64 (Intel), by default this Mac's:
# ./build.sh x86_64, ./build.sh debug x86_64. The commands then go into
# target/bin-<processor>, so target/bin keeps this Mac's. Rust needs
# nothing more for the other processor, since std is built from source.
#
# Needs nightly Rust with rust-src. build.cmd does the same on Windows.
set -e
cd "$(dirname "$0")"
RUST_TARGET=$(rustc -vV | sed -n 's/^host: //p')

usage() {
    echo "usage: build.sh [debug|install] [arm64|x86_64]" >&2
    exit 2
}

MODE=
ARCH=
for arg in "$@"; do
    case "$arg" in
        debug|install) [ -z "$MODE" ] || usage; MODE=$arg ;;
        arm64|x86_64) [ -z "$ARCH" ] || usage; ARCH=$arg ;;
        *) usage ;;
    esac
done

case "$MODE" in
    debug)
        CARGO_PROFILE=
        CARGO_OUT=debug
        RUSTFLAGS="-Zlocation-detail=none"
    ;;
    *)
        CARGO_PROFILE=--release
        CARGO_OUT=release
        RUSTFLAGS="-Zlocation-detail=none -Zfmt-debug=none"
    ;;
esac
export RUSTFLAGS

# The processor: Rust's target for it, and a folder of its own for the
# commands. Installed commands are for this Mac's own.
BIN=target/bin
if [ -n "$ARCH" ]; then
    if [ "$(uname -s)" != Darwin ]; then
        echo "build.sh: a processor is chosen on macOS only" >&2
        exit 2
    fi
    if [ "$MODE" = install ]; then
        echo "build.sh: install builds for this Mac's processor only" >&2
        exit 2
    fi
    case "$ARCH" in
        arm64) RUST_TARGET=aarch64-apple-darwin ;;
        x86_64) RUST_TARGET=x86_64-apple-darwin ;;
    esac
    BIN=target/bin-$ARCH
fi

# Build output is kept out of Time Machine backups.
if [ ! -d target ]; then
    mkdir target
    if command -v tmutil >/dev/null; then
        tmutil addexclusion target
    fi
fi

if [ "$MODE" = install ]; then
    for crate in crates/*/; do
        if [ -f "$crate/src/main.rs" ]; then
            cargo install --path "$crate" --target "$RUST_TARGET" --target-dir target/cargo -Z build-std=std
        fi
    done
    exit 0
fi

cargo build --workspace --target "$RUST_TARGET" --target-dir target/cargo -Z build-std=std $CARGO_PROFILE
mkdir -p $BIN
for crate in crates/*/; do
    if [ -f "$crate/src/main.rs" ]; then
        name=juku-$(basename "$crate")
        cp "target/cargo/$RUST_TARGET/$CARGO_OUT/$name" "$BIN/$name"
        if [ "$MODE" != debug ]; then
            strip "$BIN/$name"
        fi
    fi
done
