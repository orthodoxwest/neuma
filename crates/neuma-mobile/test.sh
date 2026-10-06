#!/bin/sh
# Builds the library, generates its Python, Swift and Kotlin bindings, and runs the same
# smoke test through each binding it can: Python always, Kotlin on the JVM when
# NEUMA_KOTLINC and NEUMA_JNA are set, and Swift on macOS.
set -eu
cd "$(dirname "$0")/../.."
out=target/neuma-mobile-bindings
cargo rustc -q -p neuma-mobile --lib --crate-type cdylib
case "$(uname)" in
  Darwin) lib=target/debug/libneuma_mobile.dylib ;;
  *) lib=target/debug/libneuma_mobile.so ;;
esac
rm -rf "$out"
for lang in python swift kotlin; do
  cargo run -q -p neuma-mobile --features bindgen --bin uniffi-bindgen -- \
    generate --library "$lib" --language "$lang" --no-format --out-dir "$out/$lang" >/dev/null
done
cp "$lib" "$out/python/"
PYTHONPATH="$out/python" python3 crates/neuma-mobile/smoke.py

# Optional: the Kotlin bindings on the JVM, given a Kotlin compiler and the JNA jar.
if [ -n "${NEUMA_KOTLINC:-}" ] && [ -n "${NEUMA_JNA:-}" ]; then
  "$NEUMA_KOTLINC" -nowarn -include-runtime -cp "$NEUMA_JNA" -d "$out/kotlin.jar" \
    "$out/kotlin/org/orthodoxwest/neuma/neuma.kt" crates/neuma-mobile/Smoke.kt
  java -Djna.library.path="$(dirname "$lib")" -cp "$out/kotlin.jar:$NEUMA_JNA" SmokeKt
fi

# On macOS: the Swift bindings, compiled with a smoke test into one executable.
if command -v swiftc >/dev/null 2>&1 && [ "$(uname)" = Darwin ]; then
  swiftc -module-name NeumaSmoke -I "$out/swift" \
    -Xcc -fmodule-map-file="$out/swift/NeumaFFI.modulemap" \
    -L target/debug -lneuma_mobile \
    "$out/swift/Neuma.swift" crates/neuma-mobile/swift/main.swift -o "$out/swift-smoke"
  DYLD_LIBRARY_PATH=target/debug "$out/swift-smoke"
fi
