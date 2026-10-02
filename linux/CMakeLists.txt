cmake_minimum_required(VERSION 3.13)
project(morrow_linux_foundation LANGUAGES CXX)

if(DEFINED FLUTTER_TARGET_PLATFORM)
  message(FATAL_ERROR
    "Linux protected product wiring unavailable; build the diagnostic GTK prerequisite with CMake directly")
endif()

set(BINARY_NAME "morrow_studio")
set(APPLICATION_ID "org.morrow.studio.foundation")
cmake_policy(SET CMP0063 NEW)
set(CMAKE_INSTALL_RPATH "$ORIGIN/lib")
if(FLUTTER_TARGET_PLATFORM_SYSROOT)
  set(CMAKE_SYSROOT "${FLUTTER_TARGET_PLATFORM_SYSROOT}")
  set(CMAKE_FIND_ROOT_PATH "${CMAKE_SYSROOT}")
  set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
  set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE ONLY)
  set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
  set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
endif()
if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
  set(CMAKE_BUILD_TYPE "Debug" CACHE STRING "Foundation build mode" FORCE)
  set_property(CACHE CMAKE_BUILD_TYPE PROPERTY STRINGS "Debug" "Profile" "Release")
endif()
function(APPLY_STANDARD_SETTINGS TARGET)
  target_compile_features(${TARGET} PUBLIC cxx_std_17)
  target_compile_options(${TARGET} PRIVATE -Wall -Wextra -Werror)
endfunction()
find_package(PkgConfig REQUIRED)
pkg_check_modules(GTK REQUIRED IMPORTED_TARGET gtk+-3.0)
add_subdirectory("runner")

# This target is deliberately a GTK prerequisite executable, not a Flutter
# product bundle. `flutter build linux` must fail explicitly until an owner and
# strict Store are integrated, rather than succeed with legacy/plaintext paths.

# No install rules: this unqualified probe is not a distributable product.
