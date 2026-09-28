#![allow(dead_code)]

// App framework - common types and trait for all apps/games

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::DrawTarget;

use crate::app_sdk::{
    AppCapabilities, AppLifecycle, AppSandboxPolicy, SwipeDirection, TouchPoint, APP_API_VERSION,
};

pub mod flappy;
pub mod game2048;
pub mod launcher;
pub mod maze;
pub mod mp3player;
pub mod sensor;
pub mod settings;
pub mod smarthome;
pub mod snake;
pub mod tetris;

/// Input state passed to apps each frame
pub struct AppInput {
    pub touch: Option<TouchPoint>,
    pub swipe: Option<SwipeDirection>,
    pub tap: bool,
    pub accel: (f32, f32, f32),
    pub dt_ms: u32, // milliseconds since last frame
}

/// Result of an app update
pub enum AppResult {
    Continue,
    Exit, // Return to launcher/watchface
    Transition(AppState),
}

/// Common trait for all apps/games
pub trait App {
    fn name(&self) -> &str;
    fn api_version(&self) -> u16 {
        APP_API_VERSION
    }
    fn lifecycle(&self) -> AppLifecycle {
        AppLifecycle::Foreground
    }
    fn setup(&mut self);
    fn enter(&mut self) {
        self.setup();
    }
    fn exit(&mut self) {}
    fn update(&mut self, input: &AppInput) -> AppResult;
    fn render<D: DrawTarget<Color = Rgb565>>(&self, d: &mut D);
}

/// All available app states
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AppState {
    Watchface,
    Launcher,
    Snake,
    Game2048,
    Tetris,
    Flappy,
    Maze,
    Sensor,
    Mp3Player,
    SmartHome,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppKind {
    System,
    User,
}

#[derive(Clone, Copy, Debug)]
pub struct AppManifest {
    pub state: AppState,
    pub app_id: &'static str,
    pub display_name: &'static str,
    pub api_version: u16,
    pub lifecycle: AppLifecycle,
    pub kind: AppKind,
    pub launcher_visible: bool,
    pub sandbox: AppSandboxPolicy,
}

pub const APP_MANIFESTS: [AppManifest; 10] = [
    AppManifest {
        state: AppState::Launcher,
        app_id: "launcher",
        display_name: "Launcher",
        api_version: APP_API_VERSION,
        lifecycle: AppLifecycle::Foreground,
        kind: AppKind::System,
        launcher_visible: false,
        sandbox: AppSandboxPolicy {
            tick_ms: 100,
            capabilities: AppCapabilities::TOUCH,
        },
    },
    AppManifest {
        state: AppState::Snake,
        app_id: "snake",
        display_name: "Snake",
        api_version: APP_API_VERSION,
        lifecycle: AppLifecycle::Foreground,
        kind: AppKind::User,
        launcher_visible: true,
        sandbox: AppSandboxPolicy {
            tick_ms: 33,
            capabilities: AppCapabilities::AUDIO,
        },
    },
    AppManifest {
        state: AppState::Game2048,
        app_id: "game-2048",
        display_name: "2048",
        api_version: APP_API_VERSION,
        lifecycle: AppLifecycle::Foreground,
        kind: AppKind::User,
        launcher_visible: true,
        sandbox: AppSandboxPolicy {
            tick_ms: 33,
            capabilities: AppCapabilities::NONE,
        },
    },
    AppManifest {
        state: AppState::Tetris,
        app_id: "tetris",
        display_name: "Tetris",
        api_version: APP_API_VERSION,
        lifecycle: AppLifecycle::Foreground,
        kind: AppKind::User,
        launcher_visible: true,
        sandbox: AppSandboxPolicy {
            tick_ms: 33,
            capabilities: AppCapabilities::NONE,
        },
    },
    AppManifest {
        state: AppState::Flappy,
        app_id: "flappy-bird",
        display_name: "Flappy Bird",
        api_version: APP_API_VERSION,
        lifecycle: AppLifecycle::Foreground,
        kind: AppKind::User,
        launcher_visible: true,
        sandbox: AppSandboxPolicy {
            tick_ms: 33,
            capabilities: AppCapabilities::TOUCH,
        },
    },
    AppManifest {
        state: AppState::Maze,
        app_id: "maze",
        display_name: "Maze (Tilt)",
        api_version: APP_API_VERSION,
        lifecycle: AppLifecycle::Foreground,
        kind: AppKind::User,
        launcher_visible: true,
        sandbox: AppSandboxPolicy {
            tick_ms: 33,
            capabilities: AppCapabilities::MOTION,
        },
    },
    AppManifest {
        state: AppState::Sensor,
        app_id: "sensor",
        display_name: "Sensors",
        api_version: APP_API_VERSION,
        lifecycle: AppLifecycle::Foreground,
        kind: AppKind::User,
        launcher_visible: true,
        sandbox: AppSandboxPolicy {
            tick_ms: 100,
            capabilities: AppCapabilities::MOTION,
        },
    },
    AppManifest {
        state: AppState::Mp3Player,
        app_id: "mp3-player",
        display_name: "MP3 Player",
        api_version: APP_API_VERSION,
        lifecycle: AppLifecycle::Foreground,
        kind: AppKind::User,
        launcher_visible: true,
        sandbox: AppSandboxPolicy {
            tick_ms: 200,
            capabilities: AppCapabilities::AUDIO.union(AppCapabilities::STORAGE),
        },
    },
    AppManifest {
        state: AppState::SmartHome,
        app_id: "smart-home",
        display_name: "Smart Home",
        api_version: APP_API_VERSION,
        lifecycle: AppLifecycle::Foreground,
        kind: AppKind::User,
        launcher_visible: true,
        sandbox: AppSandboxPolicy {
            tick_ms: 100,
            capabilities: AppCapabilities::NETWORK,
        },
    },
    AppManifest {
        state: AppState::Settings,
        app_id: "settings",
        display_name: "Settings",
        api_version: APP_API_VERSION,
        lifecycle: AppLifecycle::Foreground,
        kind: AppKind::System,
        launcher_visible: true,
        sandbox: AppSandboxPolicy {
            tick_ms: 50,
            capabilities: AppCapabilities::TOUCH.union(AppCapabilities::NETWORK),
        },
    },
];

pub fn app_manifest(state: AppState) -> Option<&'static AppManifest> {
    APP_MANIFESTS
        .iter()
        .find(|manifest| manifest.state == state)
}

pub fn app_supports(state: AppState, capability: AppCapabilities) -> bool {
    app_manifest(state)
        .map(|manifest| manifest.sandbox.capabilities.contains(capability))
        .unwrap_or(false)
}

pub fn launcher_entries() -> impl Iterator<Item = &'static AppManifest> {
    APP_MANIFESTS
        .iter()
        .filter(|manifest| manifest.launcher_visible)
}
