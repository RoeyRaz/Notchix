# qBittorrent live progress setup

Nock can show *real* torrent progress with qBittorrent 5.2.0 or later.
Without setup, the previous activity indicator remains available.

1. In qBittorrent, open **Tools > Options > Web UI**.
2. Enable **Web User Interface (Remote control)**, keep the listening port at
   `8080` (or another local port), and bind to `127.0.0.1` if that option is
   available. Do **not** expose WebUI to the internet or forward its port.
3. In the **API Key** section, generate an API key. Do not share it in chat.
4. On your Windows PC, set a per-user environment variable called
   `NOCK_QBIT_API_KEY` to your generated key. Search Windows for **Edit
   environment variables for your account**, select **New** under User
   variables, and paste the key there. You can remove this variable any time.
5. If you chose another WebUI port, also set `NOCK_QBIT_PORT` to its number.
6. Fully exit and reopen Nock so it can read the new variables. You may have
   to sign out of Windows once if Nock is launched by an older startup process.

**Important:** Windows user environment variables are not encrypted secret
storage. The API key should be treated as a credential, only used with the
local-only WebUI, and rotated if exposed. Nock never sends it off your PC.

qBittorrent versions below 5.2 lack API-key authentication. They continue to
use the existing disk-activity indicator until a separate authenticated
connector is implemented. Never disable WebUI authentication to work around
this version restriction.

This first release shows the fastest active torrent. If multiple torrents
download simultaneously, their aggregate speed is not shown. ETA is qBittorrent's
own estimate and can be unavailable.
