project(BridgethingSession)
cmake_minimum_required(VERSION 3.9.0)

set(PACKAGE_NAME BridgethingSession)
set(CMAKE_CXX_STANDARD 20)

# session-rn has no custom C++ - all sources come from nitrogen. The cmake
# autolinking script `target_sources(BridgethingSession PRIVATE ...)`s
# everything we need; we just declare the library here with the JNI_OnLoad
# adapter so the .so has at least one source to start with.
file(GLOB_RECURSE android_files RELATIVE ${CMAKE_SOURCE_DIR}
     "src/main/cpp/**.cpp"
)
add_library(${PACKAGE_NAME} SHARED ${android_files})

# Pull in nitrogen-generated sources + their include dirs + their link reqs
# (fbjni, ReactAndroid, react-native-nitro-modules).
include(${CMAKE_SOURCE_DIR}/../nitrogen/generated/android/BridgethingSession+autolinking.cmake)

include_directories("src/main/cpp")

find_library(LOG_LIB log)

target_link_libraries(
        ${PACKAGE_NAME}
        ${LOG_LIB}
        android
)
