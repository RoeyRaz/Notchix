//! Optional Spotify Web API queue search. Media-session controls stay separate.
//! OAuth uses PKCE in the system browser; refresh tokens are protected by the
//! current Windows account via PowerShell's DPAPI-backed SecureString format.

use crate::store;
use serde_json::Value;
use std::process::{Command, Stdio};

pub const REDIRECT_URI: &str = "http://127.0.0.1:53682/callback";

#[derive(Clone, Debug)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub uri: String,
}

#[derive(Debug)]
pub enum Result {
    Connected(std::result::Result<(), String>),
    Search(std::result::Result<Vec<Track>, String>),
    Queued(std::result::Result<(), String>),
}

pub fn client_id() -> String {
    std::fs::read_to_string(store::data_dir().join("spotify_client_id.txt"))
        .unwrap_or_default()
        .trim()
        .to_string()
}

pub fn save_client_id(value: &str) -> std::io::Result<()> {
    std::fs::write(
        store::data_dir().join("spotify_client_id.txt"),
        value.trim(),
    )
}

pub fn connected() -> bool {
    store::data_dir().join("spotify_refresh.dpapi").is_file()
}

pub fn disconnect() {
    let _ = std::fs::remove_file(store::data_dir().join("spotify_refresh.dpapi"));
}

pub fn authorize(client_id: &str) -> std::result::Result<(), String> {
    if client_id.trim().is_empty() {
        return Err("Enter your Spotify Client ID first.".into());
    }
    let data = serde_json::json!({"client_id": client_id.trim()});
    let output = run_script(&format!(
        r#"
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$data = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{payload}')) | ConvertFrom-Json
$redirect = '{redirect}'
$listener = [System.Net.Sockets.TcpListener]::new([Net.IPAddress]::Parse('127.0.0.1'), 53682)
try {{
    $listener.Start()
    $rng = [Security.Cryptography.RandomNumberGenerator]::Create()
    $raw = New-Object byte[] 48
    $rng.GetBytes($raw)
    $verifier = [Convert]::ToBase64String($raw).TrimEnd('=').Replace('+','-').Replace('/','_')
    $rawState = New-Object byte[] 24
    $rng.GetBytes($rawState)
    $state = [Convert]::ToBase64String($rawState).TrimEnd('=').Replace('+','-').Replace('/','_')
    $sha = [Security.Cryptography.SHA256]::Create()
    $challenge = [Convert]::ToBase64String($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($verifier))).TrimEnd('=').Replace('+','-').Replace('/','_')
    $query = 'client_id=' + [Uri]::EscapeDataString([string]$data.client_id) + '&response_type=code&redirect_uri=' + [Uri]::EscapeDataString($redirect) + '&scope=' + [Uri]::EscapeDataString('user-modify-playback-state') + '&code_challenge_method=S256&code_challenge=' + $challenge + '&state=' + $state
    Start-Process ('https://accounts.spotify.com/authorize?' + $query)
    $pending = $listener.AcceptTcpClientAsync()
    if (-not $pending.Wait(180000)) {{ throw 'Spotify sign-in timed out. Try connecting again.' }}
    $client = $pending.Result
    try {{
        $stream = $client.GetStream()
        $reader = New-Object System.IO.StreamReader($stream, [Text.Encoding]::ASCII, $false, 1024, $true)
        $requestLine = $reader.ReadLine()
        while ($null -ne ($header = $reader.ReadLine()) -and $header -ne '') {{ }}
        if (-not $requestLine -or $requestLine -notmatch '^GET /callback\x3f([^ ]+) ') {{ throw 'Spotify returned an unexpected sign-in response.' }}
        $queryPart = $Matches[1]
        $params = @{{}}
        foreach ($part in $queryPart.Split('&')) {{ $pair = $part.Split('=',2); if ($pair.Length -eq 2) {{ $params[[Uri]::UnescapeDataString($pair[0])] = [Uri]::UnescapeDataString($pair[1].Replace('+',' ')) }} }}
        $ok = ($params.state -ceq $state) -and [bool]$params.code -and -not $params.error
        $heading = if ($ok) {{ 'Spotify connection complete' }} else {{ 'Spotify connection was not completed' }}
        $html = '<!doctype html><meta charset=utf-8><title>Notchix</title><body style="font:16px Segoe UI;background:#111;color:white;padding:40px"><h2>' + $heading + '</h2><p>You can close this tab and return to Notchix.</p></body>'
        $bytes = [Text.Encoding]::UTF8.GetBytes($html)
        $head = [Text.Encoding]::ASCII.GetBytes("HTTP/1.1 200 OK`r`nContent-Type: text/html; charset=utf-8`r`nConnection: close`r`nContent-Length: $($bytes.Length)`r`n`r`n")
        $stream.Write($head, 0, $head.Length); $stream.Write($bytes, 0, $bytes.Length); $stream.Flush()
        if ($params.error) {{ throw ('Spotify sign-in was declined or failed: ' + $params.error) }}
        if ($params.state -cne $state) {{ throw 'Spotify sign-in could not be verified. Try again.' }}
        if (-not $params.code) {{ throw 'Spotify did not return an authorization code.' }}
        $token = Invoke-RestMethod -Method Post -Uri 'https://accounts.spotify.com/api/token' -TimeoutSec 15 -ContentType 'application/x-www-form-urlencoded' -Body @{{ client_id=$data.client_id; grant_type='authorization_code'; code=$params.code; redirect_uri=$redirect; code_verifier=$verifier }}
        if (-not $token.refresh_token) {{ throw 'Spotify did not provide a refresh token. Disconnect and authorize again.' }}
        $secure = ConvertTo-SecureString ([string]$token.refresh_token) -AsPlainText -Force
        $tokenPath = Join-Path $env:APPDATA 'Nock\spotify_refresh.dpapi'
        Set-Content -LiteralPath $tokenPath -Value (ConvertFrom-SecureString $secure) -Encoding ASCII
        [Console]::Out.WriteLine('OK')
    }} finally {{ $client.Dispose() }}
}} catch {{ [Console]::Out.WriteLine([string]$_.Exception.Message); exit 1 }} finally {{ $listener.Stop() }}
"#,
        payload = ps_json(&data),
        redirect = REDIRECT_URI
    ))?;
    if output.trim() == "OK" {
        Ok(())
    } else {
        Err("Spotify returned an unexpected connection response.".into())
    }
}

pub fn search(client_id: &str, query: &str) -> std::result::Result<Vec<Track>, String> {
    let data = serde_json::json!({"client_id": client_id, "query": query});
    let output = run_script(&format!(
        r#"
$ErrorActionPreference = 'Stop'; $ProgressPreference = 'SilentlyContinue'
$data = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{payload}')) | ConvertFrom-Json
try {{
    $secure = ConvertTo-SecureString ((Get-Content -Raw -LiteralPath (Join-Path $env:APPDATA 'Nock\spotify_refresh.dpapi')).Trim())
    $ptr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
    try {{ $refresh = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($ptr) }} finally {{ [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($ptr) }}
    $token = Invoke-RestMethod -Method Post -Uri 'https://accounts.spotify.com/api/token' -TimeoutSec 15 -ContentType 'application/x-www-form-urlencoded' -Body @{{ client_id=$data.client_id; grant_type='refresh_token'; refresh_token=$refresh }}
    if ($token.refresh_token) {{ $secure = ConvertTo-SecureString ([string]$token.refresh_token) -AsPlainText -Force; Set-Content -LiteralPath (Join-Path $env:APPDATA 'Nock\spotify_refresh.dpapi') -Value (ConvertFrom-SecureString $secure) -Encoding ASCII }}
    $headers = @{{ Authorization = 'Bearer ' + $token.access_token }}
    $url = 'https://api.spotify.com/v1/search?q=' + [Uri]::EscapeDataString([string]$data.query) + '&type=track&limit=5'
    $response = Invoke-RestMethod -Method Get -Uri $url -Headers $headers -TimeoutSec 15
    $items = @($response.tracks.items | ForEach-Object {{ [pscustomobject]@{{ title=$_.name; artist=($_.artists.name -join ', '); album=$_.album.name; uri=$_.uri }} }})
    [Console]::Out.WriteLine((ConvertTo-Json -InputObject $items -Compress -Depth 4))
}} catch {{ [Console]::Out.WriteLine((ConvertTo-Json -Compress @{{error=$_.Exception.Message}})); exit 1 }}
"#,
        payload = ps_json(&data)
    ))?;
    let value: Value = serde_json::from_str(&output)
        .map_err(|_| "Spotify returned an unreadable search response.".to_string())?;
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        return Err(friendly_error(error));
    }
    let Some(items) = value.as_array() else {
        return Err("Spotify returned an unexpected search response.".into());
    };
    Ok(items
        .iter()
        .filter_map(|item| {
            Some(Track {
                title: item.get("title")?.as_str()?.to_string(),
                artist: item.get("artist")?.as_str()?.to_string(),
                album: item.get("album")?.as_str()?.to_string(),
                uri: item.get("uri")?.as_str()?.to_string(),
            })
        })
        .collect())
}

pub fn queue(client_id: &str, uri: &str) -> std::result::Result<(), String> {
    let data = serde_json::json!({"client_id": client_id, "uri": uri});
    let output = run_script(&format!(
        r#"
$ErrorActionPreference = 'Stop'; $ProgressPreference = 'SilentlyContinue'
$data = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{payload}')) | ConvertFrom-Json
try {{
    $secure = ConvertTo-SecureString ((Get-Content -Raw -LiteralPath (Join-Path $env:APPDATA 'Nock\spotify_refresh.dpapi')).Trim())
    $ptr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secure)
    try {{ $refresh = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($ptr) }} finally {{ [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($ptr) }}
    $token = Invoke-RestMethod -Method Post -Uri 'https://accounts.spotify.com/api/token' -TimeoutSec 15 -ContentType 'application/x-www-form-urlencoded' -Body @{{ client_id=$data.client_id; grant_type='refresh_token'; refresh_token=$refresh }}
    if ($token.refresh_token) {{ $secure = ConvertTo-SecureString ([string]$token.refresh_token) -AsPlainText -Force; Set-Content -LiteralPath (Join-Path $env:APPDATA 'Nock\spotify_refresh.dpapi') -Value (ConvertFrom-SecureString $secure) -Encoding ASCII }}
    $headers = @{{ Authorization = 'Bearer ' + $token.access_token }}
    $url = 'https://api.spotify.com/v1/me/player/queue?uri=' + [Uri]::EscapeDataString([string]$data.uri)
    Invoke-RestMethod -Method Post -Uri $url -Headers $headers -TimeoutSec 15 | Out-Null
    [Console]::Out.WriteLine('OK')
}} catch {{ [Console]::Out.WriteLine([string]$_.Exception.Message); exit 1 }}
"#,
        payload = ps_json(&data)
    ))?;
    if output.trim() == "OK" {
        Ok(())
    } else {
        Err(friendly_error(&output))
    }
}

fn friendly_error(message: &str) -> String {
    let m = message.to_ascii_lowercase();
    if m.contains("401") || m.contains("invalid_grant") {
        "Spotify connection expired. Reconnect your account.".into()
    } else if m.contains("403") {
        "Spotify could not queue this track. Check Premium access and the active device.".into()
    } else if m.contains("404") || m.contains("no active device") {
        "Open Spotify and start playback on an active device, then try again.".into()
    } else if m.contains("429") {
        "Spotify is receiving too many requests. Wait a moment and retry.".into()
    } else {
        format!(
            "Spotify request failed: {}",
            message.trim().chars().take(150).collect::<String>()
        )
    }
}

fn ps_json(value: &Value) -> String {
    base64(value.to_string().as_bytes())
}

fn run_script(script: &str) -> std::result::Result<String, String> {
    let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let encoded = base64(&utf16);
    let mut cmd = Command::new("powershell.exe");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-EncodedCommand",
        &encoded,
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let output = cmd
        .output()
        .map_err(|_| "Could not start the Spotify connection helper.".to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if output.status.success() {
        Ok(stdout)
    } else {
        // Errors from the provider are mapped without logging request headers or tokens.
        let detail = if stdout.is_empty() {
            "Spotify request failed."
        } else {
            &stdout
        };
        Err(friendly_error(detail))
    }
}

fn base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let a = chunk[0] as usize;
        let b = *chunk.get(1).unwrap_or(&0) as usize;
        let c = *chunk.get(2).unwrap_or(&0) as usize;
        out.push(TABLE[a >> 2] as char);
        out.push(TABLE[((a & 3) << 4) | (b >> 4)] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((b & 15) << 2) | (c >> 6)] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[c & 63] as char
        } else {
            '='
        });
    }
    out
}
