# Nid — one-time setup: downloads the local whisper STT model.
# Usage:  powershell -NoProfile -ExecutionPolicy Bypass -File scripts/setup-models.ps1

$ErrorActionPreference = "Stop"

$modelDir = Join-Path $env:APPDATA "com.julienawonga.nid\models"
$modelPath = Join-Path $modelDir "ggml-base.bin"
$url = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin"

New-Item -ItemType Directory -Force -Path $modelDir | Out-Null

if (Test-Path $modelPath) {
    Write-Host "✓ whisper model already present: $modelPath" -ForegroundColor Green
    exit 0
}

Write-Host "Downloading whisper base model (~148 MB)..."
Invoke-WebRequest -Uri $url -OutFile $modelPath
Write-Host "✓ model ready: $modelPath" -ForegroundColor Green
