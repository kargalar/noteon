@echo off
setlocal
cd /d "%~dp0"

echo Noteon baslatiliyor...
cargo run --release %*
if %ERRORLEVEL% neq 0 (
    echo.
    echo Bir hata olustu (Hata kodu: %ERRORLEVEL%).
    pause
)

