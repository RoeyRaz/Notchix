# Run interactively as the same Windows user that runs Notchix.
$ErrorActionPreference = 'Stop'
$dir = Join-Path $env:APPDATA 'Nock'
New-Item -ItemType Directory -Path $dir -Force | Out-Null
Write-Host 'Create a Notion internal connection, share your task database with it, and copy its data source ID.'
$source = (Read-Host 'Notion data source ID').Trim()
if ($source -notmatch '^[0-9a-fA-F-]{32,36}$') { throw 'Invalid data source ID.' }
$property = (Read-Host 'Title property name (usually Name)').Trim()
if (-not $property) { $property = 'Name' }
$token = Read-Host 'Notion integration token' -AsSecureString
if (-not $token) { throw 'Token required.' }
@{ data_source_id = $source; title_property = $property } | ConvertTo-Json -Compress | Set-Content -LiteralPath (Join-Path $dir 'notion-config.json') -Encoding UTF8
$token | ConvertFrom-SecureString | Set-Content -LiteralPath (Join-Path $dir 'notion-token.dpapi') -Encoding UTF8
Write-Host 'Notion configured for this Windows user. The token is encrypted with Windows DPAPI.'
