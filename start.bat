@echo off
setlocal
cd /d "%~dp0"

echo Noteon baslatiliyor...
cargo run %*
if errorlevel 1 (
    echo.
    echo Bir hata olustu.
    pause
)
