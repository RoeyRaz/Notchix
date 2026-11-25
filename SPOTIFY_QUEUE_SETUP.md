# Spotify queue setup

The quick queue uses Spotify's supported Web API. The existing media controls continue to use Windows' media session and do not need this connection.

## Connect Notchix

1. Create a Spotify app in the [Spotify Developer Dashboard](https://developer.spotify.com/dashboard).
2. In the app settings, add this exact redirect URI:

   `http://127.0.0.1:53682/callback`

3. Copy the app's **Client ID**. Do not create or paste a Client Secret; Notchix uses Spotify's PKCE flow.
4. Open Notchix → Home → **Quick Spotify queue**. Enter the Client ID, press Enter, then choose **Connect Spotify** and approve the request in your browser.
5. Search by track or artist, then select **+ Queue** beside the song you want.

Notchix requests only `user-modify-playback-state`. It stores the refresh token protected by the current Windows account in `%APPDATA%\Nock\spotify_refresh.dpapi`; **Remove local connection** deletes that file. To revoke authorization at Spotify too, remove Notchix from your Spotify account's connected apps.

Queueing requires Spotify Premium and an active Spotify playback device. If Spotify rejects a request, Notchix shows the provider error instead of claiming that the song was queued. See Spotify's [Add Item to Playback Queue](https://developer.spotify.com/documentation/web-api/reference/add-to-queue) and [PKCE authorization](https://developer.spotify.com/documentation/web-api/tutorials/code-pkce-flow) documentation.
