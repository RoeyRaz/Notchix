# Run in a regular PowerShell window after installing the Windows SDK and Rust.
# Produces a locally signed MSIX and a certificate, but does NOT trust or install it.
[CmdletBinding()]
param([switch]$SkipCargoBuild)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$exe = Join-Path $root 'target\release\nock.exe'
if (-not $SkipCargoBuild) {
    Push-Location $root
    try { & cargo build --release; if ($LASTEXITCODE -ne 0) { throw 'Cargo build failed.' } }
    finally { Pop-Location }
}
if (-not (Test-Path $exe)) { throw "Missing binary: $exe" }
$sdkBin = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
$makeappx = Get-ChildItem $sdkBin -Filter makeappx.exe -Recurse -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\x64\\' } | Sort-Object FullName -Descending | Select-Object -First 1
$signtool = Get-ChildItem $sdkBin -Filter signtool.exe -Recurse -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\x64\\' } | Sort-Object FullName -Descending | Select-Object -First 1
if (-not $makeappx -or -not $signtool) { throw 'Windows SDK MakeAppx.exe and SignTool.exe (x64) are required.' }
$output = Join-Path $root 'dist'
$stage = Join-Path $output 'msix-stage'
New-Item -ItemType Directory -Force $stage, (Join-Path $stage 'Assets') | Out-Null
Copy-Item $exe (Join-Path $stage 'nock.exe') -Force
Copy-Item (Join-Path $PSScriptRoot 'AppxManifest.xml') (Join-Path $stage 'AppxManifest.xml') -Force
# Every installed update needs a strictly higher MSIX version. Derive the
# next revision automatically without manual version synchronization.
[xml]$manifest = Get-Content (Join-Path $stage 'AppxManifest.xml') -Raw
$version = [version]$manifest.Package.Identity.Version
$installed = Get-AppxPackage -Name 'Notchix.Desktop' -ErrorAction SilentlyContinue | Select-Object -First 1
if ($installed -and $installed.Version -ge $version) {
    $old = [version]$installed.Version
    if ($old.Revision -ge 65535) { throw 'MSIX revision exhausted. Increase the base version in AppxManifest.xml.' }
    $version = [version]::new($old.Major, $old.Minor, $old.Build, $old.Revision + 1)
}
$manifest.Package.Identity.Version = $version.ToString(4)
$manifest.Save((Join-Path $stage 'AppxManifest.xml'))
Add-Type -AssemblyName System.Drawing
$source = [System.Drawing.Image]::FromFile((Join-Path $root 'assets\notchix_icon256.png'))
try {
    foreach ($entry in @(@('StoreLogo.png', 50), @('Square150x150Logo.png', 150), @('Square44x44Logo.png', 44))) {
        $dimension = [int]$entry[1]
        $bitmap = New-Object System.Drawing.Bitmap($dimension, $dimension)
        try {
            $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
            try {
                $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                $graphics.DrawImage($source, 0, 0, $dimension, $dimension)
            } finally { $graphics.Dispose() }
            $bitmap.Save((Join-Path $stage "Assets\$($entry[0])"), [System.Drawing.Imaging.ImageFormat]::Png)
        } finally { $bitmap.Dispose() }
    }
} finally { $source.Dispose() }
$package = Join-Path $output "Notchix-$($version.ToString(4))-x64.msix"
& $makeappx.FullName pack /d $stage /p $package /o
if ($LASTEXITCODE -ne 0) { throw 'MSIX packaging failed.' }
$publisher = 'CN=Notchix Developer'
$cert = Get-ChildItem Cert:\CurrentUser\My |
    Where-Object { $_.Subject -eq $publisher -and $_.HasPrivateKey -and $_.NotAfter -gt (Get-Date) } |
    Sort-Object NotAfter -Descending | Select-Object -First 1
if (-not $cert) {
    $cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject $publisher -CertStoreLocation Cert:\CurrentUser\My -KeyExportPolicy NonExportable
}
$cerPath = Join-Path $output 'Notchix-Developer.cer'
Export-Certificate -Cert $cert -FilePath $cerPath -Force | Out-Null
& $signtool.FullName sign /fd SHA256 /sha1 $cert.Thumbprint /s My $package
if ($LASTEXITCODE -ne 0) { throw 'MSIX signing failed.' }
Write-Host "Built: $package"
Write-Host "Certificate: $cerPath"
Write-Host 'Next: inspect and trust your local certificate, then install the MSIX. See NOTCHIX_BUILD.md.'
