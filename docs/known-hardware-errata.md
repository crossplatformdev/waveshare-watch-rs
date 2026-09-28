# Known Hardware and Integration Caveats

Revision: `5eb445bce1222a7ac9045dfb33231bc39b29abc0`.

The following are audit observations and items requiring verification, not claims about confirmed silicon defects. No official schematic, errata sheet, or physical board was available in this checkout.

| Item | Baseline observation | Required verification / safe interim stance |
|---|---|---|
| GPIO35 with octal PSRAM | OPI PSRAM is enabled in `.cargo/config.toml`; GPIO35 is not assigned in `board.rs` or used by firmware. | Do not repurpose GPIO35 as GPIO until the exact module routing and physical safety are established. |
| GPIO39 RTC IRQ | `RTC_INT` is declared as GPIO39, but `main` does not configure it. | Do not depend on it for wake/alarm; use the existing internal timer behavior until HIL validates it. |
| GPIO38 touch IRQ | FT3168 INT uses GPIO38 with pull-up and falling-edge wait, with level reads in the loop. | Confirm interrupt polarity, wake behavior, and sleep-mode compatibility on the actual board. |
| AXP2101 IRQ | The PMIC driver writes IRQ enables to zero and clears status registers; no PMIC IRQ pin is mapped or consumed. | Treat IRQ handling as absent. Verify routing and status-register behavior before introducing IRQ use. |
| PWR button | `board.rs` names GPIO10 `PWR_BUTTON`, but it is not configured or used. No PKEY GPIO/status combination is implemented. | Do not assume GPIO10 alone represents power-key state. Verify GPIO10, PMIC PKEY, and their combination on hardware. |
| BOOT button | GPIO0 is configured as an input with pull-up and falling-edge wait; handling is present across watchface/apps. | Verify short-press/debounce behavior and system boot/flash-mode interactions on hardware. |
| TE signal | GPIO13 is read in a bounded 400-iteration loop before selected flushes; no interrupt/event wait exists. | Measure pulse and flush timing, and avoid treating the bounded loop as verified VSYNC synchronization. |
| Audio input | GPIO42 is mapped as ADC data, but only I2S TX is initialized. | Microphone/ES7210 recording and duplex behavior are not implemented or verified. |
| USB | Board description mentions USB, but no USB application path is found. | Data, charging, and wake behavior are outside this software baseline. |
| Deep/light sleep | Timer and GPIO async futures are awaited, but no measured sleep residency or validated deep-sleep wake source is recorded. | Do not claim actual low-power sleep or deep-sleep wake compatibility without HIL. |
| RTC/NTP time zone | NTP conversion adds a fixed UTC+2 offset before setting the RTC. | Treat timezone/DST handling as a known software integration issue; no UTC-only baseline guarantee exists. |
| SD timestamps | SD initialization uses a fixed `DummyTime` calendar timestamp, not RTC time. | File timestamps are not representative of current time. |

No hardware errata item in this file is confirmed against a particular board revision. Resolve these points against the official schematic and current device datasheets, then test on hardware before changing pin use or sleep/wake behavior.
