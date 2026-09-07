# Authentication Guide

`teams-tui` uses the Microsoft Graph API to access your chats, teams, and directory information. Because it is a third-party application, it must authenticate against Microsoft Entra ID (Azure AD).

## How it works

The application uses the **OAuth 2.0 Device Authorization Grant** (Device Code Flow). 
1. When you start the application for the first time, it will present a code and a URL (`https://microsoft.com/devicelogin`).
2. You navigate to the URL in your browser, enter the code, and log in with your Microsoft 365 / Teams credentials.
3. Once successful, the application securely stores a refresh token in your system's native credential manager (e.g., Windows Credential Manager, macOS Keychain, or Linux Secret Service) so you don't have to log in every time.

## Bring Your Own Client ID (For IT-Restricted Environments)

If your organization heavily restricts third-party applications, you may see an "Admin Consent Required" or "Application Blocked" error during login. 

To bypass this, you can create your own private Azure App Registration in your tenant and configure `teams-tui` to use it.

### Step 1: Create an App Registration
1. Go to the [Microsoft Entra ID admin center](https://entra.microsoft.com/#view/Microsoft_AAD_IAM/ActiveDirectoryMenuBlade/~/RegisteredApps) -> **App registrations**.
2. Click **New registration**.
3. Name it `teams-tui-local`.
4. Under **Supported account types**, choose **Accounts in this organizational directory only**.
5. Click **Register**.

### Step 2: Configure Authentication
1. Go to **Authentication** on the left menu.
2. Under **Advanced settings**, set **Allow public client flows** to **Yes**.
3. Click **Save**.

### Step 3: Add API Permissions
1. Go to **API permissions** on the left menu.
2. Click **Add a permission** -> **Microsoft Graph** -> **Delegated permissions**.
3. Add the following permissions:
   * `Channel.ReadBasic.All`
   * `ChannelMessage.Read.All`
   * `ChannelMessage.Send`
   * `Chat.ReadWrite`
   * `ChatMessage.Read`
   * `ChatMessage.Send`
   * `offline_access`
   * `People.Read`
   * `Presence.Read`
   * `Presence.ReadWrite`
   * `Team.ReadBasic.All`
   * `User.Read`
   * `User.ReadBasic.All`
4. Click **Add permissions**.
5. (Optional but recommended) Click **Grant admin consent for [Your Tenant]** if you have the rights to do so.

### Step 4: Configure `teams-tui`
1. On the **Overview** page of your App Registration, copy the **Application (client) ID**.
2. Open your `teams-tui` configuration file located at:
   * **Windows**: `%APPDATA%\microsoft\teams-tui\config\config.toml`
   * **macOS/Linux**: `~/.config/teams-tui/config.toml`
3. Add or update the `client_id` property:

```toml
client_id = "YOUR_NEW_CLIENT_ID"
```

Restart `teams-tui`, and you will authenticate using your own tenant's App Registration.
