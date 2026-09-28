#![allow(dead_code)]

use core::str;

pub const MAX_SSID_LEN: usize = 32;
pub const MAX_PASSWORD_LEN: usize = 64;
pub const SETTINGS_SLOT_LEN: usize = 112;

const SETTINGS_MAGIC: [u8; 4] = *b"WSET";
const SETTINGS_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoredWifiConfig {
    pub ssid: [u8; MAX_SSID_LEN],
    pub ssid_len: usize,
    pub password: [u8; MAX_PASSWORD_LEN],
    pub pass_len: usize,
}

impl StoredWifiConfig {
    pub const fn empty() -> Self {
        Self {
            ssid: [0; MAX_SSID_LEN],
            ssid_len: 0,
            password: [0; MAX_PASSWORD_LEN],
            pass_len: 0,
        }
    }

    pub fn new(ssid: &str, password: &str) -> Self {
        let mut config = Self::empty();
        config.set_ssid(ssid);
        config.set_password(password);
        config
    }

    pub fn set_ssid(&mut self, value: &str) {
        self.ssid.fill(0);
        let bytes = value.as_bytes();
        let len = bytes.len().min(MAX_SSID_LEN);
        self.ssid[..len].copy_from_slice(&bytes[..len]);
        self.ssid_len = len;
    }

    pub fn set_password(&mut self, value: &str) {
        self.password.fill(0);
        let bytes = value.as_bytes();
        let len = bytes.len().min(MAX_PASSWORD_LEN);
        self.password[..len].copy_from_slice(&bytes[..len]);
        self.pass_len = len;
    }

    pub fn ssid_str(&self) -> &str {
        str::from_utf8(&self.ssid[..self.ssid_len]).unwrap_or("")
    }

    pub fn password_str(&self) -> &str {
        str::from_utf8(&self.password[..self.pass_len]).unwrap_or("")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StoredSettingsSlot {
    generation: u32,
    wifi: StoredWifiConfig,
}

pub struct TransactionalSettings {
    slots: [[u8; SETTINGS_SLOT_LEN]; 2],
}

impl TransactionalSettings {
    pub const fn new() -> Self {
        Self {
            slots: [[0; SETTINGS_SLOT_LEN]; 2],
        }
    }

    pub const fn with_slots(slots: [[u8; SETTINGS_SLOT_LEN]; 2]) -> Self {
        Self { slots }
    }

    pub fn recover(&self) -> Option<StoredWifiConfig> {
        recover_slots(&self.slots).map(|slot| slot.wifi)
    }

    pub fn commit_wifi_config(&mut self, wifi: StoredWifiConfig) -> u32 {
        let (current, current_index) = recover_slots_with_index(&self.slots);
        let generation = current.map(|slot| slot.generation.wrapping_add(1)).unwrap_or(1);
        let target_slot = current_index.map(|index| index ^ 1).unwrap_or(0);
        encode_slot(
            StoredSettingsSlot { generation, wifi },
            &mut self.slots[target_slot],
        );
        generation
    }

    pub fn slots(&self) -> &[[u8; SETTINGS_SLOT_LEN]; 2] {
        &self.slots
    }
}

fn recover_slots(slots: &[[u8; SETTINGS_SLOT_LEN]; 2]) -> Option<StoredSettingsSlot> {
    recover_slots_with_index(slots).0
}

fn recover_slots_with_index(
    slots: &[[u8; SETTINGS_SLOT_LEN]; 2],
) -> (Option<StoredSettingsSlot>, Option<usize>) {
    let first = decode_slot(&slots[0]).map(|slot| (slot, 0usize));
    let second = decode_slot(&slots[1]).map(|slot| (slot, 1usize));
    match (first, second) {
        (Some((a, ai)), Some((b, bi))) => {
            if !generation_is_newer(b.generation, a.generation) {
                (Some(a), Some(ai))
            } else {
                (Some(b), Some(bi))
            }
        }
        (Some((a, ai)), None) => (Some(a), Some(ai)),
        (None, Some((b, bi))) => (Some(b), Some(bi)),
        (None, None) => (None, None),
    }
}

fn generation_is_newer(candidate: u32, current: u32) -> bool {
    candidate != current && candidate.wrapping_sub(current) < (u32::MAX / 2)
}

fn encode_slot(slot: StoredSettingsSlot, out: &mut [u8; SETTINGS_SLOT_LEN]) {
    out.fill(0);
    out[0..4].copy_from_slice(&SETTINGS_MAGIC);
    out[4] = SETTINGS_VERSION;
    out[5] = slot.wifi.ssid_len as u8;
    out[6] = slot.wifi.pass_len as u8;
    out[7] = 0;
    out[8..12].copy_from_slice(&slot.generation.to_le_bytes());
    out[12..12 + MAX_SSID_LEN].copy_from_slice(&slot.wifi.ssid);
    out[44..44 + MAX_PASSWORD_LEN].copy_from_slice(&slot.wifi.password);
    let checksum = checksum32(&out[..SETTINGS_SLOT_LEN - 4]);
    out[SETTINGS_SLOT_LEN - 4..].copy_from_slice(&checksum.to_le_bytes());
}

fn decode_slot(raw: &[u8; SETTINGS_SLOT_LEN]) -> Option<StoredSettingsSlot> {
    if raw[0..4] != SETTINGS_MAGIC || raw[4] != SETTINGS_VERSION {
        return None;
    }

    let ssid_len = raw[5] as usize;
    let pass_len = raw[6] as usize;
    if ssid_len > MAX_SSID_LEN || pass_len > MAX_PASSWORD_LEN {
        return None;
    }

    let stored_checksum = u32::from_le_bytes(raw[SETTINGS_SLOT_LEN - 4..].try_into().ok()?);
    let computed_checksum = checksum32(&raw[..SETTINGS_SLOT_LEN - 4]);
    if stored_checksum != computed_checksum {
        return None;
    }

    let mut wifi = StoredWifiConfig::empty();
    wifi.ssid.copy_from_slice(&raw[12..12 + MAX_SSID_LEN]);
    wifi.password.copy_from_slice(&raw[44..44 + MAX_PASSWORD_LEN]);
    wifi.ssid_len = ssid_len;
    wifi.pass_len = pass_len;

    Some(StoredSettingsSlot {
        generation: u32::from_le_bytes(raw[8..12].try_into().ok()?),
        wifi,
    })
}

fn checksum32(bytes: &[u8]) -> u32 {
    let mut hash = 0x811C9DC5u32;
    for byte in bytes {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}
