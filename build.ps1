param([string]$Dotnet = '', [switch]$SkipTests, [switch]$Compact)
$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot
if (-not $Dotnet) {
    $bundledSdk = Join-Path $env:LOCALAPPDATA 'cap-dev\dotnet\dotnet.exe'
    if (Test-Path -LiteralPath $bundledSdk) { $Dotnet = $bundledSdk }
    else { $Dotnet = (Get-Command dotnet -ErrorAction Stop).Source }
}
function Invoke-Dotnet([string[]]$Arguments) {
    & $Dotnet @Arguments
    if ($LASTEXITCODE -ne 0) { throw "dotnet failed with exit code $LASTEXITCODE" }
}
Invoke-Dotnet @('build', 'cap.sln', '-c', 'Release', '--nologo', '-warnaserror')
if (-not $SkipTests) {
    foreach ($project in @('Cap.Tests', 'Cap.Capture.Tests', 'Cap.Annotation.Tests', 'Cap.Recording.Tests', 'Cap.Ocr.Tests', 'Cap.Scroll.Tests')) {
        Invoke-Dotnet @('run', '--project', "tests/$project/$project.csproj", '-c', 'Release', '--no-build')
    }
}
$compression = if ($Compact) { 'true' } else { 'false' }
Invoke-Dotnet @('publish', 'src/Cap.App/Cap.App.csproj', '-c', 'Release', '-r', 'win-x64', '--self-contained', 'true', '-p:PublishSingleFile=true', '-p:IncludeNativeLibrariesForSelfExtract=true', "-p:EnableCompressionInSingleFile=$compression", '-o', 'dist', '--nologo')
$executable = Get-Item -LiteralPath (Join-Path $PSScriptRoot 'dist\cap.exe')
Write-Output "$($executable.FullName) ($($executable.Length) bytes)"
$digest = (Get-FileHash -LiteralPath $executable.FullName -Algorithm SHA256).Hash
Write-Output "SHA256: $digest"
