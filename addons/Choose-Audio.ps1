# Choose the two playback devices used by Notchix Audio Switch.
$ErrorActionPreference = 'Stop'
try {
    Import-Module AudioDeviceCmdlets -ErrorAction Stop
    Add-Type -AssemblyName System.Windows.Forms
    Add-Type -AssemblyName System.Drawing
    $outputs = @(Get-AudioDevice -List | Where-Object { $_.Type -eq 'Playback' } | Sort-Object Name)
    if ($outputs.Count -lt 2) { throw 'Connect at least two playback devices first.' }

    $form = New-Object System.Windows.Forms.Form
    $form.Text = 'Notchix | Select two audio outputs'
    $form.Size = New-Object System.Drawing.Size(535,410)
    $form.StartPosition = 'CenterScreen'
    $form.FormBorderStyle = 'FixedDialog'
    $form.MaximizeBox = $false
    $form.TopMost = $true
    $label = New-Object System.Windows.Forms.Label
    $label.Text = 'Select exactly two playback devices for the notch toggle.'
    $label.SetBounds(16,14,495,34)
    $form.Controls.Add($label)
    $list = New-Object System.Windows.Forms.CheckedListBox
    $list.CheckOnClick = $true
    $list.SetBounds(16,52,488,260)
    $list.HorizontalScrollbar = $true
    foreach ($device in $outputs) { [void]$list.Items.Add($device.Name) }
    $choicesFile = Join-Path $PSScriptRoot 'audio-outputs.txt'
    if (Test-Path $choicesFile) {
        $saved = @(Get-Content -LiteralPath $choicesFile -Encoding UTF8 | Where-Object { $_.Trim() })
    } else {
        $saved = @()
    }
    for ($i = 0; $i -lt $list.Items.Count; $i++) {
        if ($saved -contains [string]$list.Items[$i]) { $list.SetItemChecked($i,$true) }
    }
    $form.Controls.Add($list)
    $save = New-Object System.Windows.Forms.Button
    $save.Text = 'Save selected outputs'
    $save.SetBounds(322,322,182,32)
    $save.Add_Click({
        if ($list.CheckedItems.Count -ne 2) {
            [System.Windows.Forms.MessageBox]::Show('Please select exactly two devices.') | Out-Null
            return
        }
        @($list.CheckedItems | ForEach-Object { [string]$_ }) | Set-Content -LiteralPath $choicesFile -Encoding UTF8
        $form.DialogResult = [System.Windows.Forms.DialogResult]::OK
        $form.Close()
    })
    $form.Controls.Add($save)
    [void]$form.ShowDialog()
} catch {
    Add-Type -AssemblyName PresentationFramework
    [System.Windows.MessageBox]::Show($_.Exception.Message, 'Notchix audio settings') | Out-Null
    exit 1
}
