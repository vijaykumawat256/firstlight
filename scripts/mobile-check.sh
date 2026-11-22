#!/bin/sh
# Type-checks the app for the phones (iPhone and Android) without touching the Mac build folder.
# Needs: rustup targets aarch64-apple-ios-sim and aarch64-linux-android, and the Android NDK (see android-env.sh).
set -e
for t in aarch64-apple-ios-sim aarch64-linux-android; do
  echo "== $t"
  CARGO_TARGET_DIR=target/mobile "$(dirname "$0")/android-env.sh" cargo clippy -p firstlight --target "$t" -- -D warnings
done
