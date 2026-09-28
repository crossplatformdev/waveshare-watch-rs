use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;

#[derive(Clone, Copy)]
pub struct SensorSnapshot {
    pub accel: (i16, i16, i16),
    pub gyro: (i16, i16, i16),
    pub temp_c10: i16,
}

static SENSOR_REQUEST: Signal<CriticalSectionRawMutex, ()> = Signal::new();
static SENSOR_RESPONSE: Signal<CriticalSectionRawMutex, SensorSnapshot> = Signal::new();

pub fn request_snapshot() {
    SENSOR_REQUEST.signal(());
}

pub fn take_request() -> bool {
    SENSOR_REQUEST.try_take().is_some()
}

pub fn publish_snapshot(snapshot: SensorSnapshot) {
    SENSOR_RESPONSE.signal(snapshot);
}

pub fn take_snapshot() -> Option<SensorSnapshot> {
    SENSOR_RESPONSE.try_take()
}
