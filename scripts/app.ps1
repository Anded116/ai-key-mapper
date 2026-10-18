# Builds and starts the ai-key-mapper window.
#   app.ps1        release build: one aikm-app.exe, lives in the tray, can start with Windows
#   app.ps1 -Dev   development: tauri dev with vite and hot reload
param([switch]$Dev)

[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$root = Split-Path -Parent $PSScriptRoot
Set-Location (Join-Path $root "app")
$exe = Join-Path $root "app\src-tauri\target\release\aikm-app.exe"

function Stop-WithMessage([string]$Message) {
    Write-Host $Message -ForegroundColor Red
    Read-Host "Enter — закрыть"
    exit 1
}

# The window runs the mapping engine itself: a second mapper would double every input.
$others = Get-Process aikm, JoyShockMapper -ErrorAction SilentlyContinue
if ($others) {
    $names = ($others | ForEach-Object { "$($_.ProcessName).exe" } | Sort-Object -Unique) -join ", "
    Stop-WithMessage "Уже запущено: $names. Закрой его: окно само работает маппером, два маппера задвоят ввод."
}

$window = Get-Process aikm-app -ErrorAction SilentlyContinue
if ($window) {
    if ($Dev) { Stop-WithMessage "Окно ai-key-mapper уже открыто. Выйди из него через меню в трее, потом запусти снова." }
    # The running copy locks its exe; a second start just brings its window up.
    Start-Process $window[0].Path
    exit 0
}

if (-not (Get-Command npm -ErrorAction SilentlyContinue)) { Stop-WithMessage "Нет Node.js: https://nodejs.org" }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { Stop-WithMessage "Нет Rust для сборки: https://rustup.rs" }

# Hiding the pad from games goes through this helper.
cargo build --release -p hidhide-cli --manifest-path (Join-Path $root "Cargo.toml")
if ($LASTEXITCODE -ne 0) { Stop-WithMessage "Сборка aikm-hidhide не удалась." }

# Tools the chat agent runs: aikm check / --dry, aikm-import, testrange --screenshot.
cargo build --release -p engine -p jsm-import -p testrange --manifest-path (Join-Path $root "Cargo.toml")
if ($LASTEXITCODE -ne 0) { Write-Host "Утилиты агента не собрались (TestRange открыт?): агент соберёт их сам." -ForegroundColor Yellow }

if (-not (Test-Path "node_modules\.bin\tauri.cmd")) {
    Write-Host "Ставлю npm-зависимости..."
    npm install
    if ($LASTEXITCODE -ne 0) { Stop-WithMessage "npm install не удался." }
}

if ($Dev) {
    Write-Host "ai-key-mapper в режиме разработки. Первая сборка Rust занимает несколько минут."
    npm run tauri dev
    if ($LASTEXITCODE -ne 0) { Stop-WithMessage "Приложение завершилось с ошибкой." }
    exit 0
}

Write-Host "Сборка ai-key-mapper. Первый раз — несколько минут, дальше — секунды."
npx tauri build --no-bundle
if ($LASTEXITCODE -ne 0) { Stop-WithMessage "Сборка не удалась." }
Start-Process $exe
