# AudioDeviceCmdlets: https://github.com/frgnca/AudioDeviceCmdlets
# Set these to the exact two Windows playback device names you want to alternate.
$PreferredOutputs = @(
    'Headset Earphone (G435 Wireless Gaming Headset)',
    'Headphones (E-3511)'
)
$ErrorActionPreference = 'Stop'
try {
    $choicesFile = Join-Path $PSScriptRoot 'audio-outputs.txt'
    if (Test-Path $choicesFile) {
        $saved = @(Get-Content -LiteralPath $choicesFile -Encoding UTF8 | Where-Object { $_.Trim() })
        if ($saved.Count -eq 2) { $PreferredOutputs = $saved }
    }
    Import-Module AudioDeviceCmdlets -ErrorAction Stop
    $available = @(Get-AudioDevice -List | Where-Object { $_.Type -eq 'Playback' })
    $first = $available | Where-Object { $_.Name -eq $PreferredOutputs[0] } | Select-Object -First 1
    $second = $available | Where-Object { $_.Name -eq $PreferredOutputs[1] } | Select-Object -First 1
    if (-not $first -or -not $second) { throw 'Both preferred playback devices must be connected. Check their names in Toggle-Audio.ps1.' }
    $current = Get-AudioDevice -Playback
    $target = if ($current.ID -eq $first.ID) { $second } else { $first }
    Set-AudioDevice -ID $target.ID | Out-Null
} catch {
    Add-Type -AssemblyName PresentationFramework
    [System.Windows.MessageBox]::Show($_.Exception.Message, 'Nock Audio Switch') | Out-Null
    exit 1
}
