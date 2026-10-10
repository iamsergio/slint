# Copyright © SixtyFPS GmbH <info@slint.dev>
# SPDX-License-Identifier: MIT

cmake_minimum_required(VERSION 3.21)
project(native_component LANGUAGES CXX)

if (NOT TARGET Slint::Slint)
    find_package(Slint REQUIRED)
endif()

add_executable(native_component main.cpp prime_finder.cpp)
target_link_libraries(native_component PRIVATE Slint::Slint)
slint_target_sources(native_component app.slint)
