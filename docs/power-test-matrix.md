# Physical Power Test Matrix

This is a test procedure/template, not a set of results. Every case is **NOT RUN** until measured on a physical device. Do not fill in estimated or comment-derived current values.

## Test record

Record the following for every run:

| Field | Value |
|---|---|
| Case / configuration | |
| Board revision and hardware modifications | |
| Firmware commit SHA | |
| Rust toolchain and `Cargo.lock` hash | |
| Power source / battery and state of charge | |
| Supply voltage at the device | |
| Measurement instrument, range, and sampling rate | |
| Stabilization period and measurement duration | |
| Average current (measured) | |
| Peak current (measured) | |
| Procedure notes / wake events | |

## Cases

| Case | Setup to record | Status |
|---|---|---|
| Boot | Cold boot; repeat count; startup interval | NOT RUN |
| Watchface bright | Display brightness and stable duration | NOT RUN |
| Watchface dim | Display brightness and stable duration | NOT RUN |
| AOD | Brightness, RTC minute transition, observation period | NOT RUN |
| Display off | Screen-off state, network/radio and IMU state | NOT RUN |
| Light sleep | Verified sleep mode, enabled wake sources, wake count | NOT RUN |
| Deep sleep | Verified mode, wake source, retained/lost state | NOT RUN |
| Wi-Fi idle | AP, security mode, power-save config, idle duration | NOT RUN |
| Wi-Fi transfer | AP, transfer direction/size/duration, signal strength | NOT RUN |
| BLE advertising | Advertising parameters and duration | NOT RUN |
| BLE connected | Peer, connection interval/latency, traffic | NOT RUN |
| IMU low rate | Sensor configuration, sampling rate, screen state | NOT RUN |
| IMU high rate | Sensor configuration, sampling rate, screen state | NOT RUN |
| Audio playback | Codec/PA configuration, output level, duration | NOT RUN |
| Audio recording | ADC/microphone configuration and capture duration | NOT RUN |
| SD card | Card make/capacity, idle or transfer operation | NOT RUN |
| CPU 80 MHz | Workload and stable measurement interval | NOT RUN |
| CPU 160 MHz | Workload and stable measurement interval | NOT RUN |
| CPU 240 MHz | Workload and stable measurement interval | NOT RUN |
| Dual-core idle | Runtime configuration and idle interval | NOT RUN |
| Dual-core busy | Defined workload on each core and duration | NOT RUN |
| Charging | USB/source voltage, battery state, charge phase | NOT RUN |

## Repeatability

Keep the same board, instrument setup, voltage, workload, radio conditions, display settings, and duration when comparing commits. Record average and peak from the instrument rather than estimating from software. Repeat each case and preserve raw instrument exports alongside the summarized record. A missing capability or unverified sleep/wake mode is **BLOCKED**, not a zero-current result.
