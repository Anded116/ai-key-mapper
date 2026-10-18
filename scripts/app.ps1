# Starts the ai-key-mapper window (Tauri dev build).
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$root = Split-Path -Parent $PSScriptRoot
Set-Location (Join-Path $root "app")

function Stop-WithMessage([string]$Message) {
    Write-Host $Message -ForegroundColor Red
    Read-Host "Enter — закрыть"
    exit 1
}

if (-not (Get-Command npm -ErrorAction SilentlyContinue)) { Stop-WithMessage "Нет Node.js: https://nodejs.org" }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { Stop-WithMessage "Нет Rust для сборки: https://rustup.rs" }

# The window runs the mapping engine itself: a second mapper would double every input.
$running = Get-Process aikm, aikm-app, JoyShockMapper -ErrorAction SilentlyContinue
if ($running) {
    $names = ($running | ForEach-Object { "$($_.ProcessName).exe" } | Sort-Object -Unique) -join ", "
    Stop-WithMessage "Уже запущено: $names. Закрой его: окно само работает маппером, два маппера задвоят ввод."
}

# Hiding the pad from games goes through this helper.
cargo build --release -p hidhide-cli --manifest-path (Join-Path $root "Cargo.toml")
if ($LASTEXITCODE -ne 0) { Stop-WithMessage "Сборка aikm-hidhide не удалась." }

if (-not (Test-Path "node_modules\.bin\tauri.cmd")) {
    Write-Host "Ставлю npm-зависимости..."
    npm install
    if ($LASTEXITCODE -ne 0) { Stop-WithMessage "npm install не удался." }
}

Write-Host "Запуск ai-key-mapper. Первая сборка Rust занимает несколько минут."
npm run tauri dev
if ($LASTEXITCODE -ne 0) { Stop-WithMessage "Приложение завершилось с ошибкой." }
