#!/bin/sh
# Runs inside the release container: /repo is the checkout (read-only), /out receives artefacts.
set -eu

# Work on a copy, so nothing the host built — target/, node_modules/ — is touched or reused.
mkdir -p /src
tar -C /repo --exclude=./target --exclude=./dist --exclude='./crates/desktop/ui/node_modules' \
    --exclude=./.direnv -cf - . | tar -C /src -xf -
cd /src
rustup target add x86_64-pc-windows-msvc >/dev/null

(cd crates/desktop/ui && pnpm install --frozen-lockfile)

if [ -z "${ONLY_WINDOWS:-}" ]; then
echo "== Linux"
cargo build --release -p newsbuilder-cli
(cd crates/desktop && APPIMAGE_EXTRACT_AND_RUN=1 cargo tauri build --bundles deb,appimage)
mkdir -p /out/linux
cp target/release/newsbuilder target/release/newsbuilder-desktop /out/linux/
cp target/release/bundle/deb/*.deb target/release/bundle/appimage/*.AppImage /out/linux/
fi

echo "== Windows"
# libwebp's SSE4.1 sources rely on MSVC letting intrinsics through; clang-cl wants the feature
# enabled. Every CPU Windows 10/11 runs on has it. Target builds only, not build scripts.
export TARGET_CFLAGS="-msse4.1"
cargo xwin build --release -p newsbuilder-cli --target x86_64-pc-windows-msvc
(cd crates/desktop && cargo tauri build --runner cargo-xwin --target x86_64-pc-windows-msvc --bundles nsis)
mkdir -p /out/windows
cp target/x86_64-pc-windows-msvc/release/newsbuilder.exe \
   target/x86_64-pc-windows-msvc/release/newsbuilder-desktop.exe /out/windows/
cp target/x86_64-pc-windows-msvc/release/bundle/nsis/*.exe /out/windows/

echo "== done"
ls -la /out/linux /out/windows
