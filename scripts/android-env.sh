#!/bin/sh
# Sets up the environment for Android builds, then runs the given command, e.g.
#   scripts/android-env.sh npx tauri android build --apk --target aarch64
# Needs Android Studio's SDK with an NDK, and a Java 17 JDK. The bundled OpenSSL (for the encrypted database) is
# built with the NDK's clang and looks for tools by their old names, hence the explicit CC/AR/RANLIB.
. "$HOME/.cargo/env" 2>/dev/null || true
: "${ANDROID_HOME:=$HOME/Library/Android/sdk}"
: "${NDK_HOME:=$(ls -d "$ANDROID_HOME"/ndk/* | sort -V | tail -1)}"
: "${JAVA_HOME:=$(/usr/libexec/java_home -v 17 2>/dev/null)}"
TC="$NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin"
export ANDROID_HOME NDK_HOME JAVA_HOME
export CC_aarch64_linux_android="$TC/aarch64-linux-android26-clang" AR_aarch64_linux_android="$TC/llvm-ar" \
  RANLIB_aarch64_linux_android="$TC/llvm-ranlib" CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$TC/aarch64-linux-android26-clang"
exec "$@"
