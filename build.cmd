@echo off
rem Builds the SDK's commands into target\bin as juku-emu's build.cmd builds
rem its Rust code (its spec section 11.1): std built from source, the release
rem profile's immediate-abort panics, no panic locations and no Debug
rem formatting, the C runtime linked in. A command is a crate in
rem crates\<name> with a src\main.rs: juku-<name>.exe. The same as build.sh.
rem
rem   build.cmd           release build
rem   build.cmd debug     unoptimized build with debug info
rem   build.cmd install   release build of each command, installed into
rem                       cargo's bin folder
rem
rem Needs nightly Rust with rust-src and the Visual Studio Build Tools (MSVC
rem linker). The file has no labels, so it works with LF line endings too.

setlocal
cd /d "%~dp0"

set TARGET=x86_64-pc-windows-msvc
set CARGO_PROFILE=--release
set CARGO_OUT=release
rem The C runtime is linked in, except the Universal CRT, which is part of
rem Windows 10 and later: it is taken from ucrtbase.dll.
set CRT=-Ctarget-feature=+crt-static -Clink-arg=/NODEFAULTLIB:libucrt.lib -Clink-arg=ucrt.lib
set RUSTFLAGS=%CRT% -Zlocation-detail=none -Zfmt-debug=none

if "%~1"=="debug" (
    set CARGO_PROFILE=
    set CARGO_OUT=debug
    set RUSTFLAGS=%CRT% -Zlocation-detail=none
) else if "%~1"=="install" (
    for /d %%c in (crates\*) do if exist "%%c\src\main.rs" cargo install --path "%%c" --target %TARGET% --target-dir target\cargo -Z build-std=std || exit /b 1
    exit /b 0
) else if not "%~1"=="" (
    echo usage: build.cmd [debug^|install] 1>&2
    exit /b 2
)

cargo build --workspace --target %TARGET% --target-dir target\cargo -Z build-std=std %CARGO_PROFILE% || exit /b 1
if not exist target\bin mkdir target\bin
for /d %%c in (crates\*) do if exist "%%c\src\main.rs" copy /y target\cargo\%TARGET%\%CARGO_OUT%\juku-%%~nxc.exe target\bin\juku-%%~nxc.exe >nul || exit /b 1
echo Built the commands into target\bin
