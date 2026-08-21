#!/bin/sh
set -eu

ROOT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
WORKSPACE_DIR="$(dirname -- "$ROOT_DIR")"
LOCK_FILE="$ROOT_DIR/upstream.lock"
PATCH_SERIES="$ROOT_DIR/patches/series"
TARGET="${RUST_TARGET:-aarch64-linux-android}"
ANDROID_API_LEVEL="${ANDROID_API_LEVEL:-24}"
OUTPUT_DIR="${OUTPUT_DIR:-$WORKSPACE_DIR/output}"
MODULE_ARCHIVE_NAME="USB_SampleRate_Changer_WebUI"

# Homebrew's Rust package cannot install extra standard-library targets. When
# rustup is available, prefer its cargo/rustc proxies so the pinned Android
# target and the selected compiler always come from the same toolchain.
if command -v rustup >/dev/null 2>&1; then
    RUSTUP_BIN_DIR="$(dirname -- "$(command -v rustup)")"
    if [ -x "$RUSTUP_BIN_DIR/cargo" ] && [ -x "$RUSTUP_BIN_DIR/rustc" ]; then
        PATH="$RUSTUP_BIN_DIR:$PATH"
        export PATH
    fi
fi

if [ ! -r "$LOCK_FILE" ]; then
    printf 'Missing upstream lock: %s\n' "$LOCK_FILE" >&2
    exit 1
fi

# shellcheck disable=SC1090
. "$LOCK_FILE"

case "${UPSTREAM_REPOSITORY:-}" in
    https://* | git@*) ;;
    *)
        printf 'Invalid UPSTREAM_REPOSITORY in %s\n' "$LOCK_FILE" >&2
        exit 1
        ;;
esac

case "${UPSTREAM_COMMIT:-}" in
    *[!0-9a-fA-F]* | '')
        printf 'UPSTREAM_COMMIT must be a full hexadecimal commit ID\n' >&2
        exit 1
        ;;
esac

if [ "${#UPSTREAM_COMMIT}" -ne 40 ]; then
    printf 'UPSTREAM_COMMIT must contain exactly 40 hexadecimal characters\n' >&2
    exit 1
fi

for command_name in cargo git npm sed zip; do
    if ! command -v "$command_name" >/dev/null 2>&1; then
        printf 'Required command not found: %s\n' "$command_name" >&2
        exit 1
    fi
done

BUILD_DIR="$(mktemp -d "${TMPDIR:-/tmp}/usb-samplerate-webui.XXXXXX")"
UPSTREAM_DIR="$BUILD_DIR/upstream"
STAGING_DIR="$BUILD_DIR/module"
trap 'rm -rf "$BUILD_DIR"' EXIT HUP INT TERM

if [ -n "${UPSTREAM_SOURCE_DIR:-}" ]; then
    if ! git -C "$UPSTREAM_SOURCE_DIR" cat-file -e "$UPSTREAM_COMMIT^{commit}" 2>/dev/null; then
        printf 'Pinned upstream commit is unavailable in %s\n' "$UPSTREAM_SOURCE_DIR" >&2
        exit 1
    fi
    git clone -q --no-checkout "$UPSTREAM_SOURCE_DIR" "$UPSTREAM_DIR"
    git -C "$UPSTREAM_DIR" checkout -q --detach "$UPSTREAM_COMMIT"
else
    git init -q "$UPSTREAM_DIR"
    git -C "$UPSTREAM_DIR" fetch -q --depth=1 "$UPSTREAM_REPOSITORY" "$UPSTREAM_COMMIT"
    git -C "$UPSTREAM_DIR" checkout -q --detach FETCH_HEAD
fi

while IFS= read -r patch_name || [ -n "$patch_name" ]; do
    case "$patch_name" in
        '' | \#*) continue ;;
        /* | *..*)
            printf 'Unsafe patch path in %s: %s\n' "$PATCH_SERIES" "$patch_name" >&2
            exit 1
            ;;
    esac

    patch_path="$ROOT_DIR/patches/$patch_name"
    if [ ! -r "$patch_path" ]; then
        printf 'Patch listed in series does not exist: %s\n' "$patch_path" >&2
        exit 1
    fi

    git -C "$UPSTREAM_DIR" apply --check "$patch_path"
    git -C "$UPSTREAM_DIR" apply "$patch_path"
done < "$PATCH_SERIES"

mkdir -p "$STAGING_DIR"
for upstream_file in USB_SampleRate_Changer.sh functions3.shlib README.md LICENSE changelog.md; do
    if [ ! -e "$UPSTREAM_DIR/$upstream_file" ]; then
        printf 'Expected upstream file is missing: %s\n' "$upstream_file" >&2
        exit 1
    fi
    cp "$UPSTREAM_DIR/$upstream_file" "$STAGING_DIR/$upstream_file"
done

for upstream_directory in templates extras; do
    if [ ! -d "$UPSTREAM_DIR/$upstream_directory" ]; then
        printf 'Expected upstream directory is missing: %s\n' "$upstream_directory" >&2
        exit 1
    fi
    cp -R "$UPSTREAM_DIR/$upstream_directory" "$STAGING_DIR/$upstream_directory"
done

cp "$ROOT_DIR/module/module.prop" "$ROOT_DIR/module/customize.sh" \
    "$ROOT_DIR/module/uninstall.sh" "$ROOT_DIR/module/skip_mount" "$STAGING_DIR/"
cp "$ROOT_DIR/README.md" "$STAGING_DIR/WEBUI.md"

(cd "$ROOT_DIR/webui" && npm ci && WEBUI_OUT_DIR="$STAGING_DIR/webroot" npm run build)

if [ "$TARGET" = "aarch64-linux-android" ] && \
        [ -z "${CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER:-}" ]; then
    NDK_ROOT="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}"
    if [ -z "$NDK_ROOT" ] && command -v sdkmanager >/dev/null 2>&1; then
        SDK_ROOT="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-/opt/homebrew/share/android-commandlinetools}}"
        NDK_ROOT="$(find "$SDK_ROOT/ndk" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | sort -V | tail -n 1)"
    fi

    HOST_TAG=darwin-x86_64
    if [ "$(uname -s)" = Linux ]; then
        HOST_TAG=linux-x86_64
    fi

    LINKER="$NDK_ROOT/toolchains/llvm/prebuilt/$HOST_TAG/bin/aarch64-linux-android${ANDROID_API_LEVEL}-clang"
    if [ ! -x "$LINKER" ]; then
        printf 'Android NDK linker not found; set ANDROID_NDK_HOME or CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER\n' >&2
        exit 1
    fi
    export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$LINKER"
fi

CARGO_TARGET_DIR="$BUILD_DIR/cargo-target" \
    cargo build --locked --manifest-path "$ROOT_DIR/backend/Cargo.toml" --release --target "$TARGET"
cp "$BUILD_DIR/cargo-target/$TARGET/release/usbsrctl" "$STAGING_DIR/usbsrctl"

chmod 0755 "$STAGING_DIR/customize.sh" "$STAGING_DIR/uninstall.sh" \
    "$STAGING_DIR/USB_SampleRate_Changer.sh" "$STAGING_DIR/usbsrctl"

MODULE_VERSION="$(sed -n 's/^version=//p' "$STAGING_DIR/module.prop")"
case "$MODULE_VERSION" in
    '' | */*)
        printf 'Invalid module version in module.prop: %s\n' "$MODULE_VERSION" >&2
        exit 1
        ;;
esac

mkdir -p "$OUTPUT_DIR"
ARCHIVE_PATH="$OUTPUT_DIR/$MODULE_ARCHIVE_NAME-$MODULE_VERSION.zip"
TEMP_ARCHIVE="$BUILD_DIR/$MODULE_ARCHIVE_NAME-$MODULE_VERSION.zip"
(cd "$STAGING_DIR" && zip -q -r "$TEMP_ARCHIVE" . -x '*/.DS_Store')
mv -f "$TEMP_ARCHIVE" "$ARCHIVE_PATH"

printf 'Built %s\n' "$ARCHIVE_PATH"
printf 'Upstream commit: %s\n' "$UPSTREAM_COMMIT"
