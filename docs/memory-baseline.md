# Memory and Binary Baseline

Revision: `5eb445bce1222a7ac9045dfb33231bc39b29abc0`.

## Measurement status

**ELF, firmware binary, linker map, and section sizes: NOT MEASURED.** The pinned `esp` toolchain is absent in this environment, so the firmware release build did not produce an ELF or binary. Do not interpret source-level capacities below as measured SRAM/PSRAM use.

`tools/size-report <firmware.elf>` reports `.text`, `.rodata`, `.data`, and `.bss` from an ELF using GNU-compatible `size -A`. Save a report with `--output <json>` and compare later builds with `--compare <baseline.json>`. The script reports linked section sizes; it does not estimate runtime heap, stack high-water marks, DMA placement, or radio allocations.

## Known statically requested capacities

| Item | Requested/derived amount | Placement and caveats |
|---|---:|---|
| One RGB565 410 × 502 frame | 411,640 bytes | Allocated as `Vec<u16>` through the configured allocator |
| Framebuffer front + back | 823,280 bytes | Two full-screen vectors in `Framebuffer::new()` |
| Swipe snapshots | 823,280 bytes | Two additional full-screen `Vec<u16>` values allocated in `main` |
| Four full-frame buffers total | 1,646,560 bytes (~1.57 MiB) | Source-level allocated capacity; actual PSRAM placement and allocator overhead not measured |
| QSPI pixel scratch | 8,000 bytes | Heap `Vec<u8>` in `QspiBus` |
| SPI DMA macro argument | 8,000 bytes | `dma_buffers!(8000)` used for each RX/TX buffer; linker/runtime placement not confirmed |
| Audio beep array | 4,000 bytes | `static mut` buffer |
| I2S TX descriptors | 8 descriptors | `static mut`; byte footprint not measured here |
| Internal heap request | 200 KiB | `esp_alloc::heap_allocator!(size: 200 * 1024)` in `main` |
| PSRAM allocator | PSRAM peripheral passed to allocator macro | OPI mode set in `.cargo/config.toml`; allocatable/free/peak bytes not recorded |
| Network/radio/resources, task stacks, DMA and runtime | Unknown | No map file, allocator instrumentation, stack watermark, or radio memory report |

The 200 KiB internal heap request is what the current source configures; it is not a free-memory measurement. `README.md` still describes a 64 KB SRAM heap, which conflicts with current `main.rs` and must not be treated as the present setting. ESP32-S3 SRAM reservation for runtime, radio, DMA, stacks, and linker sections is not quantified.

## Baseline result fields

| Metric | Result |
|---|---|
| Firmware `.text` | **NOT MEASURED — no ELF** |
| Firmware `.rodata` | **NOT MEASURED — no ELF** |
| Firmware `.data` | **NOT MEASURED — no ELF** |
| Firmware `.bss` | **NOT MEASURED — no ELF** |
| Flash image size | **NOT MEASURED — no binary** |
| Internal SRAM static/heap free | **NOT MEASURED** |
| PSRAM static/heap free | **NOT MEASURED** |
| Task stacks / high-water | **NOT MEASURED** |
| Radio memory | **NOT MEASURED** |
| DMA buffers and placement | **PARTIALLY SOURCE-IDENTIFIED; NOT MEASURED** |

## Required next measurement

Once the pinned ESP toolchain and linker are available, build the unmodified firmware at this revision, archive the ELF, binary and linker map, run `tools/size-report`, and capture runtime allocator/stack/radio/DMA telemetry separately. Only then establish numeric flash/SRAM/PSRAM comparisons.
