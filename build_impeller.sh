#!/bin/bash

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

PLATFORM="${1}"
BUILD_TYPE=debug_unopt
INSTALL_DIR=${SCRIPT_DIR}/impeller_${BUILD_TYPE}_${PLATFORM}/

if [[ "${PLATFORM}" == "linux-x64" ]]; then
    GN_EXTRA_ARGS=""
    TARGET_BIN_FOLDER=host_${BUILD_TYPE}
elif [[ "${PLATFORM}" == "armv7" ]]; then
    if [[ -z "${ARMV7_SYSROOT}" ]]; then
        echo "Error: ARMV7_SYSROOT is not set"
        exit 1
    fi

    if [[ ! -d "${ARMV7_SYSROOT}" ]]; then
        echo "Error: ARMV7_SYSROOT directory does not exist: ${ARMV7_SYSROOT}"
        exit 1
    fi

    if [[ -z "${TOOLCHAIN_PREFIX}" ]]; then
        echo "Error: TOOLCHAIN_PREFIX is not set. Should be something like: /opt/sdk/sysroots/x86_64-pokysdk-linux/usr/bin/arm-foo-linux-gnueabi/arm-foo-linux-gnueabi-"
        exit 1
    fi

    GN_EXTRA_ARGS="--target-os linux --linux-cpu arm --target-sysroot ${ARMV7_SYSROOT} --arm-float-abi=hard --enable-minimal-linux --gn-args skia_use_vulkan=false --gn-args shell_enable_vulkan=false --gn-args skia_gl_standard=\"gles\" \
     --gn-args is_clang=false \
     --gn-args toolchain_prefix=\"${TOOLCHAIN_PREFIX}\""
    TARGET_BIN_FOLDER=linux_${BUILD_TYPE}_arm
else
    echo "Error: unsupported platform '${PLATFORM}'. Valid values: linux-x64, armv7"
    exit 1
fi

BUILD_DIR="${SCRIPT_DIR}/3rdparty/flutter/engine/src/out/${TARGET_BIN_FOLDER}"

export PATH=$PATH:/pub_data/sources/google/depot_tools/

cd ${SCRIPT_DIR}/3rdparty/flutter/
cp ./engine/scripts/standard.gclient .gclient
#rm -rf ${BUILD_DIR}  
gclient sync # this step sometimes fails and needs to be repeated

GN_ARGS="--unoptimized --no-goma --no-enable-unittests --disable-desktop-embeddings --no-build-glfw-shell --no-build-embedder-examples --no-stripped ${GN_EXTRA_ARGS}"

echo "Running GN with args: ${GN_ARGS}"
./engine/src/flutter/tools/gn ${GN_ARGS}


ninja -v -C ${BUILD_DIR} flutter/impeller/toolkit/interop:sdk 

mkdir -p ${INSTALL_DIR}/bin ${INSTALL_DIR}/lib ${INSTALL_DIR}/include
cp ${BUILD_DIR}/impellerc ${INSTALL_DIR}/bin
cp ${BUILD_DIR}/libimpeller.so ${INSTALL_DIR}/lib
cp ${SCRIPT_DIR}/3rdparty/flutter/engine/src/flutter/impeller/toolkit/interop/impeller.h ${INSTALL_DIR}/include
cp ${SCRIPT_DIR}/3rdparty/flutter/engine/src/flutter/impeller/toolkit/interop/impeller.hpp ${INSTALL_DIR}/include
