$ErrorActionPreference = 'Stop'
$noteonRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $noteonRoot
try {
    cargo build --release --locked --bin noteon
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    $noteonPackage = Join-Path $noteonRoot 'dist/Noteon'
    New-Item -ItemType Directory -Path $noteonPackage -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $noteonRoot 'target/release/noteon.exe') -Destination (Join-Path $noteonPackage 'Noteon.exe')
    Copy-Item -LiteralPath (Join-Path $noteonRoot 'README.md') -Destination $noteonPackage
    Copy-Item -LiteralPath (Join-Path $noteonRoot 'LICENSE') -Destination $noteonPackage
    $noteonDocs = Join-Path $noteonPackage 'docs'
    New-Item -ItemType Directory -Path $noteonDocs -Force | Out-Null
    Copy-Item -Path (Join-Path $noteonRoot 'docs/*.md') -Destination $noteonDocs
    Copy-Item -LiteralPath (Join-Path $noteonRoot 'docs/performance-results.txt') -Destination $noteonDocs
    Write-Output "Package ready: $noteonPackage"
} finally {
    Pop-Location
}
