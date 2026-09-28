# Firmware Migration Plan

## Baseline gate

Milestone 0 began from clean commit `5eb445bce1222a7ac9045dfb33231bc39b29abc0`. The unmodified firmware now builds successfully with the ESP32-S3 Xtensa toolchain, and architecture, hardware, size, and power baselines are documented. ELF sections and the ESP-IDF application-image length are recorded; a complete flash image, linker map, runtime heap/stack/PSRAM telemetry, physical current measurements, and board-level verification remain **NOT MEASURED / NOT RUN** because they require more build artifacts, instrumentation, or hardware. M0's target-build gate is **PASS**. Subsequent commits on this branch completed targeted M1 hygiene fixes, started M2 with board-description centralization in the driver layer, started M3 with main-owned SmartHome HTTP dispatch, started M4 with an explicit dual-core executor policy plus wake telemetry, started M5 by making wireless idle timeouts and IMU-use rules explicit runtime policy, started M6 by routing watchface incremental updates through existing dirty-region flush support, started M7 by moving launch/back decisions behind named navigation-policy helpers, started M8 with a two-slot transactional settings record plus recovery/fault tests, started M9 with a shared versioned app manifest and foreground lifecycle contract, started M10 with a shared Wasm payload plus host and feature-gated Xtensa benchmark probes, started M11 with manifest-declared sandbox capabilities and per-app runtime budgets enforced by the main-owned runtime, started M12 with a shared SDK-facing app policy module plus a minimal `watchctl` authoring/validation tool, and now start M13 by treating launcher/settings as manifest-declared system apps inside the same foreground lifecycle contract.

## Ordered milestones

1. **M0 — Baseline (current PR):** Source audit, hardware map, baseline documentation, release build, ELF section report, and comparison tool are complete. Target build gate: **PASS**. Runtime memory/power and HIL items not measured are listed explicitly; no physical values are inferred. A full flash image and linker map remain outstanding.
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
- Do not create SDK/runtime crates before their milestone. Current branch work has completed targeted M1 hygiene fixes and is taking narrow M2/M3/M4 slices; keep each step small and behavior-preserving.
- Preserve Cargo.lock and upgrade dependency families independently.
- Keep measurements separate from source-derived estimates and comments.
- Require host tests for pure parsing/policy logic and HIL evidence for electrical, timing, wake, coexistence, and power claims.
- Record pass/fail/blocked/not-run outcomes without inferring success from an unavailable tool or artifact.
