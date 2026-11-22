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
# Needs nightly Rust with rust-src. build.cmd does the same on Windows.
set -e
cd "$(dirname "$0")"
RUST_TARGET=$(rustc -vV | sed -n 's/^host: //p')

case "$1" in
    debug)
        CARGO_PROFILE=
        CARGO_OUT=debug
        RUSTFLAGS="-Zlocation-detail=none"
    ;;
    ""|install)
        CARGO_PROFILE=--release
        CARGO_OUT=release
        RUSTFLAGS="-Zlocation-detail=none -Zfmt-debug=none"
    ;;
    *)
        echo "usage: build.sh [debug|install]" >&2
        exit 2
    ;;
esac
export RUSTFLAGS

# Build output is kept out of Time Machine backups.
if [ ! -d target ]; then
    mkdir target
    if command -v tmutil >/dev/null; then
        tmutil addexclusion target
    fi
fi

if [ "$1" = install ]; then
    for crate in crates/*/; do
        if [ -f "$crate/src/main.rs" ]; then
            cargo install --path "$crate" --target "$RUST_TARGET" --target-dir target/cargo -Z build-std=std
        fi
    done
    exit 0
fi

cargo build --workspace --target "$RUST_TARGET" --target-dir target/cargo -Z build-std=std $CARGO_PROFILE
mkdir -p target/bin
for crate in crates/*/; do
    if [ -f "$crate/src/main.rs" ]; then
        name=juku-$(basename "$crate")
        cp "target/cargo/$RUST_TARGET/$CARGO_OUT/$name" "target/bin/$name"
        if [ "$1" != debug ]; then
            strip "target/bin/$name"
        fi
    fi
done
