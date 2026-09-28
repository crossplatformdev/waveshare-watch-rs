#[path = "../src/peripherals/settings_store.rs"]
mod settings_store;

use settings_store::{StoredWifiConfig, TransactionalSettings};

#[test]
fn round_trip_slot_encoding() {
    let mut store = TransactionalSettings::new();
    store.commit_wifi_config(StoredWifiConfig::new("waveshare", "secret-pass"));

    let recovered = store.recover().expect("slot should decode");
    assert_eq!(recovered.ssid_str(), "waveshare");
    assert_eq!(recovered.password_str(), "secret-pass");
}

#[test]
fn recovery_prefers_newest_valid_slot() {
    let mut store = TransactionalSettings::new();
    store.commit_wifi_config(StoredWifiConfig::new("old-net", "old-pass"));
    store.commit_wifi_config(StoredWifiConfig::new("new-net", "new-pass"));

    let recovered = store.recover().expect("one slot should be valid");
    assert_eq!(recovered.ssid_str(), "new-net");
    assert_eq!(recovered.password_str(), "new-pass");
}

#[test]
fn recovery_ignores_torn_newer_write() {
    let mut base = TransactionalSettings::new();
    base.commit_wifi_config(StoredWifiConfig::new("stable-net", "stable-pass"));

    let mut newer = TransactionalSettings::with_slots(*base.slots());
    newer.commit_wifi_config(StoredWifiConfig::new("new-net", "new-pass"));

    let mut torn_slots = *newer.slots();
    torn_slots[1][40..].fill(0);
    let torn = TransactionalSettings::with_slots(torn_slots);

    let recovered = torn.recover().expect("older committed slot should survive");
    assert_eq!(recovered.ssid_str(), "stable-net");
    assert_eq!(recovered.password_str(), "stable-pass");
}

#[test]
fn recovery_ignores_checksum_corruption() {
    let mut store = TransactionalSettings::new();
    store.commit_wifi_config(StoredWifiConfig::new("good-net", "good-pass"));
    store.commit_wifi_config(StoredWifiConfig::new("bad-net", "bad-pass"));

    let mut corrupted_slots = *store.slots();
    corrupted_slots[1][20] ^= 0x5A;
    let corrupted = TransactionalSettings::with_slots(corrupted_slots);

    let recovered = corrupted.recover().expect("older committed slot should remain valid");
    assert_eq!(recovered.ssid_str(), "good-net");
    assert_eq!(recovered.password_str(), "good-pass");
}

#[test]
fn recovery_handles_generation_wraparound() {
    let max_cfg = StoredWifiConfig::new("old", "pass");
    let mut max_store = TransactionalSettings::new();
    max_store.commit_wifi_config(max_cfg);
    let mut raw = *max_store.slots();
    raw[0][8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    let checksum = test_checksum(&raw[0][..settings_store::SETTINGS_SLOT_LEN - 4]).to_le_bytes();
    raw[0][settings_store::SETTINGS_SLOT_LEN - 4..].copy_from_slice(&checksum);

    let wrapped_cfg = StoredWifiConfig::new("new", "pass");
    let mut wrapped_store = TransactionalSettings::with_slots(raw);
    wrapped_store.commit_wifi_config(wrapped_cfg);

    let recovered = wrapped_store.recover().expect("wrapped generation should still recover newest");
    assert_eq!(recovered.ssid_str(), "new");
}

fn test_checksum(bytes: &[u8]) -> u32 {
    let mut hash = 0x811C9DC5u32;
    for byte in bytes {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}
