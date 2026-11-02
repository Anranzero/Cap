param(
    [Parameter(Mandatory = $true)][string]$GifPath,
    [Parameter(Mandatory = $true)][string]$BgraPath
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$image = [System.Drawing.Image]::FromFile((Resolve-Path -LiteralPath $GifPath))
try {
    if ($image.Width -ne 160 -or $image.Height -ne 100) { throw 'Native GIF canvas dimensions differ.' }
    $dimension = [System.Drawing.Imaging.FrameDimension]::Time
    if ($image.GetFrameCount($dimension) -ne 2) { throw 'Native GIF decoder did not find two coalesced frames.' }
    $delays = $image.GetPropertyItem(0x5100).Value
    if ([BitConverter]::ToUInt32($delays, 0) -ne 7 -or [BitConverter]::ToUInt32($delays, 4) -ne 13) {
        throw 'Native GIF delays differ from 7cs and 13cs.'
    }
    [void]$image.SelectActiveFrame($dimension, 0)
    $pixels = [System.IO.File]::ReadAllBytes((Resolve-Path -LiteralPath $BgraPath))
    for ($y = 0; $y -lt 100; $y++) {
        for ($x = 0; $x -lt 160; $x++) {
            $color = $image.GetPixel($x, $y)
            $p = ($y * 160 + $x) * 4
            if ($color.B -ne $pixels[$p] -or $color.G -ne $pixels[$p + 1] -or $color.R -ne $pixels[$p + 2]) {
                throw "Native GIF palette/LZW pixel differs at ($x,$y)."
            }
        }
    }
    [void]$image.SelectActiveFrame($dimension, 1)
    $changed = $image.GetPixel(70, 50)
    if ($changed.R -ne 255 -or $changed.G -ne 0 -or $changed.B -ne 0) {
        throw 'Native GIF decoder did not composite the 1x1 delta at (70,50).'
    }
    Write-Output 'PASS: native GDI+ GIF decode, orientation, all palette pixels, two coalesced frames, 7/13cs delays and 1x1 delta.'
}
finally { $image.Dispose() }
