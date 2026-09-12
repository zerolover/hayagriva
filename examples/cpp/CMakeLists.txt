cmake_minimum_required(VERSION 3.20)

project(hayagriva_cpp_example LANGUAGES CXX)

set(HAYAGRIVA_DIST_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../../dist/hayagriva-ffi" CACHE PATH "Path to the hayagriva FFI bundle")
set(HAYAGRIVA_INCLUDE_DIR "${HAYAGRIVA_DIST_DIR}/inc/hayagriva")
set(HAYAGRIVA_LIB_DIR "${HAYAGRIVA_DIST_DIR}/libs")

if(WIN32)
    set(HAYAGRIVA_SHARED_LIB "${HAYAGRIVA_LIB_DIR}/hayagriva_ffi.dll")
    set(HAYAGRIVA_IMPORT_LIB "${HAYAGRIVA_LIB_DIR}/hayagriva_ffi.lib")
elseif(APPLE)
    set(HAYAGRIVA_SHARED_LIB "${HAYAGRIVA_LIB_DIR}/libhayagriva_ffi.dylib")
else()
    set(HAYAGRIVA_SHARED_LIB "${HAYAGRIVA_LIB_DIR}/libhayagriva_ffi.so")
endif()

if(NOT EXISTS "${HAYAGRIVA_INCLUDE_DIR}/hayagriva.h")
    message(FATAL_ERROR "hayagriva headers not found under ${HAYAGRIVA_INCLUDE_DIR}. Build the FFI bundle into ${HAYAGRIVA_DIST_DIR} first.")
endif()

if(NOT EXISTS "${HAYAGRIVA_SHARED_LIB}")
    message(FATAL_ERROR "hayagriva shared library not found at ${HAYAGRIVA_SHARED_LIB}.")
endif()

add_executable(hayagriva_cpp_example main.cpp)
target_compile_features(hayagriva_cpp_example PRIVATE cxx_std_17)

if(MSVC)
    target_compile_options(hayagriva_cpp_example PRIVATE /utf-8)
endif()

add_library(hayagriva_ffi SHARED IMPORTED GLOBAL)
set_target_properties(
    hayagriva_ffi
    PROPERTIES
        IMPORTED_LOCATION "${HAYAGRIVA_SHARED_LIB}"
        INTERFACE_INCLUDE_DIRECTORIES "${HAYAGRIVA_INCLUDE_DIR}"
)

if(WIN32)
    if(NOT EXISTS "${HAYAGRIVA_IMPORT_LIB}")
        message(FATAL_ERROR "hayagriva import library not found at ${HAYAGRIVA_IMPORT_LIB}.")
    endif()

    set_target_properties(hayagriva_ffi PROPERTIES IMPORTED_IMPLIB "${HAYAGRIVA_IMPORT_LIB}")
endif()

get_filename_component(HAYAGRIVA_SHARED_LIB_NAME "${HAYAGRIVA_SHARED_LIB}" NAME)

target_include_directories(hayagriva_cpp_example PRIVATE
    "${CMAKE_CURRENT_SOURCE_DIR}"
    "${HAYAGRIVA_INCLUDE_DIR}"
)
target_link_libraries(hayagriva_cpp_example PRIVATE hayagriva_ffi)

add_custom_command(
    TARGET hayagriva_cpp_example
    POST_BUILD
    COMMAND "${CMAKE_COMMAND}" -E copy_if_different
            "${HAYAGRIVA_SHARED_LIB}"
            "$<TARGET_FILE_DIR:hayagriva_cpp_example>/${HAYAGRIVA_SHARED_LIB_NAME}"
)
