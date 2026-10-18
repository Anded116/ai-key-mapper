@echo off
chcp 65001 >nul
setlocal
cd /d "%~dp0"

rem Running copies lock their .exe files: the build would fail and the old version would start.
for %%P in (testrange.exe aikm.exe) do (
  tasklist /fi "imagename eq %%P" | find /i "%%P" >nul
  if not errorlevel 1 (
    echo Уже запущен %%P. Закрой TestRange и окно aikm, потом запусти снова.
    goto :fail
  )
)

where cargo >nul 2>nul
if not errorlevel 1 (
  echo Сборка TestRange и движка...
  cargo build --release -p testrange -p engine -p hidhide-cli
  if errorlevel 1 goto :fail
)
if not exist "target\release\testrange.exe" (
  echo Нет target\release\testrange.exe и нет Rust для сборки: https://rustup.rs
  goto :fail
)

if /i "%~1"=="jsm" (
  call :start_jsm
  if errorlevel 1 goto :fail
)
if /i "%~1"=="aikm" (
  call :start_aikm
  if errorlevel 1 goto :fail
)

start "" /D "%~dp0demo" "%~dp0target\release\testrange.exe"
exit /b 0

:start_aikm
tasklist /fi "imagename eq JoyShockMapper.exe" | find /i "JoyShockMapper.exe" >nul
if not errorlevel 1 (
  echo Запущен JoyShockMapper. Закрой его: два маппера сразу задвоят ввод.
  exit /b 1
)
echo Положи геймпад на стол: первая секунда — калибровка гироскопа.
start "aikm" cmd /k ""%~dp0target\release\aikm.exe" run "%~dp0profiles\testrange.toml""
timeout /t 2 /nobreak >nul
exit /b 0

:start_jsm
tasklist /fi "imagename eq JoyShockMapper.exe" | find /i "JoyShockMapper.exe" >nul
if not errorlevel 1 (
  echo JoyShockMapper уже запущен. Закрой его: иначе загрузится не тот пресет или ввод задвоится.
  exit /b 1
)
tasklist /fi "imagename eq aikm.exe" | find /i "aikm.exe" >nul
if not errorlevel 1 (
  echo Запущен aikm. Закрой его окно: два маппера сразу задвоят ввод.
  exit /b 1
)
set "JSM_HOME=%~dp0tools\JoyShockMapper\JoyShockMapper_SDL2_x64"
if not exist "%JSM_HOME%\JoyShockMapper.exe" (
  echo Скачиваю JoyShockMapper v3.6.1...
  powershell -NoProfile -Command "$ErrorActionPreference='Stop'; New-Item -ItemType Directory -Force 'tools\dl' | Out-Null; Invoke-WebRequest 'https://github.com/Electronicks/JoyShockMapper/releases/download/v3.6.1/JoyShockMapper_x64.zip' -OutFile 'tools\dl\JoyShockMapper_x64.zip'; Expand-Archive -Force 'tools\dl\JoyShockMapper_x64.zip' 'tools\JoyShockMapper'"
  if errorlevel 1 exit /b 1
)
echo Положи геймпад на стол: 3 секунды калибровки гироскопа.
start "JoyShockMapper" /D "%JSM_HOME%" "%JSM_HOME%\JoyShockMapper.exe" "%~dp0jsm"
timeout /t 4 /nobreak >nul
exit /b 0

:fail
pause
exit /b 1
