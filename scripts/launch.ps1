# Builds and starts TestRange and/or a mapper.
#   launch.ps1          TestRange only
#   launch.ps1 jsm      TestRange + JoyShockMapper v3.6.1 with the preset from jsm\
#   launch.ps1 aikm     TestRange + our engine with profiles\testrange.toml
#   launch.ps1 auto     our engine only, picking the profile by the game in focus
param([string]$Mode = "")

[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

function Stop-WithMessage([string]$Message) {
    Write-Host $Message -ForegroundColor Red
    Read-Host "Enter — закрыть"
    exit 1
}

$withGame = $Mode -ne "auto"
$withAikm = $Mode -in @("aikm", "auto")

# Running copies lock their .exe files: the build would fail and the old version would stay.
$locked = @()
if ($withGame) { $locked += "testrange" }
if ($withAikm) { $locked += "aikm" }
$running = Get-Process $locked -ErrorAction SilentlyContinue
if ($running) {
    $names = ($running | ForEach-Object { "$($_.ProcessName).exe" } | Sort-Object -Unique) -join ", "
    Stop-WithMessage "Уже запущено: $names. Закрой его, потом запусти снова."
}
if ($withAikm -and (Get-Process JoyShockMapper -ErrorAction SilentlyContinue)) {
    Stop-WithMessage "Запущен JoyShockMapper. Закрой его: два маппера сразу задвоят ввод."
}
if ($Mode -ne "" -and (Get-Process aikm-app -ErrorAction SilentlyContinue)) {
    Stop-WithMessage "Открыто окно ai-key-mapper: оно уже работает маппером. Закрой его или запусти просто TestRange.cmd."
}

if (Get-Command cargo -ErrorAction SilentlyContinue) {
    $packages = @()
    if ($withGame) { $packages += "-p", "testrange" }
    if ($withAikm) { $packages += "-p", "engine", "-p", "hidhide-cli" }
    Write-Host "Сборка..."
    cargo build --release @packages
    if ($LASTEXITCODE -ne 0) { Stop-WithMessage "Сборка не удалась." }
}
$game = Join-Path $root "target\release\testrange.exe"
$aikm = Join-Path $root "target\release\aikm.exe"
if ($withGame -and -not (Test-Path $game)) { Stop-WithMessage "Нет $game и нет Rust для сборки: https://rustup.rs" }
if ($withAikm -and -not (Test-Path $aikm)) { Stop-WithMessage "Нет $aikm и нет Rust для сборки: https://rustup.rs" }

function Start-Aikm([string]$Target) {
    Write-Host "Положи геймпад на стол: первая секунда — калибровка гироскопа."
    # cmd /k keeps the window open, so errors stay readable.
    Start-Process cmd.exe -ArgumentList "/k `"chcp 65001 >nul & `"$aikm`" run `"$Target`"`""
    Start-Sleep -Seconds 2
}

switch ($Mode) {
    "jsm" {
        if (Get-Process JoyShockMapper -ErrorAction SilentlyContinue) {
            Stop-WithMessage "JoyShockMapper уже запущен. Закрой его: иначе загрузится не тот пресет или ввод задвоится."
        }
        $jsmHome = Join-Path $root "tools\JoyShockMapper\JoyShockMapper_SDL2_x64"
        $jsm = Join-Path $jsmHome "JoyShockMapper.exe"
        if (-not (Test-Path $jsm)) {
            Write-Host "Скачиваю JoyShockMapper v3.6.1..."
            New-Item -ItemType Directory -Force (Join-Path $root "tools\dl") | Out-Null
            $zip = Join-Path $root "tools\dl\JoyShockMapper_x64.zip"
            Invoke-WebRequest "https://github.com/Electronicks/JoyShockMapper/releases/download/v3.6.1/JoyShockMapper_x64.zip" -OutFile $zip
            Expand-Archive -Force $zip (Join-Path $root "tools\JoyShockMapper")
        }
        Write-Host "Положи геймпад на стол: 3 секунды калибровки гироскопа."
        Start-Process $jsm -ArgumentList "`"$(Join-Path $root 'jsm')`"" -WorkingDirectory $jsmHome
        Start-Sleep -Seconds 4
    }
    "aikm" { Start-Aikm (Join-Path $root "profiles\testrange.toml") }
    "auto" { Start-Aikm (Join-Path $root "profiles") }
}

if ($withGame) {
    Start-Process $game -WorkingDirectory (Join-Path $root "demo")
}
