#![allow(dead_code)]

// App framework - common types and trait for all apps/games

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::DrawTarget;

use crate::peripherals::touch::{SwipeDirection, TouchPoint};

pub mod snake;
pub mod game2048;
pub mod tetris;
pub mod flappy;
pub mod maze;
pub mod settings;
pub mod mp3player;
pub mod smarthome;

/// Input state passed to apps each frame
pub struct AppInput {
    pub touch: Option<TouchPoint>,
    pub swipe: Option<SwipeDirection>,
    pub tap: bool,
    pub accel: (f32, f32, f32),
    pub dt_ms: u32, // milliseconds since last frame
}

pub const APP_API_VERSION: u16 = 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppLifecycle {
    Foreground,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AppCapabilities(u16);

impl AppCapabilities {
    pub const NONE: Self = Self(0);
    pub const TOUCH: Self = Self(1 << 0);
    pub const MOTION: Self = Self(1 << 1);
    pub const NETWORK: Self = Self(1 << 2);
    pub const AUDIO: Self = Self(1 << 3);
    pub const STORAGE: Self = Self(1 << 4);

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AppSandboxPolicy {
    pub tick_ms: u16,
    pub capabilities: AppCapabilities,
}

/// Result of an app update
pub enum AppResult {
    Continue,
    Exit, // Return to launcher/watchface
}

/// Common trait for all apps/games
pub trait App {
    fn name(&self) -> &str;
    fn api_version(&self) -> u16 { APP_API_VERSION }
    fn lifecycle(&self) -> AppLifecycle { AppLifecycle::Foreground }
    fn setup(&mut self);
    fn enter(&mut self) { self.setup(); }
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
    Mp3Player,
    SmartHome,
    Settings,
}

#[derive(Clone, Copy, Debug)]
pub struct AppManifest {
    pub state: AppState,
    pub app_id: &'static str,
    pub display_name: &'static str,
    pub api_version: u16,
    pub lifecycle: AppLifecycle,
    pub sandbox: AppSandboxPolicy,
}

pub const APP_MANIFESTS: [AppManifest; 8] = [
    AppManifest {
        state: AppState::Snake,
        app_id: "snake",
        display_name: "Snake",
        api_version: APP_API_VERSION,
        lifecycle: AppLifecycle::Foreground,
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
        sandbox: AppSandboxPolicy {
            tick_ms: 33,
            capabilities: AppCapabilities::MOTION,
        },
    },
    AppManifest {
        state: AppState::Mp3Player,
        app_id: "mp3-player",
        display_name: "MP3 Player",
        api_version: APP_API_VERSION,
        lifecycle: AppLifecycle::Foreground,
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
        sandbox: AppSandboxPolicy {
            tick_ms: 50,
            capabilities: AppCapabilities::NETWORK,
        },
    },
];

pub fn app_manifest(state: AppState) -> Option<&'static AppManifest> {
    APP_MANIFESTS.iter().find(|manifest| manifest.state == state)
}

pub fn app_supports(state: AppState, capability: AppCapabilities) -> bool {
    app_manifest(state)
        .map(|manifest| manifest.sandbox.capabilities.contains(capability))
        .unwrap_or(false)
}
