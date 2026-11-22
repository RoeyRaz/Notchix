$ErrorActionPreference = 'Stop'
$dir = Join-Path $env:APPDATA 'Nock'
$taskPath = Join-Path $dir 'notion-task-{PID}.json'
try {
    $cfg = Get-Content -LiteralPath (Join-Path $dir 'notion-config.json') -Raw | ConvertFrom-Json
    $secure = (Get-Content -LiteralPath (Join-Path $dir 'notion-token.dpapi') -Raw).Trim() | ConvertTo-SecureString
    $credential = New-Object System.Management.Automation.PSCredential('notion', $secure)
    $token = $credential.GetNetworkCredential().Password
    $task = Get-Content -LiteralPath $taskPath -Raw -Encoding UTF8 | ConvertFrom-Json
    $props = @{}
    $props[[string]$cfg.title_property] = @{ title = @(@{ text = @{ content = [string]$task.title } }) }
    $body = @{ parent = @{ type = 'data_source_id'; data_source_id = [string]$cfg.data_source_id }; properties = $props } | ConvertTo-Json -Depth 10 -Compress
    $headers = @{ Authorization = "Bearer $token"; 'Notion-Version' = '2025-09-03' }
    $result = Invoke-RestMethod -Method Post -Uri 'https://api.notion.com/v1/pages' -Headers $headers -ContentType 'application/json' -Body ([Text.Encoding]::UTF8.GetBytes($body)) -TimeoutSec 20
    if ($result.id) { 'Created task in Notion' } else { 'Notion did not confirm creation' }
} catch {
    'Notion task was not created. Check the connection, database sharing, and configuration.'
} finally {
    Remove-Item -LiteralPath $taskPath -ErrorAction SilentlyContinue
}
