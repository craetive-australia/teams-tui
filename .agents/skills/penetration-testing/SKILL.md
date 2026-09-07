---
name: penetration-testing
description: Provides methodologies for dynamically testing the application for vulnerabilities, including fuzzing and API manipulation.
---

# Penetration Testing Methodology

Use this skill when performing dynamic analysis, fuzzing, or active exploitation attempts against the codebase in a safe, controlled environment.

## 1. Fuzzing the UI and Parsers
- **Objective**: Discover Denial of Service (DoS) or memory corruption bugs in the JSON parsers or UI rendering engine.
- **Method**: Instruct the user to use `cargo fuzz`. We can write fuzz targets that feed mutated JSON payloads (mimicking Microsoft Graph API responses) into our `serde_json` structs to see if we can trigger an Out-Of-Memory (OOM) or panic.

## 2. API Interception & Manipulation
- **Objective**: Test how the application handles malformed or malicious network responses.
- **Method**: Set up a local proxy (like Burp Suite, mitmproxy, or a mock HTTP server) and point `GraphClient`'s base URL to it. Send `429 Too Many Requests`, `500 Internal Server Error`, or heavily nested JSON arrays to ensure the TUI does not freeze or crash.

## 3. Local Environment Manipulation
- **Objective**: Test how the app handles hostile OS environments.
- **Method**: 
  - Corrupt the `config.toml` file to see if the app panics or recovers gracefully.
  - Delete the token from the OS keyring while the app is running to ensure it handles auth expiration without crashing.

## Actionable Steps for Agents
If asked to "pentest" the app, write a test script or fuzzing harness that implements one of the methods above, execute it, and record the application's behavior.
