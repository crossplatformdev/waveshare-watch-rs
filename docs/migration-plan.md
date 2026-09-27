# Firmware Migration Plan

## Baseline gate

Milestone 0 began from clean commit `5eb445bce1222a7ac9045dfb33231bc39b29abc0`. Source audit and baseline documentation are in progress. The release firmware build is currently **BLOCKED** because the repository-pinned `esp` Rust toolchain is not installed in the execution environment. No milestone may be marked complete and no later milestone may begin until the unmodified firmware builds successfully and the missing baseline measurements are explicitly recorded.

## Ordered milestones

1. **M0 — Baseline (current):** Record source architecture, hardware mapping, memory/power/build constraints, hazards, and a repeatable size-report tool. Re-run the release build and record actual ELF/binary/map measurements when the required toolchain is available. **Not green yet.**
2. **M1 — Code hygiene:** Address identified error propagation, `unsafe`, polling, and ignored errors in small behavior-preserving changes, validating each unit.
3. **M2 — BSP and drivers:** Centralize the verified board description and isolate testable device drivers.
4. **M3 — Services and IPC:** Introduce system-owned peripheral services and bounded, observable communication.
5. **M4 — Dual-core runtime:** Establish an RTOS/Embassy-based core policy and measure scheduling/latency; do not build a custom scheduler.
6. **M5 — Power and resource leases:** Add explicit power/resource policy, then validate real wake sources and current draw on hardware.
7. **M6 — UI compositor:** Move toward invalidation-driven composition and measured partial updates without frame-time allocation.
8. **M7 — Navigation and input:** Centralize gestures, application navigation, BOOT-as-Back and PWR policy.
9. **M8 — Transactional storage:** Introduce power-fail-safe settings/app data with fault-injection tests.
10. **M9 — Stable App API:** Define a versioned API and lifecycle independent of hardware implementation.
11. **M10 — Wasm/WASMI spike:** Benchmark compatibility, memory/fuel, startup, and UI/host-call cost on Xtensa. Stop at the end of this milestone and decide whether Wasm is viable before integration.
12. **M11 — App runtime and sandbox:** Only proceed after the M10 decision; enforce resource and capability limits.
13. **M12 — SDK and watchctl:** Build authoring/package tooling against the stable API.
14. **M13 — System apps:** Migrate the core system UI and management applications.
15. **M14 — Hardware apps:** Add sensor, audio, storage, and connectivity apps via services.
16. **M15 — Games and watchfaces:** Port and validate apps against the SDK without direct hardware access.
17. **M16 — Security and OTA:** Add verified firmware/app update, signing, rollback, and secret-storage paths.
18. **M17 — Commercial QA:** Complete repeatable HIL, performance, power, security, recovery, and product release testing.

## Change discipline and gates

- Keep the current `no_std` firmware buildable after every meaningful change.
- Do not create SDK/runtime crates before their milestone; do not begin M1 while M0's firmware build is blocked.
- Preserve Cargo.lock and upgrade dependency families independently.
- Keep measurements separate from source-derived estimates and comments.
- Require host tests for pure parsing/policy logic and HIL evidence for electrical, timing, wake, coexistence, and power claims.
- Record pass/fail/blocked/not-run outcomes without inferring success from an unavailable tool or artifact.
