#!/bin/bash

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export PATH=$PATH:/pub_data/sources/google/depot_tools/

BUILD_TYPE=debug_unopt

cd ${SCRIPT_DIR}/3rdparty/flutter/
cp ./engine/scripts/standard.gclient .gclient
gclient sync # this step sometimes fails and needs to be repeated
./engine/src/flutter/tools/gn --unoptimized --no-goma --no-enable-unittests --disable-desktop-embeddings --no-build-glfw-shell --no-build-embedder-examples --no-stripped --enable-vulkan

ninja -C ./engine/src/out/host_${BUILD_TYPE} flutter/impeller/toolkit/interop:sdk 

INSTALL_DIR=${SCRIPT_DIR}/impeller_install_${BUILD_TYPE}/
mkdir -p ${INSTALL_DIR}/bin ${INSTALL_DIR}/lib ${INSTALL_DIR}/include
cp ${SCRIPT_DIR}/3rdparty/flutter/engine/src/out/host_${BUILD_TYPE}/impellerc ${INSTALL_DIR}/bin
cp ${SCRIPT_DIR}/3rdparty/flutter/engine/src/out/host_${BUILD_TYPE}/libimpeller.so ${INSTALL_DIR}/lib
cp ${SCRIPT_DIR}/3rdparty/flutter/engine/src/flutter/impeller/toolkit/interop/impeller.h ${INSTALL_DIR}/include
cp ${SCRIPT_DIR}/3rdparty/flutter/engine/src/flutter/impeller/toolkit/interop/impeller.hpp ${INSTALL_DIR}/include
