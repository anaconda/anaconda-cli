# Contributing to ana

Thank you for your interest in contributing to ana-cli. This document covers the basics of how to submit changes.

## Getting Started

ana-cli is written in Rust. To build from source, you need a recent stable Rust toolchain. Clone the repo, then:

```bash
cargo build
cargo test
```

## Reporting Issues

Open a GitHub Issue for bug reports and feature requests. Include enough detail to reproduce the problem: OS, platform, ana version (`ana --version`), and the command that triggered the issue.

## Submitting Changes

1. Open a GitHub Issue before starting work. Describe the bug or proposed change and wait for confirmation that the contribution is welcome.
2. Fork the repository and create a branch from `main`.
3. Reference the issue in your PR description (e.g., `Closes #123`).
4. Keep pull requests focused. One logical change per PR.
5. Include tests for new functionality or bug fixes.
6. Ensure `cargo test` passes before opening the PR.
7. Write a clear PR description that explains *what* changed and *why*.

## Developer Certificate of Origin

All contributions require a DCO sign-off. This certifies that you wrote or have the right to submit the code under the project's open source license. Add the sign-off to every commit:

```bash
git commit -s -m "Your commit message"
```

This appends a `Signed-off-by` line with your name and email. Use your real name, not a pseudonym. If you forget, you can amend:

```bash
git commit --amend -s
```

The full text of the DCO is at [developercertificate.org](https://developercertificate.org/).

## Trademark Policy

"ana" and Anaconda marks are restricted to official distributions. See the project LICENSE file for details.

## Code of Conduct

This project follows the [Code of Conduct](CODE_OF_CONDUCT.md). Please read it before participating.

## Questions

If something in this guide is unclear, open an issue. We'll improve the docs.
