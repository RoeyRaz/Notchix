# Run from PowerShell on Windows 11. Builds the MSIX, verifies the developer
# certificate is already trusted by LocalMachine, then installs the package.
# Does not disable or change any Windows notification settings.
[CmdletBinding()]
param([switch]$SkipCargoBuild)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$buildScript = Join-Path $PSScriptRoot 'Build-NockMsix.ps1'

Write-Host 'Notchix: Windows notification integration' -ForegroundColor Cyan
Write-Host 'A portable nock.exe cannot obtain the required notification listener identity.'
Write-Host 'This script creates a local developer package and verifies its signing certificate.'
Write-Host 'Exit any running Nock from its tray icon first to avoid competing notches.'

& $buildScript -SkipCargoBuild:$SkipCargoBuild
if ($LASTEXITCODE -and $LASTEXITCODE -ne 0) { throw 'MSIX build failed.' }

$package = Get-ChildItem (Join-Path $root 'dist') -Filter 'Notchix-*-x64.msix' |
    Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1 -ExpandProperty FullName
$certificate = Join-Path $root 'dist\Nock-Custom-Developer.cer'
if (-not (Test-Path $package) -or -not (Test-Path $certificate)) {
    throw 'Expected signed MSIX or generated certificate was not found.'
}

Write-Host ''
Write-Host 'The following certificate was created by your local Nock build:' -ForegroundColor Yellow
Get-PfxCertificate -FilePath $certificate | Select-Object Subject, Thumbprint, NotAfter | Format-List
$thumbprint = (Get-PfxCertificate -FilePath $certificate).Thumbprint
$trusted = Get-Item ("Cert:\LocalMachine\TrustedPeople\" + $thumbprint) -ErrorAction SilentlyContinue
if (-not $trusted) {
    Write-Host 'MSIX requires Local Computer / Trusted People, not Current User / Trusted People.' -ForegroundColor Yellow
    Write-Host 'In an ADMINISTRATOR PowerShell, after verifying the thumbprint, run:'
    Write-Host "Import-Certificate -FilePath '$certificate' -CertStoreLocation 'Cert:\LocalMachine\TrustedPeople'"
    throw 'Certificate is not trusted by the computer. No certificate stores were changed.'
}

Add-AppxPackage -Path $package

# Preserve the user's existing Start with Windows choice. A portable path or
# versioned WindowsApps path would reopen an old build after the next restart.
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$existingRun = (Get-ItemProperty -Path $runKey -Name Nock -ErrorAction SilentlyContinue).Nock
if ($existingRun) {
    Set-ItemProperty -Path $runKey -Name Nock -Value 'cmd.exe /c start "" "shell:AppsFolder\Roey.NockCustom_pnky3m006n9h6!Nock"'
}
Write-Host ''
Write-Host 'Installed Notchix.' -ForegroundColor Green
Write-Host 'Launch Notchix from Start, open Activity, and select Allow access.'
Write-Host 'Leave Windows and WhatsApp notifications ON until a real message appears in Activity.'
Write-Host 'Do not launch target\release\nock.exe for notification integration.'
