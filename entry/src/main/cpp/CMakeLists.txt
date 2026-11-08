cmake_minimum_required(VERSION 3.5.0)
project(photocraft_native)
set(CMAKE_CXX_STANDARD 17)
get_filename_component(PHOTOCRAFT_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../../.." ABSOLUTE)
# OHOS_ARCH is the ABI hvigor is building (abiFilters in entry/build-profile.json5). This mapping
# is the only place in the C++ layer that knows about architectures: adding an ABI means adding a
# line here plus the corresponding triple in scripts/build_native.sh and scripts/verify_native.py.
if(OHOS_ARCH STREQUAL "arm64-v8a")
  set(RUST_TARGET aarch64-unknown-linux-ohos)
elseif(OHOS_ARCH STREQUAL "x86_64")
  set(RUST_TARGET x86_64-unknown-linux-ohos)
elseif(OHOS_ARCH STREQUAL "armeabi-v7a")
  set(RUST_TARGET armv7-unknown-linux-ohos)
else()
  message(FATAL_ERROR "PhotoCraft does not support OHOS_ARCH=${OHOS_ARCH}")
endif()
if(NOT DEFINED PHOTOCRAFT_RUST_PROFILE)
  set(PHOTOCRAFT_RUST_PROFILE release)
endif()
# PHOTOCRAFT_RUST_TARGET_DIR points at a target directory outside the workspace (CI caches it); it
# must contain <triple>/<profile>/libphotocraft_hmos.a for the ABI being built.
if(DEFINED PHOTOCRAFT_RUST_TARGET_DIR AND NOT PHOTOCRAFT_RUST_TARGET_DIR STREQUAL "")
  set(RUST_LIBRARY "${PHOTOCRAFT_RUST_TARGET_DIR}/${RUST_TARGET}/${PHOTOCRAFT_RUST_PROFILE}/libphotocraft_hmos.a")
else()
  set(RUST_LIBRARY "${PHOTOCRAFT_ROOT}/native/rust/target/${RUST_TARGET}/${PHOTOCRAFT_RUST_PROFILE}/libphotocraft_hmos.a")
endif()
if(NOT EXISTS "${RUST_LIBRARY}")
  message(FATAL_ERROR "Run scripts/build_native.sh before building HAP: ${RUST_LIBRARY}")
endif()
add_library(photocraft SHARED bridge.cpp)
# v0.21.0 pulls in iana-time-zone, whose OHOS backend (tz_ohos.rs) calls
# OH_TimeService_GetTimeZone from libtime_service_ndk.so. The Rust static library only declares
# the symbol, so the final shared object has to link the NDK library explicitly.
target_link_libraries(photocraft PRIVATE "${RUST_LIBRARY}" ace_napi.z ace_ndk.z native_window native_vsync hilog_ndk.z time_service_ndk EGL GLESv3 dl m pthread)

target_link_options(photocraft PRIVATE "-Wl,--gc-sections" "-Wl,--exclude-libs,ALL")
