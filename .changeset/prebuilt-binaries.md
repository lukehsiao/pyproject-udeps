---
"pyproject-udeps": minor
---

**feat**: publish prebuilt binaries on GitHub releases.

Releases now attach binaries for ten targets (Linux gnu/musl on x86_64, aarch64, and armv7; macOS x86_64 and aarch64; Windows x86_64 and aarch64) with sha256 checksums, so `cargo binstall pyproject-udeps` works and CI can install via `taiki-e/install-action` without a compile.
