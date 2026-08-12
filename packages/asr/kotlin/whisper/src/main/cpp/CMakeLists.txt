cmake_minimum_required(VERSION 3.22.1)

project(bridgething_whisper CXX)

include(FetchContent)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

set(CMAKE_C_FLAGS_DEBUG "-O3 -DNDEBUG")
set(CMAKE_CXX_FLAGS_DEBUG "-O3 -DNDEBUG")

set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
set(WHISPER_BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(WHISPER_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(WHISPER_BUILD_SERVER OFF CACHE BOOL "" FORCE)
set(WHISPER_CURL OFF CACHE BOOL "" FORCE)
set(WHISPER_SDL2 OFF CACHE BOOL "" FORCE)
set(GGML_OPENMP OFF CACHE BOOL "" FORCE)
set(GGML_LLAMAFILE OFF CACHE BOOL "" FORCE)
set(GGML_BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(GGML_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)

FetchContent_Declare(
  whisper_cpp
  URL https://github.com/ggml-org/whisper.cpp/archive/refs/tags/v1.9.1.tar.gz
  URL_HASH SHA256=147267177eef7b22ec3d2476dd514d1b12e160e176230b740e3d1bd600118447
)
FetchContent_MakeAvailable(whisper_cpp)

add_library(bridgething_whisper SHARED whisper_jni.cpp)

target_link_libraries(bridgething_whisper PRIVATE whisper log)

target_link_options(bridgething_whisper PRIVATE "-Wl,-z,max-page-size=16384")
