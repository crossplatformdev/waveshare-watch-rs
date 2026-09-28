#![no_std]
#![no_main]

extern crate alloc;

mod board;
mod drivers;
mod peripherals;
mod ui;
mod apps;
mod app_sdk;
#[cfg(feature = "wasm-spike")]
mod wasm_spike;
#[cfg(feature = "wasm-spike")]
mod wasm_spike_payload;

use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::sync::atomic::{AtomicU32, Ordering};

use embedded_hal_bus::i2c::RefCellDevice;
use esp_alloc as _;
use esp_backtrace as _;

esp_bootloader_esp_idf::esp_app_desc!();

use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Instant, Timer};

use esp_hal::delay::Delay;
use esp_hal::dma::{DmaRxBuf, DmaTxBuf};
use esp_hal::dma_buffers;
use esp_hal::gpio::{InputConfig, Level, Output, OutputConfig, Pull, Input};
use esp_hal::i2c::master::{Config as I2cConfig, I2c};
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::spi::Mode as SpiMode;
use esp_hal::system::Stack as CoreStack;
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;
use static_cell::{ConstStaticCell, StaticCell};

use crate::drivers::co5300::Co5300Display;
use crate::drivers::framebuffer::Framebuffer;
use crate::drivers::qspi_bus::QspiBus;
use crate::peripherals::power::Axp2101Power;
use crate::peripherals::power_stats::{DisplayState, PowerStats, WifiMode};
use crate::peripherals::touch::{Ft3168Touch, SwipeDirection, TouchPoint};
use crate::peripherals::rtc::Pcf85063aRtc;
use crate::peripherals::imu::Qmi8658Imu;
use crate::ui::watchface::WatchFace;
use crate::ui::pages::{self, Page};
use crate::ui::power_page;
use crate::app_sdk::{AppCapabilities, AppLifecycle, APP_API_VERSION};
use crate::apps::{app_manifest, app_supports, App, AppInput, AppResult, AppState};
use crate::apps::snake::SnakeGame;
use crate::apps::game2048::Game2048;
use crate::apps::tetris::TetrisGame;
use crate::apps::flappy::FlappyGame;
use crate::apps::maze::MazeGame;
use crate::apps::launcher::Launcher;
use crate::apps::settings::SettingsApp;
use crate::apps::mp3player::Mp3Player;
use crate::apps::smarthome::{HttpMethod, SmartHomeApp};
use crate::peripherals::audio::{Es8311, fill_beep_buffer};
use crate::peripherals::http::{HttpResponse, http_get, http_post};

// Network runner task (must be spawned for WiFi to work)
#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, esp_radio::wifi::WifiDevice<'static>>) -> ! {
    runner.run().await
}

#[derive(Clone, Copy)]
struct SmartHomeRequest {
    idx: usize,
    method: HttpMethod,
    url: [u8; 96],
    url_len: usize,
}

impl SmartHomeRequest {
    fn url(&self) -> &str {
        core::str::from_utf8(&self.url[..self.url_len]).unwrap_or("")
    }
}

#[derive(Clone, Copy)]
struct SmartHomeResponse {
    idx: usize,
    response: [u8; 32],
    response_len: usize,
    success: bool,
}

static SMARTHOME_REQUEST: Signal<CriticalSectionRawMutex, SmartHomeRequest> = Signal::new();
static SMARTHOME_RESPONSE: Signal<CriticalSectionRawMutex, SmartHomeResponse> = Signal::new();
static EXECUTOR_CORE_1: StaticCell<esp_rtos::embassy::Executor> = StaticCell::new();
static APP_CORE_STACK: StaticCell<CoreStack<8192>> = StaticCell::new();
static CORE1_HEARTBEAT_MS: AtomicU32 = AtomicU32::new(0);
const WIRELESS_TOGGLE_DEBOUNCE_MS: u64 = 1_000;
const WIRELESS_IDLE_AUTO_OFF_SECS: u64 = 300;
const WIRELESS_IDLE_RECHECK_SECS: u64 = 60;
const NAV_BUTTON_DEBOUNCE_MS: u64 = 200;

fn now_ms() -> u32 { Instant::now().as_millis() as u32 }

fn wireless_idle_expired(now: Instant, last_policy_change: Instant, idle_secs: u64) -> bool {
    idle_secs >= WIRELESS_IDLE_AUTO_OFF_SECS
        && (now - last_policy_change).as_secs() >= WIRELESS_IDLE_RECHECK_SECS
}

fn imu_lease_active(
    screen_state: u8,
    app_state: AppState,
    current_page: Page,
    gyro_enabled: bool,
) -> bool {
    screen_state >= 2
        && (gyro_enabled
            || app_state == AppState::Maze
            || app_state == AppState::Tetris
            || app_state == AppState::Flappy
            || (app_state == AppState::Watchface && current_page == Page::Sensors))
}

fn watchface_navigation_target(
    current_page: Page,
    swipe_event: Option<SwipeDirection>,
    apps_tapped: bool,
    boot_pressed: bool,
) -> Option<AppState> {
    if apps_tapped || boot_pressed {
        Some(AppState::Launcher)
    } else if current_page == Page::Clock && matches!(swipe_event, Some(SwipeDirection::Up)) {
        Some(AppState::Launcher)
    } else {
        None
    }
}

fn boot_back_target(app_state: AppState) -> AppState {
    match app_state {
        AppState::Watchface => AppState::Launcher,
        AppState::Launcher | AppState::Snake => AppState::Watchface,
        AppState::Game2048
        | AppState::Tetris
        | AppState::Flappy
        | AppState::Maze
        | AppState::Mp3Player
        | AppState::SmartHome
        | AppState::Settings => AppState::Launcher,
    }
}

fn apply_navigation_target(
    app_state: &mut AppState,
    target: AppState,
    launcher: &mut Launcher,
    settings_app: &mut SettingsApp,
    watchface: &mut WatchFace,
    page_dirty: &mut bool,
) {
    if *app_state == AppState::Settings && target != AppState::Settings {
        settings_app.exit();
    }
    *app_state = target;
    if target == AppState::Launcher {
        launcher.enter();
    } else if target == AppState::Settings {
        settings_app.enter();
    }
    if target == AppState::Watchface {
        watchface.force_redraw();
        *page_dirty = true;
    }
}

fn app_tick_budget(state: AppState) -> Option<Duration> {
    app_manifest(state).map(|manifest| Duration::from_millis(manifest.sandbox.tick_ms as u64))
}

fn sandboxed_app_input(
    state: AppState,
    touch: Option<TouchPoint>,
    swipe: Option<SwipeDirection>,
    tap: bool,
    accel: (f32, f32, f32),
    dt_ms: u32,
) -> AppInput {
    AppInput {
        touch: if app_supports(state, AppCapabilities::TOUCH) {
            touch
        } else {
            None
        },
        swipe,
        tap,
        accel: if app_supports(state, AppCapabilities::MOTION) {
            accel
        } else {
            (0.0, 0.0, 0.0)
        },
        dt_ms,
    }
}

#[embassy_executor::task]
async fn core1_heartbeat_task() -> ! {
    loop {
        CORE1_HEARTBEAT_MS.store(now_ms(), Ordering::Relaxed);
        Timer::after(Duration::from_secs(1)).await;
    }
}

// Simple NTP sync (UDP to pool.ntp.org:123)
async fn ntp_sync(
    stack: embassy_net::Stack<'static>,
    rtc: &mut crate::peripherals::rtc::Pcf85063aRtc<impl embedded_hal::i2c::I2c>,
) -> Result<(), ()> {
    use embassy_net::udp::{UdpSocket, PacketMetadata};

    let mut rx_meta = [PacketMetadata::EMPTY; 1];
    let mut rx_buf = [0u8; 256];
    let mut tx_meta = [PacketMetadata::EMPTY; 1];
    let mut tx_buf = [0u8; 256];

    let mut socket = UdpSocket::new(stack, &mut rx_meta, &mut rx_buf, &mut tx_meta, &mut tx_buf);
    socket.bind(12345).map_err(|_| ())?;

    // NTP request packet (simplified: 48 bytes, first byte = 0x1B for client mode)
    let mut ntp_request = [0u8; 48];
    ntp_request[0] = 0x1B; // LI=0, VN=3, Mode=3 (client)

    // Resolve pool.ntp.org (use Google's NTP IP directly: 216.239.35.0)
    let ntp_addr = embassy_net::Ipv4Address::new(216, 239, 35, 0);
    socket.send_to(&ntp_request, (ntp_addr, 123)).await.map_err(|_| ())?;

    // Wait for response (timeout 5s)
    let mut response = [0u8; 48];
    match embassy_time::with_timeout(
        Duration::from_secs(5),
        socket.recv_from(&mut response),
    ).await {
        Ok(Ok((len, _addr))) if len >= 48 => {
            // Parse NTP timestamp (bytes 40-43 = seconds since 1900-01-01)
            let ntp_secs = u32::from_be_bytes([response[40], response[41], response[42], response[43]]);
            // Convert NTP epoch (1900) to Unix epoch (1970): subtract 70 years in seconds
            let unix_secs = ntp_secs.wrapping_sub(2_208_988_800);
            // Convert to hours/minutes/seconds (UTC+2 for France)
            let utc_offset = 2 * 3600; // CEST (summer time)
            let local_secs = unix_secs + utc_offset;
            let time_of_day = local_secs % 86400;
            let hours = (time_of_day / 3600) as u8;
            let minutes = ((time_of_day % 3600) / 60) as u8;
            let seconds = (time_of_day % 60) as u8;

            // Calculate date (simplified: days since epoch)
            let total_days = (local_secs / 86400) as i32;
            // Simple date from days since 1970-01-01
            let (year, month, day) = days_to_date(total_days);

            println!("[NTP] Time: {:02}:{:02}:{:02} {:02}/{:02}/{}", hours, minutes, seconds, day, month, year);

            // Set RTC
            let dt = crate::peripherals::rtc::DateTime::new(
                (year - 2000) as u8, month as u8, day as u8,
                hours, minutes, seconds,
            );
            let _ = rtc.set_time(&dt);
            Ok(())
        }

        _ => Err(()),
    }
}

fn summarize_http_response(resp: &HttpResponse) -> alloc::string::String {
    if resp.body_len > 0 {
        if let Ok(body) = core::str::from_utf8(&resp.body[..resp.body_len]) {
            let body = body.trim();
            if !body.is_empty() {
                return alloc::format!("{} {}", resp.status, body);
            }
        }
    }
    alloc::format!("{}", resp.status)
}

async fn dispatch_smarthome_request(
    stack: embassy_net::Stack<'static>,
    method: HttpMethod,
    url: &str,
) -> Result<(alloc::string::String, bool), ()> {
    let resp = match method {
        HttpMethod::Get => http_get(stack, url).await?,
        HttpMethod::Post => http_post(stack, url, "").await?,
    };
    let success = (200..300).contains(&resp.status);
    Ok((summarize_http_response(&resp), success))
}

#[embassy_executor::task]
async fn smarthome_http_task(stack: embassy_net::Stack<'static>) -> ! {
    loop {
        let req = SMARTHOME_REQUEST.wait().await;
        let (response_text, success) = match dispatch_smarthome_request(stack, req.method, req.url()).await {
            Ok((response, success)) => (response, success),
            Err(()) => (alloc::string::String::from("REQ ERR"), false),
        };

        let mut response = SmartHomeResponse {
            idx: req.idx,
            response: [0u8; 32],
            response_len: 0,
            success,
        };
        let bytes = response_text.as_bytes();
        let len = bytes.len().min(response.response.len());
        response.response[..len].copy_from_slice(&bytes[..len]);
        response.response_len = len;
        SMARTHOME_RESPONSE.signal(response);
    }
}

fn days_to_date(days_since_epoch: i32) -> (u32, u32, u32) {
    // Simplified date calculation from days since 1970-01-01
    let mut y = 1970i32;
    let mut remaining = days_since_epoch;
    loop {
        let days_in_year = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) { 366 } else { 365 };
        if remaining < days_in_year { break; }
        remaining -= days_in_year;
        y += 1;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let month_days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 0;
    while m < 12 && remaining >= month_days[m] {
        remaining -= month_days[m];
        m += 1;
    }
    (y as u32, (m + 1) as u32, (remaining + 1) as u32)
}

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::RgbColor;

/// Take a cheap snapshot of the current power state into the shared
/// `PowerStats` struct. Called from the Power page renderer (and from the
/// swipe preview) so the user sees a live read-out without us adding any
/// extra sampling. All inputs are already tracked by the main loop.
fn update_power_stats(
    stats: &mut PowerStats,
    screen_state: u8,
    imu_on: bool,
    wifi_connected: bool,
    wifi_on_request: bool,
    brightness: u8,
    batt_mv: u16,
    batt_pct: u8,
    charging: bool,
) {
    stats.display = Some(match screen_state {
        0 => DisplayState::Off,
        1 => DisplayState::Aod,
        2 => DisplayState::Dim,
        _ => DisplayState::Bright,
    });
    stats.wifi = Some(if !wifi_on_request && !wifi_connected {
        WifiMode::Off
    } else if wifi_connected {
        // set_power_save(Maximum) is applied via Config, so once connected
        // we're in WIFI_PS_MAX_MODEM.
        WifiMode::PowerSave
    } else {
        // Radio up but handshake still in progress.
        WifiMode::Active
    });
    stats.imu_on = imu_on;
    stats.brightness = brightness;
    // Audio/SD track "currently drawing current". The codec is in shutdown
    // except during a beep; the SD is not currently gated (see TODO in
    // main init). For now, report both off — flip these flags when the
    // main loop wakes the respective subsystem.
    stats.audio_on = false;
    stats.sd_on = false;
    stats.battery_mv = batt_mv;
    stats.battery_pct = batt_pct;
    stats.charging = charging;
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    // Heap: 200KB SRAM + PSRAM for large allocs
    // The BLE+WiFi coex radio stack needs ~100KB+ of internal SRAM for
    // btdm_controller_init tasks and buffers. 64KB was too small and
    // caused a StoreProhibited panic (null-pointer from failed alloc).
    esp_alloc::heap_allocator!(size: 200 * 1024);
    // Power-aware: default to 160MHz instead of 240MHz.
    // Saves ~30% CPU power without noticeable impact on UI/sensor work.
    // Game code can still trigger short bursts via DMA/peripherals at 80MHz QSPI which is unchanged.
    let peripherals = esp_hal::init(
        esp_hal::Config::default()
            .with_cpu_clock(esp_hal::clock::CpuClock::_160MHz)
    );
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);

    // PSRAM
    esp_alloc::psram_allocator!(peripherals.PSRAM, esp_hal::psram);

    // Embassy timer
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    esp_println::logger::init_logger_from_env();
    println!("=== Waveshare Watch RS v0.4 (Embassy) ===");

    let delay = Delay::new();

    // === I2C Bus ===
    let i2c = I2c::new(
        peripherals.I2C0,
        I2cConfig::default().with_frequency(Rate::from_hz(board::I2C_FREQ_HZ)),
    )
    .expect("I2C failed")
    .with_sda(peripherals.GPIO15)
    .with_scl(peripherals.GPIO14);
    let i2c_ref = RefCell::new(i2c);

    // === Power ===
    let mut power = Axp2101Power::new(RefCellDevice::new(&i2c_ref));
    let _ = power.init();
    // Drop the die-temp ADC channel we never show on the UI — shaves a
    // few hundred µA off the AXP2101 housekeeping load.
    let _ = power.trim_adc_channels();
    println!("[POWER] OK");

    // === Display 80MHz DMA ===
    let spi_config = SpiConfig::default()
        .with_frequency(Rate::from_mhz(80))
        .with_mode(SpiMode::_0);
    let (rx_buf, rx_desc, tx_buf, tx_desc) = dma_buffers!(8000);
    let dma_rx = DmaRxBuf::new(rx_desc, rx_buf).unwrap();
    let dma_tx = DmaTxBuf::new(tx_desc, tx_buf).unwrap();
    let spi = Spi::new(peripherals.SPI2, spi_config)
        .expect("SPI failed")
        .with_sck(peripherals.GPIO11)
        .with_sio0(peripherals.GPIO4)
        .with_sio1(peripherals.GPIO5)
        .with_sio2(peripherals.GPIO6)
        .with_sio3(peripherals.GPIO7)
        .with_dma(peripherals.DMA_CH0)
        .with_buffers(dma_rx, dma_tx);
    let cs = Output::new(peripherals.GPIO12, Level::High, OutputConfig::default());
    let reset = Output::new(peripherals.GPIO8, Level::High, OutputConfig::default());
    let mut display = Co5300Display::new(QspiBus::new(spi, cs), reset);
    display.init();

    // Enable Tearing Effect output on CO5300 (TE pin = GPIO13)
    // Command 0x35 = TEARON, param 0x00 = VBlank only
    display.bus_mut().write_c8d8(0x35, 0x00);
    let te_pin = Input::new(peripherals.GPIO13, InputConfig::default());
    println!("[DISPLAY] OK (TE VSync enabled)");

    // === Framebuffer PSRAM ===
    let mut fb = Framebuffer::new();
    fb.clear_color(Rgb565::BLACK);
    fb.flush(&mut display);
    println!("[FB] OK");

    // === Touch ===
    let mut touch_rst = Output::new(peripherals.GPIO9, Level::High, OutputConfig::default());
    // GPIO38 is the FT3168 INT line: held high by pull-up, pulled low by the controller
    // when a finger is on the screen. We use it both for level checks and as an async wake source.
    let mut touch_int = Input::new(peripherals.GPIO38, InputConfig::default().with_pull(Pull::Up));
    touch_rst.set_low(); delay.delay_millis(10); touch_rst.set_high(); delay.delay_millis(50);
    let mut touch = Ft3168Touch::new(RefCellDevice::new(&i2c_ref));
    let _ = touch.init();
    println!("[TOUCH] OK");

    // === RTC ===
    let mut rtc = Pcf85063aRtc::new(RefCellDevice::new(&i2c_ref));
    let _ = rtc.init();
    println!("[RTC] OK");

    // === IMU ===
    let mut imu = Qmi8658Imu::new(RefCellDevice::new(&i2c_ref));
    let _ = imu.init();
    println!("[IMU] OK");

    // === SD Card (SPI3) ===
    println!("[SD] Init...");
    let sd_spi_config = SpiConfig::default()
        .with_frequency(Rate::from_mhz(4))
        .with_mode(SpiMode::_0);
    let sd_spi = Spi::new(peripherals.SPI3, sd_spi_config)
        .expect("SPI3 failed")
        .with_sck(peripherals.GPIO2)
        .with_mosi(peripherals.GPIO1)
        .with_miso(peripherals.GPIO3);
    let sd_cs = Output::new(peripherals.GPIO17, Level::High, OutputConfig::default());

    use embedded_hal_bus::spi::ExclusiveDevice;
    let sd_spi_dev = ExclusiveDevice::new_no_delay(sd_spi, sd_cs).unwrap();
    let sd_card = embedded_sdmmc::SdCard::new(sd_spi_dev, delay);
    let mut mp3_files: alloc::vec::Vec<alloc::string::String> = alloc::vec::Vec::new();
    match sd_card.num_bytes() {
        Ok(size) => {
            println!("[SD] Card {}MB", size / 1024 / 1024);

            // Scan /mp3/ directory
            struct DummyTime;
            impl embedded_sdmmc::TimeSource for DummyTime {
                fn get_timestamp(&self) -> embedded_sdmmc::Timestamp {
                    embedded_sdmmc::Timestamp::from_calendar(2026, 4, 6, 12, 0, 0).unwrap()
                }
            }

            let mut volume_mgr = embedded_sdmmc::VolumeManager::new(sd_card, DummyTime);
            match volume_mgr.open_raw_volume(embedded_sdmmc::VolumeIdx(0)) {
            Ok(volume) => {
                if let Ok(root_dir) = volume_mgr.open_root_dir(volume) {
                    if let Ok(mp3_dir) = volume_mgr.open_dir(root_dir, "MP3") {
                        println!("[SD] Found /MP3/ folder");
                        let _ = volume_mgr.iterate_dir(mp3_dir, |entry| {
                            if !entry.attributes.is_directory() {
                                let name = core::str::from_utf8(&entry.name.base_name()).unwrap_or("?");
                                let ext = core::str::from_utf8(&entry.name.extension()).unwrap_or("");
                                let full = alloc::format!("{}.{}", name.trim(), ext.trim());
                                println!("[SD]   {}", full);
                                mp3_files.push(full);
                            }
                        });
                        println!("[SD] {} files found", mp3_files.len());
                        let _ = volume_mgr.close_dir(mp3_dir);
                    } else {
                        // Try lowercase
                        if let Ok(mp3_dir) = volume_mgr.open_dir(root_dir, "mp3") {
                            println!("[SD] Found /mp3/ folder");
                            let _ = volume_mgr.iterate_dir(mp3_dir, |entry| {
                                if !entry.attributes.is_directory() {
                                    let name = core::str::from_utf8(&entry.name.base_name()).unwrap_or("?");
                                    let ext = core::str::from_utf8(&entry.name.extension()).unwrap_or("");
                                    let full = alloc::format!("{}.{}", name.trim(), ext.trim());
                                    println!("[SD]   {}", full);
                                    mp3_files.push(full);
                                }
                            });
                            println!("[SD] {} files found", mp3_files.len());
                            let _ = volume_mgr.close_dir(mp3_dir);
                        } else {
                            println!("[SD] No /mp3/ or /MP3/ folder");
                        }
                    }
                    let _ = volume_mgr.close_dir(root_dir);
                } else {
                    println!("[SD] Can't open root dir");
                }
            }
            Err(e) => {
                println!("[SD] Can't open volume: {:?}", e);
            }
            }
        }
        Err(_) => println!("[SD] No card"),
    }

    // === Audio (ES8311 codec + I2S) ===
    // CRITICAL ORDER:
    // 1. Keep PA amplifier DISABLED (GPIO46 LOW) - prevents white noise from floating I2S line
    // 2. Init codec (codec init leaves DAC powered but no input yet)
    // 3. Immediately mute the codec DAC + HP output
    // 4. Init I2S bus
    // Only when we actually beep: unmute codec -> raise PA_EN -> write DMA -> lower PA_EN -> mute
    println!("[AUDIO] Init codec...");
    let mut audio_codec = Es8311::new(RefCellDevice::new(&i2c_ref));
    let mut pa_en = Output::new(peripherals.GPIO46, Level::Low, OutputConfig::default());
    if audio_codec.init().is_ok() {
        println!("[AUDIO] Codec OK");
        // Full power-down of the analog blocks (not just mute). The PGA, DAC
        // and HP driver are explicitly cut — saves ~20 mA versus mute() which
        // only zeroes the volume register. `unmute()` brings them back on
        // demand at playback time.
        let _ = audio_codec.shutdown();
    } else {
        println!("[AUDIO] Codec FAILED; disabling playback");
    }

    // === I2S Audio Output (using public write_dma) ===
    println!("[AUDIO] Init I2S...");
    use esp_hal::i2s::master::{I2s, Config as I2sConfig, DataFormat};
    use esp_hal::dma::DmaDescriptor;
    let i2s_config = I2sConfig::default()
        .with_sample_rate(Rate::from_hz(16000))
        .with_data_format(DataFormat::Data16Channel16);
    let i2s_periph = I2s::new(peripherals.I2S0, peripherals.DMA_CH1, i2s_config)
        .expect("I2S failed")
        .with_mclk(peripherals.GPIO16);
    static I2S_TX_DESC: ConstStaticCell<[DmaDescriptor; 8]> =
        ConstStaticCell::new([DmaDescriptor::EMPTY; 8]);
    let i2s_tx_desc = I2S_TX_DESC.take();
    let mut i2s_tx = i2s_periph.i2s_tx
        .with_bclk(peripherals.GPIO41)
        .with_ws(peripherals.GPIO45)
        .with_dout(peripherals.GPIO40)
        .build(i2s_tx_desc);

    // Pre-generate beep sound (800Hz, 50ms, stereo 16-bit @ 16kHz = 3200 bytes)
    static BEEP_BUF: ConstStaticCell<[u8; 4000]> = ConstStaticCell::new([0u8; 4000]);
    let beep_buf = BEEP_BUF.take();
    let beep_len = fill_beep_buffer(beep_buf, 800, 16000, 50);
    println!("[AUDIO] I2S OK ({} bytes beep)", beep_len);

    // === WiFi init (esp-radio) — RADIO STAYS OFF AT BOOT ===
    //
    // POWER BUDGET NOTE:
    // Up to v0.4 the watch booted straight into a connected WiFi STA with
    // no power-save configured. The 2.4 GHz radio + PA alone sustains
    // ~90 mA continuous — the single biggest drain by far, roughly half
    // of the total ~1-hour runtime the user complained about.
    //
    // We now keep the esp-radio stack *initialized* (so we can spin it up
    // at any time without re-allocating 300 KB of radio buffers) but we
    // never call `wifi_controller.start()` on boot. The user toggles the
    // radio on/off with the 'W' button in the top-left of the watchface.
    // When toggled on, we also immediately set PowerSave::MaxModem so the
    // radio sleeps between DTIM beacons (~20 mA average instead of 90).
    //
    // DHCP + NTP are deferred to the first wake of the radio, so a watch
    // that never toggles WiFi pays exactly zero network cost.
    println!("[RADIO] Init radio stack (WiFi OFF, BLE OFF)");
    static RADIO: StaticCell<esp_radio::Controller<'static>> = StaticCell::new();
    // Coerce &'static mut → &'static so both WiFi and BLE can share the controller.
    let radio_controller: &'static esp_radio::Controller<'static> =
        RADIO.init(esp_radio::init().expect("esp-radio init failed"));

    // === WiFi init (OFF by default) ===
    let wifi_config = esp_radio::wifi::Config::default()
        .with_power_save_mode(esp_radio::wifi::PowerSaveMode::Maximum);
    let (mut wifi_controller, wifi_interfaces) = esp_radio::wifi::new(
        radio_controller,
        peripherals.WIFI,
        wifi_config,
    ).expect("WiFi init failed");

    // === BLE init (OFF by default, advertising starts on user toggle) ===
    let mut ble_connector = esp_radio::ble::controller::BleConnector::new(
        radio_controller,
        peripherals.BT,
        esp_radio::ble::Config::default(),
    ).expect("BLE init failed");
    println!("[BLE] Connector ready (advertising OFF)");

    // Pre-fill STA credentials so "toggle WiFi" just flips a bit later.
    // Falls back to empty strings if WIFI_SSID / WIFI_PASS are not set at
    // compile time — WiFi simply won't connect but the watch boots fine.
    use esp_radio::wifi::{ModeConfig, ClientConfig, AuthMethod};
    let wifi_ssid = option_env!("WIFI_SSID").unwrap_or("");
    let wifi_pass = option_env!("WIFI_PASS").unwrap_or("");
    // NOTE: we do NOT call wifi_controller.start() here. The radio stays
    // fully idle until the user taps the WiFi button.

    // === Network Stack scaffolding (idle until radio starts) ===
    use embassy_net::{Config as NetConfig, StackResources};

    let net_config = NetConfig::dhcpv4(Default::default());
    static RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();
    let resources = RESOURCES.init(StackResources::new());

    let (stack, runner) = embassy_net::new(wifi_interfaces.sta, net_config, resources, 12345u64);

    // Core policy: keep the main UI/event loop and active services on core 0
    // while reserving a second Embassy executor on core 1 for background work.
    let app_core_stack = APP_CORE_STACK.init(CoreStack::new());
    esp_rtos::start_second_core(
        peripherals.CPU_CTRL,
        sw_int.software_interrupt0,
        sw_int.software_interrupt1,
        app_core_stack,
        || {
            let executor = EXECUTOR_CORE_1.init(esp_rtos::embassy::Executor::new());
            executor.run(|spawner| {
                spawner.spawn(core1_heartbeat_task()).ok();
            });
        },
    );
    _spawner.spawn(net_task(runner)).ok();
    _spawner.spawn(smarthome_http_task(stack)).ok();

    let mut boot_button = Input::new(peripherals.GPIO0, InputConfig::default().with_pull(Pull::Up));
    println!("=== All systems GO! (Embassy async, WiFi OFF) ===");

    // === State ===
    let mut watchface = WatchFace::new();
    watchface.wifi_connected = false; // radio stays off until user taps the button
    let mut current_page = Page::Clock;
    // Live power-diagnostic snapshot, updated in the main loop and read
    // by the Power page renderer. Kept as plain POD so reading it is free.
    let mut power_stats = PowerStats::new();
    power_stats.cpu_mhz = 160;
    power_stats.core1_online = false;
    let mut app_state = AppState::Watchface;
    let mut snake_game = SnakeGame::new();
    let mut game_2048 = Game2048::new();
    let mut tetris_game = TetrisGame::new();
    let mut flappy_game = FlappyGame::new();
    let mut maze_game = MazeGame::new();
    let mut launcher = Launcher::new();
    let mut settings_app = SettingsApp::new();
    if !wifi_ssid.is_empty() {
        settings_app.wifi_config.set_ssid(wifi_ssid);
        settings_app.wifi_config.set_password(wifi_pass);
    }
    let mut mp3_player = Mp3Player::new();
    let mut smarthome_app = SmartHomeApp::new();
    if !mp3_files.is_empty() {
        mp3_player.set_track_count(mp3_files.len());
        mp3_player.set_track_name(&mp3_files[0]);
    }
    let mut last_touch_y: u16 = 0;
    let mut last_touch_x: u16 = 0;
    let mut accel = (0.0f32, 0.0f32, 0.0f32);
    let mut gyro_data = (0i16, 0i16, 0i16);
    let mut imu_temp: i16 = 250;
    let mut batt_pct: u8 = 0;
    let mut batt_mv: u16 = 0;
    let mut charging = false;
    let mut page_dirty = true;
    let mut swiping = false;
    let mut last_interaction = Instant::now();
    // screen_state levels:
    //   3 = full bright (interactive)
    //   2 = dim (transition)
    //   1 = AOD (Always-On Display: super-dim, minimal HH:MM, 1 update / minute)
    //   0 = full off (DISPOFF + SLPIN)
    let mut screen_state: u8 = 3;
    // Tracks the last minute we rendered in AOD so we update the screen exactly
    // once per minute, not faster. Saves both DMA bandwidth and AMOLED current.
    let mut aod_last_minute: u8 = 99;
    let mut swipe_dir: i32 = 0;
    let mut swipe_start_x: i32 = 0;
    let pixel_count = board::LCD_WIDTH as usize * board::LCD_HEIGHT as usize;
    let mut snap_current: Vec<u16> = vec![0u16; pixel_count];
    let mut snap_target: Vec<u16> = vec![0u16; pixel_count];

    // Initial render
    if let Ok(pct) = power.get_battery_percent() {
        batt_pct = pct;
        batt_mv = power.get_battery_voltage().unwrap_or(0);
        charging = power.is_charging().unwrap_or(false);
        watchface.update_battery(batt_pct, batt_mv, charging);
    }
    if let Ok(dt) = rtc.get_time() {
        watchface.update_time(dt.hours, dt.minutes, dt.seconds);
        watchface.update_date(dt.day, dt.month, dt.year);
    }
    watchface.force_redraw();
    let _ = watchface.render(&mut fb);
    fb.flush(&mut display);

    #[cfg(feature = "wasm-spike")]
    match crate::wasm_spike::run_boot_probe() {
        Ok(metrics) => {
            println!(
                "WASM spike: module={}B engine={}B linker={}B store={}B compile={}us instantiate={}us compat={}",
                metrics.module_bytes,
                metrics.engine_bytes,
                metrics.linker_bytes,
                metrics.store_bytes,
                metrics.compile_us,
                metrics.instantiate_us,
                metrics.compat_version,
            );
            println!(
                "WASM spike: host={} / {} calls in {}us checksum={} fuel={}->{} loop={}",
                metrics.host_call_count,
                metrics.host_call_iterations,
                metrics.host_call_us,
                metrics.host_call_checksum,
                metrics.fuel_before,
                metrics.fuel_after,
                metrics.fuel_loop_result,
            );
        }
        Err(err) => println!("WASM spike failed: {}", err),
    }

    // === Event-driven async main loop ===
    //
    // The loop sleeps the CPU between iterations using `select` over:
    //   * a periodic timer whose period depends on the current app/screen state
    //   * the touch interrupt line (GPIO38, async falling edge)
    //   * the boot button line (GPIO0, async falling edge)
    //
    // Tick budgets per state (only consumed when nothing else wakes us):
    //   * screen off                : 30 s   (deep idle, just to refresh battery%)
    //   * watchface clock, gyro off : 1 s    (only the seconds digit changes)
    //   * watchface clock, gyro on  : 33 ms  (smooth gyro ball)
    //   * sensors page              : 100 ms (10 Hz IMU)
    //   * power page                : 1 s    (diagnostic — must not self-skew)
    //   * launcher / settings / mp3 : 100 ms
    //   * Snake / 2048 / Tetris / Maze / Flappy : 33 ms (~30 Hz, panel cap)
    //
    // A finger held on the screen forces 16 ms tick regardless of state.
    //
    // When the user touches the screen or presses BOOT, the select returns immediately
    // and we process the input. With this design, the CPU is parked >99% of the time
    // while sitting on the watchface.
    use embassy_futures::select::select3;

    let mut next_rtc = Instant::now();
    let mut next_battery = Instant::now();
    let mut last_frame = Instant::now();
    let mut next_watchface_flush = Instant::now();
    // Radio state: we track both what the user *wants* and what the radio
    // actually is. They drift apart briefly during connect/disconnect.
    let mut wifi_on_request: bool = false;      // user toggle
    let mut wifi_started: bool = false;         // controller.start() called
    let mut wifi_connected: bool = false;       // connect_async succeeded
    let mut ntp_synced: bool = false;
    let mut last_wifi_policy_change = Instant::now();
    let mut last_ble_policy_change = Instant::now();
    // Request pending from a UI tap on the WiFi button.
    let mut wifi_toggle_request: bool = false;
    // BLE state
    let mut ble_on: bool = false;
    let mut ble_toggle_request: bool = false;
    let mut smarthome_request_in_flight = false;
    // Power-down the IMU at boot — only enable when a consumer (gyro toggle, game, sensors page) needs it.
    let _ = imu.power_down();
    let mut imu_powered = false;
    // Tracks the previous-iteration state of the FT3168 INT line.
    // We need this to keep polling touch.poll() ONCE more after the finger lifts,
    // otherwise we miss the swipe-end event and pages stay stuck mid-drag.
    let mut was_touching = false;

    loop {
        // Pick a tick budget based on current state. This is the MAX time we'll
        // sleep without any external wake source.
        let touch_held = touch_int.is_low();
        let button_held = boot_button.is_low();

        let tick = if touch_held || button_held {
            // Something is currently being held: wake fast enough to track motion / detect
            // long-press, but no faster than necessary.
            Duration::from_millis(16) // ~60 Hz
        } else if screen_state == 0 {
            // Screen completely off: only wake every 30 s for housekeeping (battery refresh).
            // GPIO falling edges still wake us instantly.
            Duration::from_secs(30)
        } else if screen_state == 1 {
            // AOD mode: wake every 10 s to check if a new minute has started.
            // We don't need exactly 60 s precision because the user only sees minutes change.
            Duration::from_secs(10)
        } else {
            match app_state {
                AppState::Watchface => match current_page {
                    // Clock page: 1 Hz when gyro is off (only seconds change),
                    // 33 ms when gyro is on (smooth ball animation).
                    Page::Clock => if watchface.gyro_enabled {
                        Duration::from_millis(33)
                    } else {
                        Duration::from_secs(1)
                    },
                    Page::Sensors => Duration::from_millis(100), // 10 Hz IMU display
                    Page::System  => Duration::from_secs(2),     // basically static
                    // Power page refreshes at 1 Hz — fast enough to see
                    // changes, slow enough not to skew the measurement.
                    Page::Power   => Duration::from_secs(1),
                },
                AppState::Launcher => Duration::from_millis(100),
                // Flappy previously ran at 8 ms (~125 Hz). The panel can't
                // even display that (VSync is ~33 ms) so the extra ticks
                // just burned CPU and DMA for no visible benefit.
                state => app_tick_budget(state).unwrap_or(Duration::from_millis(100)),
            }
        };
        power_stats.last_tick_ms = tick.as_millis().min(u16::MAX as u64) as u16;
        let wait_started = Instant::now();

        // Sleep until the tick budget elapses OR a falling edge arrives on touch / boot button.
        // Notes:
        //   * If a pin is already low, wait_for_falling_edge will not fire (no edge to wait for),
        //     but the tick above is short (16 ms) so we still wake reactively.
        //   * The futures from esp-hal install GPIO interrupts on creation and remove them on
        //     drop, so the executor parks the CPU between events: this is the main power win.
        let _ = select3(
            Timer::after(tick),
            touch_int.wait_for_falling_edge(),
            boot_button.wait_for_falling_edge(),
        ).await;
        let wake_elapsed = (Instant::now() - wait_started).as_millis();
        if wake_elapsed > tick.as_millis() {
            let slip = (wake_elapsed - tick.as_millis()).min(u16::MAX as u64) as u16;
            power_stats.wake_slip_ms_max = power_stats.wake_slip_ms_max.max(slip);
        }

        let now = Instant::now();
        let dt_ms = (now - last_frame).as_millis() as u32;
        last_frame = now;
        power_stats.core1_online = now_ms().wrapping_sub(CORE1_HEARTBEAT_MS.load(Ordering::Relaxed)) <= 2_000;

        // === Sensors (gated by need + screen state) ===
        // IMU only when an interactive consumer needs it (gyro enabled, IMU-driven game, sensors page).
        // When screen is off OR no consumer needs it, we power-down the IMU completely
        // (CTRL7 = 0). The QMI8658's gyro alone draws ~1.5 mA so this is a meaningful win.
        let need_imu = imu_lease_active(
            screen_state,
            app_state,
            current_page,
            watchface.gyro_enabled,
        );
        if need_imu && !imu_powered {
            let _ = imu.power_up();
            imu_powered = true;
        } else if !need_imu && imu_powered {
            let _ = imu.power_down();
            imu_powered = false;
        }
        if need_imu {
            if let Ok(a) = imu.read_accel() {
                accel = (a.x, a.y, a.z);
                watchface.update_accel(a.x, a.y, a.z);
            }
            if let Ok(g) = imu.read_gyro() {
                gyro_data = ((g.x * 10.0) as i16, (g.y * 10.0) as i16, (g.z * 10.0) as i16);
            }
            if let Ok(t) = imu.read_temperature() {
                imu_temp = (t * 10.0) as i16;
            }
        }

        // RTC: 1 Hz update is enough for a clock display. Skip when screen is off OR in AOD
        // (AOD updates the RTC manually once per minute).
        if screen_state >= 2 && now >= next_rtc {
            if let Ok(dt) = rtc.get_time() {
                watchface.update_time(dt.hours, dt.minutes, dt.seconds);
                watchface.update_date(dt.day, dt.month, dt.year);
            }
            next_rtc = now + Duration::from_secs(1);
        }

        // Battery: every 60 s normally, every 5 min when the screen is off
        // (we still check occasionally to track charge state and update on next wake).
        if now >= next_battery {
            if let Ok(pct) = power.get_battery_percent() {
                batt_pct = pct;
                batt_mv = power.get_battery_voltage().unwrap_or(0);
                charging = power.is_charging().unwrap_or(false);
                watchface.update_battery(batt_pct, batt_mv, charging);
            }
            next_battery = if screen_state == 0 {
                now + Duration::from_secs(600)
            } else {
                // Battery percent rarely changes faster than every few
                // minutes; polling every 60 s was pure I²C overhead.
                now + Duration::from_secs(180)
            };
        }

        // === Touch ===
        // Poll the I2C touch controller when:
        //   1. screen is on AND a finger is currently on the panel (INT low), OR
        //   2. screen is on AND the finger was on the panel last iteration (catches the
        //      lift/swipe-end event — without this, page swipes stay stuck mid-drag).
        // This keeps the bus quiet >99% of the time but never misses release events.
        let mut swipe_event = None;
        let mut tap_event = false;
        let int_low = touch_int.is_low();
        // Touch I2C is only polled in fully-interactive states (AOD has no touch handling).
        let touch_active = screen_state >= 2 && (int_low || was_touching);
        was_touching = int_low;
        if touch_active {
            if let Ok((point, event)) = touch.poll() {
            // Swipe handling for page navigation (only in Watchface mode)
            if app_state == AppState::Watchface {
                if let Some(tp) = point {
                    last_touch_x = tp.x;
                    last_touch_y = tp.y;
                    // Don't start a page swipe if the finger is on the
                    // brightness slider — horizontal drag there adjusts
                    // brightness, not pages.
                    let on_slider = current_page == Page::Clock
                        && WatchFace::brightness_from_tap(tp.x, tp.y).is_some();
                    if !swiping && !on_slider {
                        if swipe_start_x == 0 { swipe_start_x = tp.x as i32; }
                        else {
                            let dx = tp.x as i32 - swipe_start_x;
                            if dx.unsigned_abs() > 30 {
                                swiping = true;
                                swipe_dir = if dx < 0 { -1 } else { 1 };
                                snap_current.copy_from_slice(fb.buffer());
                                let target = if swipe_dir < 0 { current_page.next() } else { current_page.prev() };
                                fb.clear_color(target.color());
                                match target {
                                    Page::Clock => {
                                        let mut wf2 = WatchFace::new();
                                        if let Ok(dt) = rtc.get_time() { wf2.update_time(dt.hours, dt.minutes, dt.seconds); }
                                        wf2.update_battery(batt_pct, batt_mv, charging);
                                        wf2.wifi_connected = wifi_connected;
                                        wf2.force_redraw();
                                        let _ = wf2.render(&mut fb);
                                    }
                                    Page::Sensors => { let _ = pages::draw_sensors_page(&mut fb, 0,0,0,0,0,0,0); }
                                    Page::System => { let _ = pages::draw_system_page(&mut fb, batt_mv, batt_pct, charging); }
                                    Page::Power => {
                                        update_power_stats(&mut power_stats, screen_state, imu_powered,
                                            wifi_connected, wifi_on_request, watchface.brightness,
                                            batt_mv, batt_pct, charging);
                                        let _ = power_page::draw_power_page(&mut fb, &power_stats);
                                    }
                                }
                                snap_target.copy_from_slice(fb.buffer());
                            }
                        }
                    }
                    if swiping {
                        let delta = (tp.x as i32 - swipe_start_x).clamp(-(board::LCD_WIDTH as i32), board::LCD_WIDTH as i32);
                        let offset = ((delta * swipe_dir).clamp(0, board::LCD_WIDTH as i32) as usize) & !1;
                        let w = board::LCD_WIDTH as usize;
                        let h = board::LCD_HEIGHT as usize;
                        if offset > 0 && offset < w {
                            if swipe_dir < 0 {
                                display.set_addr_window(0, 0, (w-offset) as u16, h as u16);
                                display.bus_mut().begin_pixels();
                                for row in 0..h { display.bus_mut().stream_pixels(&snap_current[row*w+offset..row*w+w]); }
                                display.bus_mut().end_pixels();
                                display.set_addr_window((w-offset) as u16, 0, offset as u16, h as u16);
                                display.bus_mut().begin_pixels();
                                for row in 0..h { display.bus_mut().stream_pixels(&snap_target[row*w..row*w+offset]); }
                                display.bus_mut().end_pixels();
                            } else {
                                display.set_addr_window(0, 0, offset as u16, h as u16);
                                display.bus_mut().begin_pixels();
                                for row in 0..h { display.bus_mut().stream_pixels(&snap_target[row*w+w-offset..row*w+w]); }
                                display.bus_mut().end_pixels();
                                display.set_addr_window(offset as u16, 0, (w-offset) as u16, h as u16);
                                display.bus_mut().begin_pixels();
                                for row in 0..h { display.bus_mut().stream_pixels(&snap_current[row*w..row*w+w-offset]); }
                                display.bus_mut().end_pixels();
                            }
                        }
                    }
                }
                if let Some(swipe) = event {
                    swipe_start_x = 0;
                    if swiping {
                        swiping = false;
                        let ok = matches!(
                            (&swipe.direction, swipe_dir),
                            (SwipeDirection::Left, -1) | (SwipeDirection::Right, 1)
                        );
                        if ok {
                            if swipe_dir < 0 { current_page = current_page.next(); }
                            else { current_page = current_page.prev(); }
                            fb.buffer_mut().copy_from_slice(&snap_target);
                            page_dirty = true;
                        } else {
                            fb.buffer_mut().copy_from_slice(&snap_current);
                            fb.flush(&mut display);
                        }
                    } else {
                        swipe_event = Some(swipe.direction);
                        tap_event = swipe.direction == SwipeDirection::Tap;
                    }
                }
            } else {
                // In app mode: track position + forward events
                if let Some(tp) = point {
                    last_touch_x = tp.x;
                    last_touch_y = tp.y;
                }
                if let Some(swipe) = event {
                    swipe_event = Some(swipe.direction);
                    tap_event = swipe.direction == SwipeDirection::Tap;
                }
            }
            }
        }

        // === Screen sleep/wake state machine ===
        // Levels:
        //   3 = full bright + interactive (default)
        //   2 = dim brightness, still interactive (transition state at 20 s idle)
        //   1 = AOD: minimal HH:MM, super-dim, 1 update/min, no I/O
        //   0 = full off (DISPOFF + SLPIN), only GPIO interrupts can wake
        //
        // Transitions on idle: 3 → (20s) → 2 → (40s) → 1 (AOD) → (10min) → 0 (off)
        // Any touch/button bumps us straight back to 3.
        let any_touch = touch_int.is_low();
        if any_touch || swipe_event.is_some() || tap_event || boot_button.is_low() {
            last_interaction = now;
            if screen_state < 3 {
                // Wake up to full bright. If we were fully off (state 0), re-init the panel.
                if screen_state == 0 {
                    display.display_on();
                    Timer::after(Duration::from_millis(20)).await;
                }
                // Restore the user's chosen brightness from the slider.
                display.set_brightness(watchface.brightness);
                screen_state = 3;
                next_watchface_flush = now;
                if app_state == AppState::Watchface {
                    watchface.force_redraw();
                    page_dirty = true;
                    last_wifi_policy_change = now;
                    last_ble_policy_change = now;
                }
            }
        }
        let idle_secs = (now - last_interaction).as_secs();
        // 3 min in AOD → fully off (was 10 min — aggressive saves ~8 mA×7 min)
        if idle_secs >= 180 && screen_state > 0 {
            display.set_brightness(0x00);
            display.display_off();
            screen_state = 0;
        // 15 s idle → AOD (was 40 s — faster dim saves ~45 mA×25 s every cycle)
        } else if idle_secs >= 15 && screen_state > 1 {
            if app_state == AppState::Watchface && current_page == Page::Clock {
                display.set_brightness(0x18); // very dim, ~10% of normal
                screen_state = 1;
                aod_last_minute = 99; // force first AOD frame
            } else {
                // Not on the clock face → no AOD, just go straight to off
                display.set_brightness(0x00);
                display.display_off();
                screen_state = 0;
            }
        // 8 s idle → dim transition (was 20 s)
        } else if idle_secs >= 8 && screen_state > 2 {
            display.set_brightness(0x40);
            screen_state = 2;
        }

        // === WiFi on/off state machine ===
        //
        // We take exactly one action per loop iteration so we don't block
        // the UI during a long-running connect_async(). User wants (wifi_on_request)
        // is driven by tapping the 'W' button on the watchface.
        //
        // On enable:  start() -> connect_async() -> set_power_save(MaxModem) -> NTP (once)
        // On disable: disconnect_async() -> stop()
        //
        // An "auto-off after 5 minutes idle" safety net is kept so that if
        // the user leaves WiFi enabled and wanders off, the radio drops on
        // its own. Turning it back on is manual — intentional.
        if settings_app.wifi_state == crate::peripherals::wifi::WifiState::Connecting
            && app_supports(AppState::Settings, AppCapabilities::NETWORK)
            && !wifi_on_request
            && !wifi_connected
        {
            let ssid = settings_app.wifi_config.ssid_str();
            if !ssid.is_empty()
                && {
                    let password = settings_app.wifi_config.password_str();
                    let client_config = ClientConfig::default()
                        .with_ssid(alloc::string::String::from(ssid))
                        .with_password(alloc::string::String::from(password))
                        .with_auth_method(if password.is_empty() { AuthMethod::None } else { AuthMethod::WpaWpa2Personal });
                    let mode_config = ModeConfig::Client(client_config);
                    wifi_controller.set_config(&mode_config).is_ok()
                }
            {
                wifi_on_request = true;
                last_wifi_policy_change = now;
            } else {
                if ssid.is_empty() {
                    println!("[WIFI] No SSID configured — WiFi disabled");
                } else {
                    println!("[WIFI] Config failed");
                }
                settings_app.wifi_state = crate::peripherals::wifi::WifiState::Error;
            }
        }

        // Debounce the WiFi button: ignore rapid re-taps within 1 s.
        if wifi_toggle_request
            && (now - last_wifi_policy_change).as_millis() >= WIRELESS_TOGGLE_DEBOUNCE_MS
        {
            wifi_on_request = !wifi_on_request;
            wifi_toggle_request = false;
            last_wifi_policy_change = now;
            if wifi_on_request {
                if app_supports(AppState::Settings, AppCapabilities::NETWORK) {
                    let ssid = settings_app.wifi_config.ssid_str();
                    if !ssid.is_empty()
                        && {
                            let password = settings_app.wifi_config.password_str();
                            let client_config = ClientConfig::default()
                                .with_ssid(alloc::string::String::from(ssid))
                                .with_password(alloc::string::String::from(password))
                                .with_auth_method(if password.is_empty() { AuthMethod::None } else { AuthMethod::WpaWpa2Personal });
                            let mode_config = ModeConfig::Client(client_config);
                            wifi_controller.set_config(&mode_config).is_ok()
                        }
                    {
                        settings_app.wifi_state = crate::peripherals::wifi::WifiState::Connecting;
                    } else {
                        if ssid.is_empty() {
                            println!("[WIFI] No SSID configured — WiFi disabled");
                        } else {
                            println!("[WIFI] Config failed");
                        }
                        wifi_on_request = false;
                        settings_app.wifi_state = crate::peripherals::wifi::WifiState::Error;
                    }
                } else {
                    wifi_on_request = false;
                    println!("[WIFI] Settings sandbox blocks network access");
                    settings_app.wifi_state = crate::peripherals::wifi::WifiState::Error;
                }
            } else {
                settings_app.wifi_state = crate::peripherals::wifi::WifiState::Disconnected;
            }
            println!("[WIFI] User toggled → {}", if wifi_on_request { "ON" } else { "OFF" });
        } else if wifi_toggle_request {
            wifi_toggle_request = false; // swallow the bounce
        }

        if wifi_on_request && !wifi_connected {
            if !wifi_started {
                if wifi_controller.start().is_ok() {
                    wifi_started = true;
                }
            }
            if wifi_started {
                // 8 s timeout — avoids blocking the UI forever if AP
                // is unreachable or credentials are wrong.
                match embassy_time::with_timeout(
                    Duration::from_secs(8),
                    wifi_controller.connect_async(),
                ).await {
                    Ok(Ok(())) => {
                        println!("[WIFI] Connected (PS=MaxModem)");
                        wifi_connected = true;
                        settings_app.wifi_state = crate::peripherals::wifi::WifiState::Connected;
                        watchface.wifi_connected = true;
                        watchface.force_redraw();
                        page_dirty = true;
                        // NTP sync only once per boot, after DHCP lands.
                        if !ntp_synced {
                            for _ in 0..30 {
                                if stack.config_v4().is_some() { break; }
                                Timer::after(Duration::from_millis(100)).await;
                            }
                            if stack.config_v4().is_some() {
                                if ntp_sync(stack, &mut rtc).await.is_ok() {
                                    ntp_synced = true;
                                    println!("[NTP] synced");
                                }
                            }
                        }
                    }
                    _ => {
                        // Timeout or error — back off instead of hammering.
                        println!("[WIFI] Connect failed/timeout");
                        wifi_on_request = false;
                        settings_app.wifi_state = crate::peripherals::wifi::WifiState::Error;
                        watchface.wifi_connected = false;
                        watchface.force_redraw();
                        page_dirty = true;
                    }
                }
            }
            last_wifi_policy_change = now;
        }
        if !wifi_on_request && wifi_connected {
            let _ = wifi_controller.disconnect_async().await;
            println!("[WIFI] Disconnected");
            wifi_connected = false;
            settings_app.wifi_state = crate::peripherals::wifi::WifiState::Disconnected;
            watchface.wifi_connected = false;
            let _ = wifi_controller.stop();
            wifi_started = false;
            watchface.force_redraw();
            page_dirty = true;
            last_wifi_policy_change = now;
        }
        // Safety net: radio leases auto-expire after prolonged user idle.
        if wifi_on_request && wireless_idle_expired(now, last_wifi_policy_change, idle_secs) {
            wifi_on_request = false;
            last_wifi_policy_change = now;
        }

        // === BLE state machine ===
        if ble_toggle_request {
            ble_toggle_request = false;
            ble_on = !ble_on;
            if ble_on {
                match crate::peripherals::ble::start_advertising(&mut ble_connector) {
                    Ok(()) => println!("[BLE] Advertising started"),
                    Err(_) => {
                        println!("[BLE] Failed to start advertising");
                        ble_on = false;
                    }
                }
            } else {
                let _ = crate::peripherals::ble::stop_advertising(&mut ble_connector);
                println!("[BLE] Advertising stopped");
            }
            watchface.ble_on = ble_on;
            power_stats.ble_on = ble_on;
            watchface.force_redraw();
            page_dirty = true;
            last_ble_policy_change = now;
        } else if ble_on && wireless_idle_expired(now, last_ble_policy_change, idle_secs) {
            let _ = crate::peripherals::ble::stop_advertising(&mut ble_connector);
            println!("[BLE] Auto-off after idle");
            ble_on = false;
            watchface.ble_on = false;
            power_stats.ble_on = false;
            watchface.force_redraw();
            page_dirty = true;
            last_ble_policy_change = now;
        }

        // === AOD render path ===
        // In AOD we render *only* when the minute changes. Reads RTC, draws minimal
        // black-background HH:MM into the framebuffer, flushes once. Total work per minute:
        // ~1 RTC read + ~1 framebuffer fill + 1 DMA flush. The CPU sleeps the rest of the time.
        if screen_state == 1 {
            if let Ok(dt) = rtc.get_time() {
                if dt.minutes != aod_last_minute {
                    aod_last_minute = dt.minutes;
                    watchface.update_time(dt.hours, dt.minutes, dt.seconds);
                    if let Ok(pct) = power.get_battery_percent() {
                        watchface.update_battery(pct, batt_mv, charging);
                    }
                    let _ = watchface.render_aod(&mut fb);
                    fb.flush(&mut display);
                }
            }
            continue; // skip the normal app/render path
        }

        // When the screen is off, skip all rendering/flushing.
        // Keeps the QSPI bus idle so the CO5300 stays in a clean sleep state,
        // and the wake-up set_brightness/display_on commands always get through.
        if screen_state == 0 {
            continue;
        }

        // === App state machine ===
        match app_state {
            AppState::Watchface => {
                if !swiping {
                    let mut need_flush = false;
                    let mut partial_flush_region = None;
                    if page_dirty {
                        fb.clear_color(current_page.color());
                        match current_page {
                            Page::Clock => { watchface.force_redraw(); }
                            Page::System => { let _ = pages::draw_system_page(&mut fb, batt_mv, batt_pct, charging); }
                            Page::Power => {
                                // Rebuild stats snapshot, then render once.
                                // Subsequent frames will only redraw every
                                // ~1 s (see below) to keep the diagnostic
                                // itself cheap.
                                update_power_stats(&mut power_stats, screen_state, imu_powered,
                                    wifi_connected, wifi_on_request, watchface.brightness,
                                    batt_mv, batt_pct, charging);
                                let _ = power_page::draw_power_page(&mut fb, &power_stats);
                            }
                            _ => {}
                        }
                        page_dirty = false;
                        need_flush = true;
                    }
                    match current_page {
                        Page::Clock => {
                            // Only render if WatchFace says something is dirty.
                            if watchface.needs_render() {
                                if let Ok(render_outcome) = watchface.render(&mut fb) {
                                    if render_outcome.full_redraw {
                                        need_flush = true;
                                    } else if let Some(region) = render_outcome.dirty_region() {
                                        partial_flush_region = Some(region);
                                    }
                                }
                            }
                        }
                        Page::Sensors => {
                            // Sensors page is repainted at the loop tick rate (10 Hz).
                            let ax = (accel.0 * 100.0) as i16;
                            let ay = (accel.1 * 100.0) as i16;
                            let az = (accel.2 * 100.0) as i16;
                            fb.clear_color(current_page.color());
                            let _ = pages::draw_sensors_page(&mut fb, ax, ay, az, gyro_data.0, gyro_data.1, gyro_data.2, imu_temp);
                            need_flush = true;
                        }
                        Page::Power => {
                            // Refresh the snapshot + redraw at ~1 Hz. Any faster
                            // and the diagnostic itself starts to skew the
                            // measurement it's supposed to report.
                            if now >= next_watchface_flush {
                                update_power_stats(&mut power_stats, screen_state, imu_powered,
                                    wifi_connected, wifi_on_request, watchface.brightness,
                                    batt_mv, batt_pct, charging);
                                let _ = power_page::draw_power_page(&mut fb, &power_stats);
                                need_flush = true;
                                next_watchface_flush = now + Duration::from_secs(1);
                            }
                        }
                        Page::System => {} // Static, already rendered
                    }
                    // Only flush if we actually drew something. The TE wait + 402 KB DMA
                    // is by far the heaviest periodic operation in the firmware, so we
                    // gate it strictly on dirtiness.
                    if need_flush {
                        fb.flush_vsync(&mut display, &te_pin);
                        next_watchface_flush = now;
                    } else if let Some(region) = partial_flush_region {
                        fb.flush_region_vsync(&mut display, &te_pin, region.x, region.y, region.w, region.h);
                        next_watchface_flush = now;
                    }
                }

                // Tap/touch dispatch on the Clock page.
                if current_page == Page::Clock {
                    // Brightness slider — responds to both taps and held
                    // drags so you can slide your finger along it.
                    if let Some(bri) = WatchFace::brightness_from_tap(last_touch_x, last_touch_y) {
                        if (touch_int.is_low() || tap_event) && bri != watchface.brightness {
                            watchface.brightness = bri;
                            display.set_brightness(bri);
                            watchface.force_redraw();
                            page_dirty = true;
                        }
                    } else if tap_event {
                        // BLE toggle
                        if WatchFace::is_ble_zone(last_touch_x, last_touch_y) {
                            ble_toggle_request = true;
                            watchface.force_redraw();
                            page_dirty = true;
                        // WiFi toggle
                        } else if WatchFace::is_wifi_zone(last_touch_x, last_touch_y) {
                            wifi_toggle_request = true;
                            watchface.force_redraw();
                            page_dirty = true;
                        // CPU frequency cycle (live DVFS)
                        } else if WatchFace::is_cpu_zone(last_touch_x, last_touch_y) {
                            watchface.cycle_cpu();
                            let actual = crate::peripherals::cpu_clock::set_cpu_mhz(watchface.cpu_mhz);
                            watchface.cpu_mhz = actual;
                            power_stats.cpu_mhz = actual;
                            println!("CPU freq: {}MHz (live)", actual);
                            watchface.force_redraw();
                            page_dirty = true;
                        // Apps launcher
                        } else if let Some(target) = watchface_navigation_target(
                            current_page,
                            swipe_event,
                            WatchFace::is_apps_zone(last_touch_x, last_touch_y),
                            false,
                        ) {
                            apply_navigation_target(&mut app_state, target, &mut launcher, &mut settings_app, &mut watchface, &mut page_dirty);
                        // Gyro toggle
                        } else if WatchFace::is_gyro_zone(last_touch_y) {
                            let enabled = watchface.toggle_gyro();
                            println!("Gyro: {}", if enabled { "ON" } else { "OFF" });
                        }
                    }
                }

                // Reboot button on Power page
                if current_page == Page::Power && tap_event {
                    if power_page::is_reboot_zone(last_touch_x, last_touch_y) {
                        println!("REBOOT requested");
                        esp_hal::system::software_reset();
                    }
                }

                if let Some(target) = watchface_navigation_target(
                    current_page,
                    swipe_event,
                    false,
                    boot_button.is_low(),
                ) {
                    apply_navigation_target(&mut app_state, target, &mut launcher, &mut settings_app, &mut watchface, &mut page_dirty);
                    if boot_button.is_low() {
                        Timer::after(Duration::from_millis(NAV_BUTTON_DEBOUNCE_MS)).await;
                    }
                }
            }

            AppState::Snake => {
                let prev_score = snake_game.score();
                let input = sandboxed_app_input(app_state, None, swipe_event, tap_event, accel, dt_ms.max(1));
                match snake_game.update(&input) {
                    AppResult::Continue => {
                        if snake_game.stepped() {
                            snake_game.render(&mut fb);
                            fb.flush(&mut display);
                            // Beep when food eaten via I2S DMA
                            if snake_game.score() > prev_score {
                                // Unmute codec, then raise PA amplifier, then play
                                if app_supports(app_state, AppCapabilities::AUDIO)
                                    && audio_codec.is_initialized()
                                {
                                    let beep_data = &beep_buf[..beep_len];
                                    let _ = audio_codec.unmute();
                                    delay.delay_millis(2); // let codec stabilize before enabling amp
                                    pa_en.set_high();
                                    if let Ok(transfer) = i2s_tx.write_dma(&beep_data) {
                                        let _ = transfer.wait();
                                    }
                                    // Lower amp FIRST, then mute codec to avoid pop
                                    pa_en.set_low();
                                    let _ = audio_codec.mute();
                                } else {
                                    pa_en.set_low();
                                }
                            }
                        }
                    }
                    AppResult::Exit => {
                        app_state = AppState::Watchface;
                        watchface.force_redraw();
                        page_dirty = true;
                    }
                    AppResult::Transition(_) => {}
                }

                if boot_button.is_low() {
                    let target = boot_back_target(app_state);
                    apply_navigation_target(&mut app_state, target, &mut launcher, &mut settings_app, &mut watchface, &mut page_dirty);
                    Timer::after(Duration::from_millis(NAV_BUTTON_DEBOUNCE_MS)).await;
                }
            }

            AppState::Launcher => {
                // Track touch position for tap detection
                if let Ok((point, _)) = touch.poll() {
                    if let Some(tp) = point {
                        last_touch_x = tp.x;
                        last_touch_y = tp.y;
                    }
                }
                let touch_point = (tap_event || touch_int.is_low()).then_some(TouchPoint {
                    x: last_touch_x,
                    y: last_touch_y,
                    fingers: 1,
                });
                let input = sandboxed_app_input(app_state, touch_point, swipe_event, tap_event, accel, dt_ms.max(1));
                match launcher.update(&input) {
                    AppResult::Transition(new_state) => {
                        if let Some(manifest) = app_manifest(new_state) {
                            if manifest.api_version == APP_API_VERSION
                                && manifest.lifecycle == AppLifecycle::Foreground
                            {
                                launcher.exit();
                                app_state = new_state;
                                match app_state {
                                    AppState::Snake => snake_game.enter(),
                                    AppState::Game2048 => { game_2048.enter(); game_2048.render(&mut fb); fb.flush(&mut display); }
                                    AppState::Tetris => tetris_game.enter(),
                                    AppState::Flappy => flappy_game.enter(),
                                    AppState::Maze => maze_game.enter(),
                                    AppState::Mp3Player => mp3_player.enter(),
                                    AppState::SmartHome => smarthome_app.enter(),
                                    AppState::Settings => settings_app.enter(),
                                    AppState::Watchface | AppState::Launcher => {}
                                }
                            }
                        } else if new_state == AppState::Watchface {
                            apply_navigation_target(
                                &mut app_state,
                                AppState::Watchface,
                                &mut launcher,
                                &mut settings_app,
                                &mut watchface,
                                &mut page_dirty,
                            )
                        }
                    }
                    AppResult::Continue => {
                        launcher.render(&mut fb);
                        fb.flush(&mut display);
                    }
                    AppResult::Exit => {}
                }
                if boot_button.is_low() {
                    let target = boot_back_target(app_state);
                    apply_navigation_target(&mut app_state, target, &mut launcher, &mut settings_app, &mut watchface, &mut page_dirty);
                    Timer::after(Duration::from_millis(NAV_BUTTON_DEBOUNCE_MS)).await;
                }
            }

            AppState::Game2048 => {
                let input = sandboxed_app_input(app_state, None, swipe_event, tap_event, accel, dt_ms.max(1));
                game_2048.update(&input);
                // Only render on input (swipe moves tiles)
                if swipe_event.is_some() {
                    game_2048.render(&mut fb);
                    fb.flush_vsync(&mut display, &te_pin);
                }
                if boot_button.is_low() {
                    let target = boot_back_target(app_state);
                    apply_navigation_target(&mut app_state, target, &mut launcher, &mut settings_app, &mut watchface, &mut page_dirty);
                    Timer::after(Duration::from_millis(NAV_BUTTON_DEBOUNCE_MS)).await;
                }
            }

            AppState::Tetris => {
                let input = sandboxed_app_input(app_state, None, swipe_event, tap_event, accel, dt_ms.max(1));
                tetris_game.update(&input);
                if tetris_game.stepped() || swipe_event.is_some() || tap_event {
                    tetris_game.render(&mut fb);
                    fb.flush_vsync(&mut display, &te_pin);
                }
                if boot_button.is_low() {
                    let target = boot_back_target(app_state);
                    apply_navigation_target(&mut app_state, target, &mut launcher, &mut settings_app, &mut watchface, &mut page_dirty);
                    Timer::after(Duration::from_millis(NAV_BUTTON_DEBOUNCE_MS)).await;
                }
            }

            AppState::Flappy => {
                // Touch via GPIO38 (instant)
                let touch_down = touch_int.is_low();
                let fake_touch = if touch_down { Some(crate::peripherals::touch::TouchPoint { x: 200, y: 250, fingers: 1 }) } else { None };
                let input = sandboxed_app_input(app_state, fake_touch, swipe_event, tap_event, accel, dt_ms.max(1));
                flappy_game.update(&input);
                // Double-buffered render: draw to fb, swap+flush with VSync
                flappy_game.render(&mut fb);
                if now >= next_watchface_flush {
                    fb.swap_and_flush(&mut display, &te_pin);
                    next_watchface_flush = now + Duration::from_millis(33);
                }
                if boot_button.is_low() {
                    let target = boot_back_target(app_state);
                    apply_navigation_target(&mut app_state, target, &mut launcher, &mut settings_app, &mut watchface, &mut page_dirty);
                    Timer::after(Duration::from_millis(NAV_BUTTON_DEBOUNCE_MS)).await;
                }
            }

            AppState::Maze => {
                let input = sandboxed_app_input(app_state, None, swipe_event, tap_event, accel, dt_ms.max(1));
                maze_game.update(&input);
                // Maze renders at 30fps (IMU continuous)
                if now >= next_watchface_flush {
                    maze_game.render(&mut fb);
                    fb.flush_vsync(&mut display, &te_pin);
                    next_watchface_flush = now + Duration::from_millis(33);
                }
                if boot_button.is_low() {
                    let target = boot_back_target(app_state);
                    apply_navigation_target(&mut app_state, target, &mut launcher, &mut settings_app, &mut watchface, &mut page_dirty);
                    Timer::after(Duration::from_millis(NAV_BUTTON_DEBOUNCE_MS)).await;
                }
            }

            AppState::SmartHome => {
                if let Some(response) = SMARTHOME_RESPONSE.try_take() {
                    smarthome_request_in_flight = false;
                    let response_text = core::str::from_utf8(&response.response[..response.response_len]).unwrap_or("ERR");
                    smarthome_app.set_response(response.idx, response_text, response.success);
                }
                let input = sandboxed_app_input(app_state, None, swipe_event, tap_event, accel, dt_ms.max(1));
                smarthome_app.update(&input);
                if let Some((idx, method, url)) = smarthome_app.get_pending_request() {
                    if !app_supports(app_state, AppCapabilities::NETWORK) {
                        smarthome_app.set_response(idx, "SANDBOX", false);
                    } else if smarthome_request_in_flight {
                        smarthome_app.set_response(idx, "BUSY", false);
                    } else if wifi_connected {
                        let mut request = SmartHomeRequest {
                            idx,
                            method,
                            url: [0u8; 96],
                            url_len: 0,
                        };
                        let url_bytes = url.as_bytes();
                        let url_len = url_bytes.len().min(request.url.len());
                        request.url[..url_len].copy_from_slice(&url_bytes[..url_len]);
                        request.url_len = url_len;
                        SMARTHOME_REQUEST.signal(request);
                        smarthome_request_in_flight = true;
                    } else {
                        smarthome_app.set_response(idx, "NO WIFI", false);
                    }
                }
                smarthome_app.render(&mut fb);
                if now >= next_watchface_flush {
                    fb.flush_vsync(&mut display, &te_pin);
                    next_watchface_flush = now + Duration::from_millis(100);
                }
                if boot_button.is_low() {
                    let target = boot_back_target(app_state);
                    apply_navigation_target(&mut app_state, target, &mut launcher, &mut settings_app, &mut watchface, &mut page_dirty);
                    Timer::after(Duration::from_millis(NAV_BUTTON_DEBOUNCE_MS)).await;
                }
            }

            AppState::Mp3Player => {
                let input = sandboxed_app_input(app_state, None, swipe_event, tap_event, accel, dt_ms.max(1));
                mp3_player.update(&input);
                mp3_player.render(&mut fb);
                if now >= next_watchface_flush {
                    fb.flush_vsync(&mut display, &te_pin);
                    next_watchface_flush = now + Duration::from_millis(200);
                }
                if boot_button.is_low() {
                    let target = boot_back_target(app_state);
                    apply_navigation_target(&mut app_state, target, &mut launcher, &mut settings_app, &mut watchface, &mut page_dirty);
                    Timer::after(Duration::from_millis(NAV_BUTTON_DEBOUNCE_MS)).await;
                }
            }

            AppState::Settings => {
                if let Ok((Some(tp), _)) = touch.poll() {
                    last_touch_x = tp.x;
                    last_touch_y = tp.y;
                }
                let touch_point = (tap_event || touch_int.is_low()).then_some(TouchPoint {
                    x: last_touch_x,
                    y: last_touch_y,
                    fingers: 1,
                });
                let input = sandboxed_app_input(app_state, touch_point, swipe_event, tap_event, accel, dt_ms.max(1));
                let _ = settings_app.update(&input);
                settings_app.render(&mut fb);
                if now >= next_watchface_flush {
                    fb.flush_vsync(&mut display, &te_pin);
                    next_watchface_flush = now + Duration::from_millis(50);
                }
                if boot_button.is_low() {
                    let target = boot_back_target(app_state);
                    apply_navigation_target(&mut app_state, target, &mut launcher, &mut settings_app, &mut watchface, &mut page_dirty);
                    Timer::after(Duration::from_millis(NAV_BUTTON_DEBOUNCE_MS)).await;
                }
            }

        }
    }
}
