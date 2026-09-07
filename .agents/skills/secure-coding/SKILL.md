---
name: secure-coding
description: Enforces secure coding practices for Rust applications, particularly focusing on OAuth token handling, secure storage, and memory safety.
---

# Secure Coding Guidelines

When writing or modifying code in this project, you must act as a Secure Coding Expert and adhere to the following principles:

## 1. Secrets Management & Token Handling
- **Never Log Secrets**: Ensure that `access_token`, `refresh_token`, and user credentials are NEVER sent to `logger::log_info` or `logger::log_error`.
- **Secure Storage**: Rely on the `keyring` crate to store secrets securely in the OS's native credential manager. Never write tokens to plain text files (e.g., `config.toml` should only hold non-sensitive data like `client_id` and `tenant_id`).
- **Memory Wiping**: When dealing with highly sensitive plaintext cryptographic keys (if applicable), consider using crates like `zeroize` to wipe memory after use.

## 2. Rust Safety Guarantees
- **No Unsafe**: Do not use the `unsafe` keyword unless absolutely necessary for FFI, and even then, isolate it and heavily document the safety invariants.
- **Panic Prevention**: Avoid `.unwrap()` and `.expect()` in production code. Always use proper `Result` and `Option` matching or the `?` operator. A TUI should gracefully display errors to the user rather than crashing the terminal.

## 3. Network Security
- **Strict HTTPS**: All interactions with the Microsoft Graph API must use `https://`. 
- **TLS Backend**: We explicitly use `rustls-tls` in our `reqwest` configuration to ensure a secure, memory-safe, and statically linked TLS stack. Do not revert to `native-tls` without a valid security justification.
