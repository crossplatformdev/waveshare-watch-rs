# Memory and Binary Baseline

Revision: `5eb445bce1222a7ac9045dfb33231bc39b29abc0`.

## Measurement status

The firmware ELF was built successfully with Xtensa Rust 1.97.0-nightly (`1.97.0.0`); `cargo build --release` and `cargo build --workspace --release` both passed. The artifact is a 32-bit Xtensa statically linked ELF, 1,178,972 bytes on disk. `espflash 4.6.0 save-image --chip esp32s3 --merge --flash-size 32mb --flash-mode qio --flash-freq 80mhz --skip-padding --skip-update-check` generated a 766,624-byte ESP-IDF application image at `/tmp/waveshare-watch-app.bin`. This app image does not include a bootloader or partition table, and its flash parameters have not been verified on hardware. Linker map, runtime heap/stack watermarks, SRAM/PSRAM free/peak, and radio/DMA runtime allocation remain **NOT MEASURED**. A host `size` program parsed the target ELF; the section values below are static ELF section sizes, not physical memory consumption.

`tools/size-report <firmware.elf>` reports classified `.text`, `.rwtext`, `.vectors`, `.rodata`, `.rodata_desc`, `.data`, and `.bss` sections using GNU-compatible `size -A`; other linker sections are retained in the JSON `sections` map. Save a report with `--output <json>` and compare later builds with `--compare <baseline.json>`. The classified `total` sums only those selected sections; it excludes reserved dummy regions and is not flash image size or runtime RAM usage.

## Known statically requested capacities

| Item | Requested/derived amount | Placement and caveats |
|---|---:|---|
| One RGB565 410 × 502 frame | 411,640 bytes | Allocated as `Vec<u16>` through the configured allocator |
| Framebuffer front + back | 823,280 bytes | Two full-screen vectors in `Framebuffer::new()` |
| Swipe snapshots | 823,280 bytes | Two additional full-screen `Vec<u16>` values allocated in `main` |
| Four full-frame buffers total | 1,646,560 bytes (~1.57 MiB) | Source-level allocated capacity; actual PSRAM placement and allocator overhead not measured |
| QSPI pixel scratch | 8,000 bytes | Heap `Vec<u8>` in `QspiBus` |
| SPI DMA macro argument | 8,000 bytes | `dma_buffers!(8000)` used for each RX/TX buffer; linker/runtime placement not confirmed |
| Audio beep array | 4,000 bytes | `ConstStaticCell<[u8; 4000]>` backing storage |
| I2S TX descriptors | 8 descriptors | `ConstStaticCell<[DmaDescriptor; 8]>`; byte footprint not measured here |
| Internal heap request | 200 KiB | `esp_alloc::heap_allocator!(size: 200 * 1024)` in `main` |
| PSRAM allocator | PSRAM peripheral passed to allocator macro | OPI mode set in `.cargo/config.toml`; allocatable/free/peak bytes not recorded |
| Network/radio/resources, task stacks, DMA and runtime | Unknown | No map file, allocator instrumentation, stack watermark, or radio memory report |

The 200 KiB internal heap request is what the current source configures; it is not a free-memory measurement. `README.md` still describes a 64 KB SRAM heap, which conflicts with current `main.rs` and must not be treated as the present setting. ESP32-S3 SRAM reservation for runtime, radio, DMA, stacks, and linker sections is not quantified. Other linker sections include `.stack` 15,068 bytes, `.rwdata_dummy` 63,232 bytes, and `.rotext_dummy` 131,072 bytes; these special sections are not included in the classified total.

## Baseline result fields

| Metric | Result |
|---|---|
| Firmware `.text`, `.rwtext`, `.vectors` | **578,833 bytes** |
| Firmware `.rodata`, `.rodata_desc` | **103,832 bytes** |
| Firmware `.data`, `.data.wifi` | **18,308 bytes** |
| Firmware `.bss` | **245,148 bytes** |
| Classified sections total | **946,121 bytes** |
| ELF file size | **1,178,972 bytes** |
| ESP-IDF application image | **766,624 bytes** (no bootloader/partition table; skip-padding) |
| Full flash image size | **NOT MEASURED — bootloader/partition table not included** |
| Internal SRAM static/heap free | **NOT MEASURED** |
| PSRAM static/heap free | **NOT MEASURED** |
| Task stacks / high-water | **NOT MEASURED** |
| Radio memory | **NOT MEASURED** |
| DMA buffers and placement | **PARTIALLY SOURCE-IDENTIFIED; NOT MEASURED** |

## Required next measurement

Generate and archive the bootable binary and linker map, and capture runtime allocator/stack/radio/DMA telemetry separately. Only then establish numeric flash/SRAM/PSRAM comparisons.
