param(
    [string]$OutputDirectory = (Join-Path $PSScriptRoot "..\assets")
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null

function New-KRunBitmap([int]$Size) {
    $bitmap = New-Object System.Drawing.Bitmap $Size, $Size, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $graphics.Clear([System.Drawing.Color]::Transparent)

    $teal = [System.Drawing.ColorTranslator]::FromHtml("#168393")
    $white = [System.Drawing.Color]::White
    $margin = [Math]::Max(1, [int]($Size * 0.047))
    $radius = [int]($Size * 0.20)
    $diameter = $radius * 2
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $path.AddArc($margin, $margin, $diameter, $diameter, 180, 90)
    $path.AddArc($Size - $margin - $diameter, $margin, $diameter, $diameter, 270, 90)
    $path.AddArc($Size - $margin - $diameter, $Size - $margin - $diameter, $diameter, $diameter, 0, 90)
    $path.AddArc($margin, $Size - $margin - $diameter, $diameter, $diameter, 90, 90)
    $path.CloseFigure()
    $brush = New-Object System.Drawing.SolidBrush $teal
    $graphics.FillPath($brush, $path)

    $k = New-Object System.Drawing.Drawing2D.GraphicsPath
    $points = [System.Drawing.PointF[]]@(
        [System.Drawing.PointF]::new($Size*0.25, $Size*0.22),
        [System.Drawing.PointF]::new($Size*0.39, $Size*0.22),
        [System.Drawing.PointF]::new($Size*0.39, $Size*0.44),
        [System.Drawing.PointF]::new($Size*0.61, $Size*0.22),
        [System.Drawing.PointF]::new($Size*0.77, $Size*0.22),
        [System.Drawing.PointF]::new($Size*0.53, $Size*0.48),
        [System.Drawing.PointF]::new($Size*0.79, $Size*0.78),
        [System.Drawing.PointF]::new($Size*0.61, $Size*0.78),
        [System.Drawing.PointF]::new($Size*0.39, $Size*0.52),
        [System.Drawing.PointF]::new($Size*0.39, $Size*0.78),
        [System.Drawing.PointF]::new($Size*0.25, $Size*0.78)
    )
    $k.AddPolygon($points)
    $whiteBrush = New-Object System.Drawing.SolidBrush $white
    $graphics.FillPath($whiteBrush, $k)

    $arrow = [System.Drawing.PointF[]]@(
        [System.Drawing.PointF]::new($Size*0.61, $Size*0.13),
        [System.Drawing.PointF]::new($Size*0.84, $Size*0.13),
        [System.Drawing.PointF]::new($Size*0.84, $Size*0.36),
        [System.Drawing.PointF]::new($Size*0.77, $Size*0.29),
        [System.Drawing.PointF]::new($Size*0.67, $Size*0.39),
        [System.Drawing.PointF]::new($Size*0.58, $Size*0.30),
        [System.Drawing.PointF]::new($Size*0.68, $Size*0.20)
    )
    $graphics.FillPolygon($whiteBrush, $arrow)

    $whiteBrush.Dispose()
    $brush.Dispose()
    $k.Dispose()
    $path.Dispose()
    $graphics.Dispose()
    return $bitmap
}

$brand = New-KRunBitmap 512
$brand.Save((Join-Path $OutputDirectory "krun.png"), [System.Drawing.Imaging.ImageFormat]::Png)
$brand.Dispose()

$sizes = @(16, 20, 24, 32, 40, 48, 64, 128, 256)
$images = New-Object System.Collections.Generic.List[byte[]]
foreach ($size in $sizes) {
    $bitmap = New-KRunBitmap $size
    $stream = New-Object System.IO.MemoryStream
    $bitmap.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
    $images.Add($stream.ToArray())
    $stream.Dispose()
    $bitmap.Dispose()
}

$iconPath = Join-Path $OutputDirectory "krun.ico"
$stream = [System.IO.File]::Create($iconPath)
$writer = New-Object System.IO.BinaryWriter $stream
$writer.Write([uint16]0)
$writer.Write([uint16]1)
$writer.Write([uint16]$sizes.Count)
$offset = 6 + (16 * $sizes.Count)
for ($index = 0; $index -lt $sizes.Count; $index++) {
    $size = $sizes[$index]
    $writer.Write([byte]($(if ($size -ge 256) { 0 } else { $size })))
    $writer.Write([byte]($(if ($size -ge 256) { 0 } else { $size })))
    $writer.Write([byte]0)
    $writer.Write([byte]0)
    $writer.Write([uint16]1)
    $writer.Write([uint16]32)
    $writer.Write([uint32]$images[$index].Length)
    $writer.Write([uint32]$offset)
    $offset += $images[$index].Length
}
foreach ($image in $images) {
    $writer.Write($image)
}
$writer.Dispose()
$stream.Dispose()

Write-Host "Generated KRun icon assets in $OutputDirectory"
