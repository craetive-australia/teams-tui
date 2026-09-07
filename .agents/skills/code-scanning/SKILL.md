---
name: code-scanning
description: Provides commands and workflows for running static application security testing (SAST), dependency auditing, and linting on the Rust codebase.
---

# Code Scanning Workflow

Use this skill when asked to scan the codebase for vulnerabilities or when verifying the security posture of a pull request.

## 1. Dependency Auditing (`cargo audit`)
Rust projects can suffer from supply-chain vulnerabilities. 
- **Command**: Run `cargo audit` in the terminal.
- **Action**: If vulnerabilities are found, update the affected dependencies in `Cargo.toml` or run `cargo update -p <crate_name>`.

## 2. Advanced Linting (`cargo clippy`)
Clippy can catch numerous bugs that lead to security vulnerabilities (e.g., integer overflows, out-of-bounds access, panic-inducing unwraps).
- **Command**: Run `cargo clippy -- -W clippy::pedantic -W clippy::unwrap_used -W clippy::expect_used`
- **Action**: Fix any warnings related to potential panics, type conversions, or memory safety.

## 3. Supply Chain Security (`cargo deny`)
If `cargo deny` is installed, use it to check for unmaintained crates, license compliance, and multiple versions of the same crate.
- **Command**: `cargo deny check advisories`

## Actionable Steps for Agents
If you are instructed to "scan the code", you should immediately run `cargo audit` and `cargo clippy`, parse the output, and propose a remediation plan for any medium or high-severity findings.
