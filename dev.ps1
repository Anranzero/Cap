param([string]$Dotnet = '')
$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot
$previewDirectory = Join-Path $PSScriptRoot '.artifacts\dev-preview'
New-Item -ItemType Directory -Path $previewDirectory -Force | Out-Null
$refreshFile = Join-Path $previewDirectory 'refresh.trigger'
$created = $false
$watchMutex = New-Object System.Threading.Mutex($true, 'Local\cap.development.watch.8E306F42', [ref]$created)
try {
    # Reopening the desktop shortcut refreshes the existing preview instead of starting a second watcher.
    [IO.File]::WriteAllText($refreshFile, [DateTime]::UtcNow.ToString('O'))
    if (-not $created) { return }
    if (-not $Dotnet) {
        $bundledSdk = Join-Path $env:LOCALAPPDATA 'cap-dev\dotnet\dotnet.exe'
        if (Test-Path -LiteralPath $bundledSdk) { $Dotnet = $bundledSdk }
        else { $Dotnet = (Get-Command dotnet -ErrorAction Stop).Source }
    }
    $env:DOTNET_WATCH_SUPPRESS_LAUNCH_BROWSER = '1'
    Write-Output 'cap preview: save C# / XAML / icons to rebuild and reopen the window.'
    & $Dotnet watch --no-hot-reload --no-launch-profile --project 'tests/Cap.Ui.Tests/Cap.Ui.Tests.csproj' run --configuration Debug -- $previewDirectory --preview
    if ($LASTEXITCODE -ne 0) { throw "Preview watcher exited with code $LASTEXITCODE" }
}
finally { if ($created) { $watchMutex.ReleaseMutex() }; $watchMutex.Dispose() }
