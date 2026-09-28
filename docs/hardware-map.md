# Hardware Map Baseline

Source revision: `5eb445bce1222a7ac9045dfb33231bc39b29abc0`.

This map transcribes the repository's current `src/board.rs` constants and `src/main.rs` wiring. It is **not** an independently validated schematic/pinout. No board revision, schematic file, or hardware-in-loop evidence is present in this checkout. Verify against the official schematic and the physical board before relying on unverified pins.

## Pin and bus inventory

| Function | Current pin/address | Code status |
|---|---|---|
| CO5300 QSPI SDIO0–3 | GPIO4–7 | SPI2 quad data pins configured |
| CO5300 QSPI SCLK / CS / RESET | GPIO11 / GPIO12 / GPIO8 | Configured in `main` |
| CO5300 TE | GPIO13 | Configured as input and level-checked during flush |
| Display | 410 × 502; column offset 22, row offset 0 | Constants in `board.rs` |
| Shared I2C SDA / SCL | GPIO15 / GPIO14, 400 kHz | I2C0 configured |
| FT3168 address / INT / RESET | 0x38 / GPIO38 / GPIO9 | Driver initialized; INT used for falling-edge wake and level checks |
| AXP2101 address | 0x34 | Driver initialized; interrupt enables are written disabled |
| QMI8658 address / INT | 0x6B / GPIO21 | Driver initialized/read; GPIO21 is not wired in `main` |
| PCF85063A address / INT | 0x51 / GPIO39 | RTC driver used; GPIO39 is not wired in `main` |
| SD SPI3 CLK / CMD(MOSI) / DATA(MISO) / CS | GPIO2 / GPIO1 / GPIO3 / GPIO17 | Card probing and MP3-directory enumeration attempted at boot |
| I2S MCLK / BCLK / LRCK / DAC data | GPIO16 / GPIO41 / GPIO45 / GPIO40 | I2S TX configured for beep output |
| I2S ADC data | GPIO42 | Board constant only; no I2S RX/input path |
| Speaker amplifier enable | GPIO46 | Configured low at boot, enabled around beep playback |
| BOOT | GPIO0 | Input configured; used for wake and app navigation |
| PWR | GPIO10 | Board constant only; no input/service handling |

## Device-use summary

| Hardware | Baseline implementation |
|---|---|
| ESP32-S3R8, flash, octal PSRAM | ESP32-S3 HAL target and PSRAM allocator are configured; actual board revision and capacity are not measured here |
| CO5300 AMOLED | Custom driver, initialization, brightness/on/off, QSPI DMA flush |
| FT3168 touch | I2C read/poll and swipe/tap tracking; GPIO38 participates in wake |
| QMI8658 IMU | I2C initialization, acceleration/gyro/temperature reads, software power-up/down |
| PCF85063A RTC | I2C read/write; NTP code sets time when Wi-Fi connects |
| AXP2101 PMIC | I2C setup, battery and status reads, ADC/rail setup |
| ES8311 + amplifier | Codec initialization/shutdown and short I2S TX beep |
| ES7210 + microphones | No codec/ADC input driver or capture path identified |
| microSD | SPI probing and directory enumeration; not a general storage service |
| Wi-Fi | Stack/controller initialized but radio is not started on boot; connect/NTP path is conditional |
| BLE | Connector initialized; basic advertising HCI commands only, no GATT service |
| USB | No application-level USB path identified |

GPIO35 is absent from the board pin constants and source use. With octal PSRAM enabled it must remain unavailable as a GPIO pending specific schematic and physical validation. GPIO39 and GPIO21 are declared interrupt pins but unused as GPIOs. No PMIC IRQ pin or verified IRQ input path is declared. These are source observations, not proof of board routing.

## Verification status

- Official schematic cross-check: **NOT RUN** (not included in this checkout).
- Pin electrical/board-revision verification: **NOT RUN**.
- HIL checks for GPIO35, RTC IRQ, touch wake, PMIC status/IRQ, BOOT, PWR, display/TE, audio, SD, Wi-Fi, and BLE: **NOT RUN**.
- The software configuration does not establish GPIO35 safety, RTC IRQ reliability, PWR semantics, deep-sleep wake support, or peripheral electrical behavior.
