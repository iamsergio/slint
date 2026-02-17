#!/bin/bash

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

SLINT_NO_QT=1

if [ "$#" -lt 1 ]; then
	echo "Usage: $0 --debug|--release"
	exit 1
fi

if [ "$1" != "--debug" ] && [ "$1" != "--release" ]; then
	echo "First argument must be --debug or --release"
	exit 1
fi

if [ "$1" = "--release" ]; then
	CARGO_BUILD_FLAGS="--release"
	TARGET_DIR="target/release"
	SHOULD_STRIP=1
else
	CARGO_BUILD_FLAGS=""
	TARGET_DIR="target/debug"
	SHOULD_STRIP=0
fi

mkdir -p "${SCRIPT_DIR}/bin"

build_and_move() {
	local features="$1"
	local outname="$2"

	cargo build -p slint-viewer ${CARGO_BUILD_FLAGS} --no-default-features --features ${features}
	mv "${TARGET_DIR}/slint-viewer" "${SCRIPT_DIR}/bin/${outname}"

	if [ "${SHOULD_STRIP}" -eq 1 ]; then
		if command -v strip >/dev/null 2>&1; then
			strip "${SCRIPT_DIR}/bin/${outname}" || true
		else
			echo "strip not found; skipping stripping for ${outname}"
		fi
	fi
}

build_and_move backend-winit,renderer-impeller slint-viewer-impeller
build_and_move backend-winit,renderer-skia slint-viewer-skia
build_and_move backend-winit,renderer-femtovg slint-viewer-femtovg

ls -lh "${SCRIPT_DIR}/bin"
