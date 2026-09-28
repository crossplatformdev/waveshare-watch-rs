# Power and Wakeup Baseline

Revision: `5eb445bce1222a7ac9045dfb33231bc39b29abc0`.

## Measurement status

No physical current measurement, board revision record, battery/source/voltage record, or repeatable HIL power run was available. All current values that appear in comments or UI are unverified software notes, not measured results. This baseline makes **no mA or battery-life claim**.

## Software behavior at baseline

| State/resource | Current behavior in source |
|---|---|
| CPU | Initialized to 160 MHz. A helper performs runtime clock register writes for 80/160/240 MHz and invokes the ROM frequency update routine. |
| Wi-Fi | Radio/controller and network objects are initialized at boot; Wi-Fi is not started until requested. Configures maximum modem power save and attempts auto-off after user idle. Network runner task is spawned at boot. |
| BLE | Connector is initialized at boot; advertising is off until requested. On/off actions send HCI advertising commands. |
| IMU | `power_down()` is requested during initialization; IMU is enabled for selected UI/game consumers and read on loop ticks. |
| Audio | ES8311 is initialized then shut down; codec/PA are enabled for a short game beep. |
| Display | Brightness and display commands implement interactive, dim, AOD, and display-off states. Idle thresholds in `main.rs` are 8 s (dim), 15 s (AOD when on the clock page), and 180 s (display off). |
| AOD | Main loop has a 10-second timer tick and reads RTC to check whether the minute changed; it does not sleep directly to the exact next-minute deadline. |
| Touch/BOOT | `select3` waits on timer and falling-edge futures for GPIO38 and GPIO0. Levels are also sampled in the main loop. |
| Touch controller | I2C touch polling is conditional on an active/just-ended touch in the interactive watchface path; launcher also polls. |
| TE | Flush methods perform up to 400 synchronous level reads waiting for GPIO13; there is no TE IRQ/event wait. |
| Main-loop timers | Adaptive 16 ms held-input tick; 30 s display-off tick; 10 s AOD tick; 1 s clock/power page; 100 ms sensor/menu; 33 ms game/gyro tick. |
| Battery/RTC | Battery is queried on a 180 s active interval and 600 s display-off interval; RTC is queried at 1 s when interactive and during AOD checks. |
| Sleep | The async executor awaits timer/input futures between iterations. Actual light-sleep residency, wake-source behavior, and deep-sleep support are not established. |

The loop can wake periodically even when there is no external input. In addition, held input forces a 16 ms tick and animations/games retain timed updates. AOD minute updates are detected by periodic checks rather than a deadline-aligned wake.

## Physical power matrix status

All rows remain **NOT RUN**: boot; bright/dim watchface; AOD; display off; light/deep sleep; Wi-Fi idle/transfer; BLE advertising/connected; IMU low/high rate; audio playback/recording; SD; CPU 80/160/240 MHz; dual-core idle/busy; charging. The complete repeatable test-case and data-capture template is [power-test-matrix.md](power-test-matrix.md).

No energy budget or “power optimized” conclusion is made from this source audit.
