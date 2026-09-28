# Current Firmware Architecture Baseline

Baseline source revision: `5eb445bce1222a7ac9045dfb33231bc39b29abc0`

## Overview

The firmware is one `no_std`, `no_main` Cargo package (`waveshare-watch-rs` 0.4.0), not a workspace. `src/main.rs` owns peripheral initialization, global state, input handling, power/display state transitions, networking orchestration, and application dispatch. The code is asynchronous at the main-loop boundary through `esp-rtos`/Embassy; the applications and rendering are synchronous and run in the same main task.

There is no independent kernel scheduler, application process isolation, inter-core service IPC, or application ABI. App state is selected by a single `AppState` enum and apps receive a frame-oriented `AppInput`.

## Layers

| Area | Files | Current responsibility |
|---|---|---|
| Board constants | `src/board.rs` | Pin numbers, bus addresses, display geometry |
| Display drivers | `src/drivers/co5300.rs`, `qspi_bus.rs`, `framebuffer.rs` | CO5300 command sequence, QSPI pixel transfers, RGB565 drawing and full/rectangular flushes |
| Peripheral drivers | `src/peripherals/*.rs` | AXP2101, FT3168, PCF85063A, QMI8658, ES8311, radio helpers, HTTP parsing/client scaffolding |
| UI | `src/ui/*.rs` | Watchface, pages, launcher, T9 keyboard, power page |
| Apps | `src/apps/*.rs` | Snake, 2048, Tetris, Flappy, Maze, Settings, MP3-player UI, SmartHome UI |
| Orchestration | `src/main.rs` | Hardware bring-up, adaptive timed/event loop, state machines, rendering and app dispatch |

The same I2C peripheral is shared using `RefCell` and `RefCellDevice`. The display uses SPI2 in QSPI mode with DMA; the SD card uses SPI3. Wi-Fi and BLE share an `esp-radio` controller. The main async function continues to own the UI/event loop and service orchestration on core 0, while a second-core Embassy executor is brought online on core 1 for background work and emits a live heartbeat for runtime telemetry.

## Runtime and rendering

`main` initializes the HAL at 160 MHz, internal and PSRAM allocators, RTOS timer, software interrupts, buses, display, sensors, storage/audio interfaces, and radio objects. It allocates a two-buffer display framebuffer and two full-frame swipe snapshots. An adaptive `select3` wait races a timer against falling-edge waits on touch and BOOT, after which the core-0 main task samples services and runs application logic. Core 1 now hosts a second Embassy executor that publishes a heartbeat used by the power diagnostics instead of leaving the secondary core unused.

The loop is timed rather than continuously spinning during normal idle, but it still wakes periodically. Main-loop tick periods include 16 ms while a button/touch is held, 30 s with display off, 10 s in AOD, 1 s for a static clock, 100 ms for sensors and several menus, and 33 ms for games/gyro animation. Touch I2C reads are conditional in the watchface path; the launcher also reads touch in its dispatch path. The firmware now records the current sleep budget and worst observed wake-latency slip in `PowerStats` for on-device inspection, and it uses explicit runtime policy helpers for IMU leases, wireless idle auto-off, and BOOT/watchface navigation targets.

Most screen changes still transmit the complete framebuffer. The clock watchface now carries dirty-region metadata through `WatchFace::render()` and uses `Framebuffer::flush_region` for incremental time/battery/gyro updates, but the broader UI does not yet use a general dirty-region compositor. TE synchronization is a bounded GPIO level loop, not an interrupt/event wait.

## Product functionality represented in code

- Display, touch, RTC, IMU, PMIC battery/status, SD card, speaker-output beep, Wi-Fi connection/NTP, and BLE advertising have code paths.
- App implementations are statically linked and dispatched in-process.
- The MP3 player is still UI scaffolding only. The SmartHome application now hands bounded HTTP requests to `main.rs`, which owns the network stack and writes short status/response summaries back to the app state; this still does not establish TLS or broader product-level HTTP behavior.
- Driver structs and board constants do not by themselves establish that a feature is electrically verified or complete; see [hardware-map.md](hardware-map.md) and [known-hardware-errata.md](known-hardware-errata.md).

## Current architectural constraints

- `main.rs` is the central owner of hardware and policy; hardware APIs are not isolated behind system services, though some power/resource decisions are now named policy helpers instead of open-coded conditions.
- Error results are frequently ignored or converted to defaults in initialization and the event loop.
- There is no app isolation, capability model, transactional app install, OTA flow, or simulator in the baseline. A narrow M8 slice now adds a transactional two-slot settings record with standalone host-side fault-injection tests, but it is not yet backed by persistent flash or app-install storage.
- The present M4 slice establishes only a coarse core policy (UI/event loop plus active services on core 0; secondary executor reserved on core 1). It does not yet migrate more application work to core 1 or include broader latency instrumentation, load balancing, or application-level affinity controls.
- The present M6 slice is limited to watchface invalidation-driven partial flushes; most other pages and app renders still redraw whole frames.
- The present M7 slice centralizes only a narrow subset of navigation policy (watchface launch gestures and BOOT-as-Back targets); broader gesture routing and power-button policy are still inline in `main.rs`.
- The present M8 slice covers only the settings-record format and recovery logic; persistent media integration and broader app-data storage are still pending.
- The present baseline records implementation facts only; it is not a target architecture decision.

## Source audit findings

- One `unsafe` block remains in `src/peripherals/cpu_clock.rs` for direct volatile SYSTEM-register access. The I2S descriptor and beep storage previously called out here now use `ConstStaticCell` in `src/main.rs` instead of `static mut`.
- There are 12 direct `.unwrap()`/`.expect()` calls in Rust sources. They include bus/DMA/radio setup and the fixed FAT timestamp construction in `src/main.rs`; failures in hardware setup can panic. Other fallible results are frequently discarded or replaced with defaults, including QSPI transfer results in `src/drivers/qspi_bus.rs`.
- Polling and waits include the adaptive `Timer::after` main-loop tick, conditional touch I2C reads, RTC/battery periodic reads, and two bounded 400-iteration TE level loops in `src/drivers/framebuffer.rs`. Other loops stream QSPI chunks or handle network/game logic; not every loop is an idle busy-wait.
- Heap-backed allocations include four full-frame `Vec<u16>` buffers, an 8,000-byte QSPI scratch `Vec`, and a dynamically sized SD MP3-file list. There is no per-frame allocation measurement or allocator telemetry.
- Existing TODO markers include SD power gating and Wi-Fi scanning.
- Source scanning did not find `#[test]`/`#[cfg(test)]`; host tests could not reach project code with the installed stable toolchain. See [build-baseline.md](build-baseline.md).
