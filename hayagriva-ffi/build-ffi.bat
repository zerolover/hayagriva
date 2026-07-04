@echo off
setlocal EnableExtensions

set "FFI_DIR=%~dp0"
for %%I in ("%FFI_DIR%..") do set "REPO_ROOT=%%~fI"

set "DIST_DIR=%REPO_ROOT%\dist\hayagriva-ffi"
set "INCLUDE_DIR=%DIST_DIR%\inc\hayagriva"
set "LIB_DIR=%DIST_DIR%\libs"
set "LIB_SOURCE_DIR=%FFI_DIR%\target\release"

where cargo >nul 2>nul
if errorlevel 1 (
    echo cargo not found in PATH 1>&2
    exit /b 1
)

if not exist "%INCLUDE_DIR%" mkdir "%INCLUDE_DIR%"
if not exist "%LIB_DIR%" mkdir "%LIB_DIR%"

cargo build --manifest-path "%FFI_DIR%\Cargo.toml" --release

copy /Y "%FFI_DIR%\include\hayagriva.h" "%INCLUDE_DIR%\hayagriva.h" >nul
copy /Y "%LIB_SOURCE_DIR%\hayagriva_ffi.dll" "%LIB_DIR%\hayagriva_ffi.dll" >nul
copy /Y "%LIB_SOURCE_DIR%\hayagriva_ffi.dll.lib" "%LIB_DIR%\hayagriva_ffi.lib" >nul

dir /b /s "%DIST_DIR%"
