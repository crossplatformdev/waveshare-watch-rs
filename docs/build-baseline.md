# Build and Test Baseline

## Checkout

- Repository: `crossplatformdev/waveshare-watch-rs`
- Baseline commit: `5eb445bce1222a7ac9045dfb33231bc39b29abc0`
- Initial worktree: clean (`git status --short` produced no entries)
- Package: `waveshare-watch-rs` 0.4.0, edition 2021; single package, not a workspace
- Target configured by `.cargo/config.toml`: `xtensa-esp32s3-none-elf`
- Pinned toolchain in `rust-toolchain.toml`: custom channel `esp`

## Validation outcome

| Command/check | Result | Detail |
|---|---|---|
| `cargo build --release` | **BLOCKED / NOT BUILT** | Rustup reports `custom toolchain 'esp' ... is not installed`. No firmware artifact was generated. |
| `cargo build --workspace --release` | **NOT RUN** | The target build is blocked by the same missing pinned toolchain; this repository has no workspace members. |
| Host tests (`cargo test --workspace --target x86_64-unknown-linux-gnu`) | **BLOCKED / TESTS NOT RUN** | The attempt using stable failed while compiling dependency `esp-sync`: `#![feature(asm_experimental_arch)]` is rejected on stable and `xtensa_lx` is unavailable for the host target. No project tests ran; no `#[test]` or `#[cfg(test)]` items were found in `src/`. |
| `cargo fmt --all -- --check` | **BLOCKED with pinned toolchain** | `esp` toolchain is not installed. A diagnostic run through the installed stable toolchain reports existing formatting differences across source files; source files were not reformatted in this baseline-only task. |
| `cargo clippy --workspace --all-targets -- -D warnings` | **NOT RUN** | The pinned toolchain is missing; no embedded clippy result is available. |
| Firmware size/link map | **NOT AVAILABLE** | No successful target build; see [memory-baseline.md](memory-baseline.md). |

The custom toolchain blocker is environmental: only `stable-x86_64-unknown-linux-gnu` was installed, and no `espup` or Xtensa target utilities were present. The Rust toolchain is pinned intentionally by the repository. The host-test attempt additionally confirms this target-specific dependency graph cannot be tested with the installed stable host toolchain. Do not substitute a stable host build for embedded target validation.

## Test inventory

No Rust unit/integration test attributes or test directory were found in the checked-in source inventory. CI workflow inventory contains only a wiki-sync workflow; there is no firmware build/test workflow in the checked-in `.github/workflows`.

## Reproduction

Run the release build with the repository's `esp` toolchain provisioned, Xtensa target support and linker installed, and the corresponding ESP environment set up. Record the exact toolchain version, Cargo.lock hash, build output and generated artifact sizes in subsequent baseline updates.
