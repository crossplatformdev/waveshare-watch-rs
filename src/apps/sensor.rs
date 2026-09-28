use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::DrawTarget;

use crate::apps::{App, AppInput, AppResult};
use crate::services::sensor::{request_snapshot, take_snapshot, SensorSnapshot};
use crate::ui::pages;

pub struct SensorApp {
    snapshot: SensorSnapshot,
}

impl SensorApp {
    pub fn new() -> Self {
        Self {
            snapshot: SensorSnapshot {
                accel: (0, 0, 0),
                gyro: (0, 0, 0),
                temp_c10: 0,
            },
        }
    }
}

impl App for SensorApp {
    fn name(&self) -> &str {
        "Sensors"
    }

    fn setup(&mut self) {
        request_snapshot();
    }

    fn update(&mut self, _input: &AppInput) -> AppResult {
        if let Some(snapshot) = take_snapshot() {
            self.snapshot = snapshot;
        }
        request_snapshot();
        AppResult::Continue
    }

    fn render<D: DrawTarget<Color = Rgb565>>(&self, d: &mut D) {
        let _ = pages::draw_sensors_page(
            d,
            self.snapshot.accel.0,
            self.snapshot.accel.1,
            self.snapshot.accel.2,
            self.snapshot.gyro.0,
            self.snapshot.gyro.1,
            self.snapshot.gyro.2,
            self.snapshot.temp_c10,
        );
    }
}
