# Teams TUI

A blazing fast, keyboard-centric Terminal User Interface for Microsoft Teams, written in Rust.

`teams-tui` allows you to chat, view channels, and search your organization's directory entirely from your terminal without opening the heavy Electron/WebView client.

## Features
* 🚀 **Instant Launch:** Starts up in ~1 second.
* ⌨️ **Vim-like Keybindings:** Navigate chats and channels without taking your hands off the keyboard.
* 🔍 **Fuzzy Directory Search:** Find and chat with coworkers instantly.
* 💬 **Real-time Status:** View presence indicators (Available, Busy, Away).
* 🔒 **Secure:** Uses native OS credential managers to securely store OAuth tokens. No passwords are ever seen or stored.
* 📦 **Cross-Platform:** Available for Windows, macOS, and Linux.

## Installation

### Cargo (Cross-Platform)
If you have the Rust toolchain installed:
```bash
cargo install teams-tui
```

### Windows (WinGet)
```powershell
winget install teams-tui
```

### Pre-compiled Binaries
You can download the latest binaries for Windows, macOS, and Linux from the [Releases](https://github.com/andre/teams-tui/releases) page.

## Getting Started

1. Open your terminal and run `teams-tui`.
2. On your first launch, the app will prompt you to authenticate via the Device Code flow.
3. A URL and a code will be displayed. Open the URL in your browser, enter the code, and log in with your Microsoft 365 account.
4. Once authenticated, `teams-tui` will fetch your chats and teams, and you're ready to go!

If you encounter an "Admin Consent Required" error during login, please refer to the [Authentication Guide](docs/AUTHENTICATION.md) to set up a private Client ID for your tenant.

## Keybindings

| Key(s) | Action |
|---|---|
| `j` / `k` / `Down` / `Up` | Navigate lists (Chats, Channels, Messages) |
| `h` / `l` / `Left` / `Right` | Switch between Panes |
| `Enter` | Select Chat/Channel or Send Message |
| `i` | Focus message input box |
| `Esc` | Unfocus input box or close modal |
| `/` | Open Search (Find User/Chat) |
| `q` | Quit Application |

## Configuration

`teams-tui` can be configured via a `config.toml` file located at:
* **Windows**: `%APPDATA%\microsoft\teams-tui\config\config.toml`
* **macOS/Linux**: `~/.config/teams-tui/config.toml`

Available options (with defaults):
```toml
# The Microsoft Graph API Client ID
client_id = "14d82eec-204b-4a2f-88e6-54b83fb904e6"

# Polling intervals in seconds
active_chat_poll_interval = 5
active_channel_poll_interval = 15

# Enable debug logging (saved to teams-tui.log)
debug_logging = false
```

## Privacy & Telemetry
`teams-tui` connects directly to the Microsoft Graph API. **No data is ever sent to any third-party servers**. Your chats, tokens, and directory information remain strictly between your local machine and Microsoft.

## License
MIT
