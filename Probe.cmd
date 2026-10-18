@echo off
chcp 65001 >nul
cd /d "%~dp0"
rem Live state of the PlayStation controller: sticks, buttons, gyro, touchpad.
cargo run --release -q -p dualsense --bin aikm-probe -- %*
pause
