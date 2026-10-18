# Builds and starts TestRange, optionally with a mapper next to it.
#   launch.ps1          TestRange only
#   launch.ps1 jsm      JoyShockMapper v3.6.1 with the preset from jsm\
#   launch.ps1 aikm     our engine with profiles\testrange.toml
param([string]$Mode = "")

[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

function Stop-WithMessage([string]$Message) {
    Write-Host $Message -ForegroundColor Red
    Read-Host "Enter — закрыть"
    exit 1
}

# Running copies lock their .exe files: the build would fail and the old version would stay.
$running = Get-Process testrange, aikm -ErrorAction SilentlyContinue
if ($running) {
    $names = ($running | ForEach-Object { "$($_.ProcessName).exe" } | Sort-Object -Unique) -join ", "
    Stop-WithMessage "Уже запущено: $names. Закрой TestRange и окно aikm, потом запусти снова."
}

if (Get-Command cargo -ErrorAction SilentlyContinue) {
    Write-Host "Сборка TestRange и движка..."
    cargo build --release -p testrange -p engine -p hidhide-cli
    if ($LASTEXITCODE -ne 0) { Stop-WithMessage "Сборка не удалась." }
}
$game = Join-Path $root "target\release\testrange.exe"
if (-not (Test-Path $game)) { Stop-WithMessage "Нет $game и нет Rust для сборки: https://rustup.rs" }

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
    "aikm" {
        if (Get-Process JoyShockMapper -ErrorAction SilentlyContinue) {
            Stop-WithMessage "Запущен JoyShockMapper. Закрой его: два маппера сразу задвоят ввод."
        }
        $aikm = Join-Path $root "target\release\aikm.exe"
        $profilePath = Join-Path $root "profiles\testrange.toml"
        Write-Host "Положи геймпад на стол: первая секунда — калибровка гироскопа."
        # cmd /k keeps the window open, so errors stay readable.
        Start-Process cmd.exe -ArgumentList "/k `"chcp 65001 >nul & `"$aikm`" run `"$profilePath`"`""
        Start-Sleep -Seconds 2
    }
}

Start-Process $game -WorkingDirectory (Join-Path $root "demo")
