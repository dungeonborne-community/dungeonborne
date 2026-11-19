@echo off
rem Build dbnative.dll (x64) -> ue4ss\Mods\DungeonborneRuClient\native\dbnative.dll
rem Folder "native", not "dlls": UE4SS loads dlls\main.dll as a C++ mod by itself.
rem Two compilers, the same as the workspace's Rust builds (config/tools.lock):
rem   1. MSVC: the newest Visual Studio / Build Tools with C++ (vswhere ships with the VS installer);
rem   2. without Visual Studio: clang-cl from home\toolchains\llvm (cargo xtask setup --tool=llvm) with the CRT and the
rem      Windows SDK from cargo-xwin (home\cache\xwin\xwin, cargo xtask setup).
rem   build.bat         MSVC if it is installed, else clang-cl
rem   build.bat clang   clang-cl even when Visual Studio is there (also when vcvars64.bat fails on an odd PATH)
setlocal
cd /d "%~dp0"
set "OUTDIR=..\..\Mods\DungeonborneRuClient\native"
if not exist "%OUTDIR%" mkdir "%OUTDIR%"
if defined DB_HOME (set "HOMEDIR=%DB_HOME%") else (set "HOMEDIR=%~dp0..\..\..\..\..\home")
set "CLANG=%HOMEDIR%\toolchains\llvm\bin\clang-cl.exe"
set "XWIN=%HOMEDIR%\cache\xwin\xwin"

if /i "%~1"=="clang" goto clang
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if not exist "%VSWHERE%" goto clang
set "VSDIR="
for /f "usebackq delims=" %%i in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSDIR=%%i"
if not defined VSDIR goto clang
call "%VSDIR%\VC\Auxiliary\Build\vcvars64.bat" >nul || exit /b 1
echo compiler: MSVC
cl /nologo /O2 /W3 /utf-8 /LD /MT /D_CRT_SECURE_NO_WARNINGS dbnative.c /Fe:%OUTDIR%\dbnative.dll /link winhttp.lib winmm.lib || exit /b 1
goto done

:clang
if not exist "%CLANG%" (echo no MSVC and no clang-cl: install Build Tools with C++, or cargo xtask setup --apply --tool=llvm & exit /b 1)
if not exist "%XWIN%\sdk\lib\um\x86_64" (echo no Windows SDK from cargo-xwin in %XWIN%: cargo xtask setup --apply & exit /b 1)
echo compiler: clang-cl + xwin
"%CLANG%" /nologo /O2 /W3 /utf-8 /LD /MT /D_CRT_SECURE_NO_WARNINGS -fuse-ld=lld --target=x86_64-pc-windows-msvc ^
  /imsvc "%XWIN%\crt\include" /imsvc "%XWIN%\sdk\include\ucrt" /imsvc "%XWIN%\sdk\include\um" /imsvc "%XWIN%\sdk\include\shared" ^
  dbnative.c /Fe:%OUTDIR%\dbnative.dll ^
  /link /libpath:"%XWIN%\crt\lib\x86_64" /libpath:"%XWIN%\sdk\lib\um\x86_64" /libpath:"%XWIN%\sdk\lib\ucrt\x86_64" winhttp.lib winmm.lib || exit /b 1

:done
del /q dbnative.obj %OUTDIR%\dbnative.exp %OUTDIR%\dbnative.lib 2>nul
rem which source this dll was built from (cargo xtask mods check compares content, not file times)
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0stamp.ps1" || exit /b 1
echo OK
