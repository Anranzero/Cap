param([switch]$SkipTests, [switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
Push-Location -LiteralPath $PSScriptRoot
try {
    $portable = Join-Path $PSScriptRoot '..\..\.build-tools\Enter-RustBuild.ps1'
    if (Test-Path -LiteralPath $portable) { . $portable }
    & cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw 'Rust format check failed.' }
    if (-not $SkipTests) {
        & cargo test --locked --all-targets -- --test-threads=1
        if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed.' }
    }
    & cargo clippy --locked --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Rust clippy failed.' }
    if (-not $CheckOnly) {
        & cargo build --locked --release
        if ($LASTEXITCODE -ne 0) { throw 'Rust release build failed.' }
        New-Item -ItemType Directory -Path 'dist' -Force | Out-Null
        Copy-Item -LiteralPath 'target\release\cap-rust.exe' -Destination 'dist\cap-rust.exe'
        Get-FileHash -LiteralPath 'dist\cap-rust.exe' -Algorithm SHA256
    }
} finally { Pop-Location }
