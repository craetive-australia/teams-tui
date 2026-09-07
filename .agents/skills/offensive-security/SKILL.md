---
name: offensive-security
description: Guides agents in applying a threat-modeling mindset to identify architectural flaws, privilege escalation vectors, and logic bugs.
---

# Offensive Security & Threat Modeling

When tasked with an offensive security review, you must adopt the mindset of an attacker trying to exploit the application. 

## Key Attack Vectors in Teams TUI

### 1. OAuth Scope Abuse
- **Review**: Check `AUTHENTICATION.md` and `src/auth.rs`. Are we requesting scopes we don't need? 
- **Exploit Scenario**: If an attacker compromises the OS credential manager and steals the `access_token`, what is the maximum damage they can do? Ensure the app operates on the principle of least privilege.

### 2. TUI Injection & Escape Sequences
- **Review**: The application renders text received from the Microsoft Graph API (e.g., chat messages, user names). 
- **Exploit Scenario**: Can an attacker send a malicious chat message containing ANSI escape sequences or control characters that manipulate the user's terminal, hide malicious text, or execute commands? Ensure `ratatui` properly sanitizes raw strings.

### 3. File System Permissions
- **Review**: Where does `config.toml` and `teams-tui.log` live?
- **Exploit Scenario**: If the log file contains sensitive data and is created with `0644` (world-readable) permissions on a shared Linux machine, local privilege escalation or data theft could occur.

## Actionable Steps for Agents
When performing an offensive security review, document a "Threat Model" artifact. List the Trust Boundaries, Threat Actors, and propose mitigations for the attack vectors above.
