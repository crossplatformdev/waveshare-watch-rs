#![allow(dead_code)]

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppLifecycle {
    Foreground,
}

impl AppLifecycle {
    #[allow(dead_code)]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Foreground => "foreground",
        }
    }
}

pub const APP_API_VERSION: u16 = 1;
pub const UPDATE_MANIFEST_VERSION: u16 = 1;
pub const UPDATE_SIGNATURE_ALGORITHM: &str = "ed25519";

#[derive(Debug, Clone, Copy)]
pub struct TouchPoint {
    pub x: u16,
    pub y: u16,
    pub fingers: u8,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SwipeDirection {
    Up,
    Down,
    Left,
    Right,
    Tap,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UpdateKind {
    Firmware,
    App,
}

impl UpdateKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Firmware => "firmware",
            Self::App => "app",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "firmware" => Some(Self::Firmware),
            "app" => Some(Self::App),
            _ => None,
        }
    }
}

#[allow(dead_code)]
pub const UPDATE_KIND_NAMES: [(&str, UpdateKind); 2] =
    [("firmware", UpdateKind::Firmware), ("app", UpdateKind::App)];

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

    #[allow(dead_code)]
    pub const fn bits(self) -> u16 {
        self.0
    }
}

#[allow(dead_code)]
pub const APP_CAPABILITY_NAMES: [(&str, AppCapabilities); 5] = [
    ("touch", AppCapabilities::TOUCH),
    ("motion", AppCapabilities::MOTION),
    ("network", AppCapabilities::NETWORK),
    ("audio", AppCapabilities::AUDIO),
    ("storage", AppCapabilities::STORAGE),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AppSandboxPolicy {
    pub tick_ms: u16,
    pub capabilities: AppCapabilities,
}
