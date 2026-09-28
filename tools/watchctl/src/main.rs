use std::env;
use std::fs;
use std::process::ExitCode;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[path = "../../../src/app_sdk.rs"]
mod app_sdk;

use app_sdk::{
    AppCapabilities, AppLifecycle, AppSandboxPolicy, UpdateKind, APP_API_VERSION,
    APP_CAPABILITY_NAMES, UPDATE_KIND_NAMES, UPDATE_MANIFEST_VERSION, UPDATE_SIGNATURE_ALGORITHM,
};

#[derive(Deserialize)]
struct PackageManifest {
    app_id: String,
    display_name: String,
    api_version: u16,
    lifecycle: String,
    tick_ms: u16,
    capabilities: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct UpdateManifest {
    manifest_version: u16,
    kind: String,
    artifact_name: String,
    version: String,
    sequence: u32,
    rollback_floor: u32,
    artifact_size: u64,
    artifact_sha256: String,
    signature_algorithm: String,
    key_id: String,
    signature_hex: String,
}

fn main() -> ExitCode {
    match run(env::args().collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("watchctl: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    match args.get(1).map(String::as_str) {
        Some("sdk") => {
            print_sdk();
            Ok(())
        }
        Some("init-manifest") => {
            let app_id =
                args.get(2)
                    .ok_or("usage: watchctl init-manifest <app-id> <display-name>")?;
            let display_name =
                args.get(3)
                    .ok_or("usage: watchctl init-manifest <app-id> <display-name>")?;
            print_manifest_template(app_id, display_name);
            Ok(())
        }
        Some("validate-manifest") => {
            let path = args
                .get(2)
                .ok_or("usage: watchctl validate-manifest <manifest.json>")?;
            validate_manifest(path)
        }
        Some("init-update-manifest") => {
            let kind = args.get(2).ok_or(
                "usage: watchctl init-update-manifest <firmware|app> <artifact-name> <version>",
            )?;
            let artifact_name = args.get(3).ok_or(
                "usage: watchctl init-update-manifest <firmware|app> <artifact-name> <version>",
            )?;
            let version = args.get(4).ok_or(
                "usage: watchctl init-update-manifest <firmware|app> <artifact-name> <version>",
            )?;
            print_update_manifest_template(kind, artifact_name, version)
        }
        Some("sign-update-manifest") => {
            let manifest_path = args.get(2).ok_or(
                "usage: watchctl sign-update-manifest <manifest.json> <artifact.bin> <secret-key-hex> [output.json]",
            )?;
            let artifact_path = args.get(3).ok_or(
                "usage: watchctl sign-update-manifest <manifest.json> <artifact.bin> <secret-key-hex> [output.json]",
            )?;
            let secret_key_hex = args.get(4).ok_or(
                "usage: watchctl sign-update-manifest <manifest.json> <artifact.bin> <secret-key-hex> [output.json]",
            )?;
            let output = args.get(5).map(String::as_str);
            sign_update_manifest(manifest_path, artifact_path, secret_key_hex, output)
        }
        Some("verify-update-manifest") => {
            let manifest_path = args.get(2).ok_or(
                "usage: watchctl verify-update-manifest <manifest.json> <artifact.bin> <public-key-hex>",
            )?;
            let artifact_path = args.get(3).ok_or(
                "usage: watchctl verify-update-manifest <manifest.json> <artifact.bin> <public-key-hex>",
            )?;
            let public_key_hex = args.get(4).ok_or(
                "usage: watchctl verify-update-manifest <manifest.json> <artifact.bin> <public-key-hex>",
            )?;
            verify_update_manifest(manifest_path, artifact_path, public_key_hex)
        }
        _ => Err(String::from(
            "usage: watchctl <sdk|init-manifest|validate-manifest|init-update-manifest|sign-update-manifest|verify-update-manifest> [args]",
        )),
    }
}

fn print_sdk() {
    println!("{{");
    println!("  \"api_version\": {},", APP_API_VERSION);
    println!(
        "  \"lifecycle\": \"{}\",",
        AppLifecycle::Foreground.as_str()
    );
    println!("  \"capabilities\": [");
    for (index, (name, capability)) in APP_CAPABILITY_NAMES.iter().enumerate() {
        let suffix = if index + 1 == APP_CAPABILITY_NAMES.len() {
            ""
        } else {
            ","
        };
        println!(
            "    {{ \"name\": \"{}\", \"bit\": {} }}{}",
            name,
            capability.bits(),
            suffix
        );
    }
    println!("  ],");
    println!("  \"update\": {{");
    println!("    \"manifest_version\": {},", UPDATE_MANIFEST_VERSION);
    println!(
        "    \"signature_algorithm\": \"{}\",",
        UPDATE_SIGNATURE_ALGORITHM
    );
    println!("    \"kinds\": [");
    for (index, (name, _)) in UPDATE_KIND_NAMES.iter().enumerate() {
        let suffix = if index + 1 == UPDATE_KIND_NAMES.len() {
            ""
        } else {
            ","
        };
        println!("      \"{}\"{}", name, suffix);
    }
    println!("    ]");
    println!("  }}");
    println!("}}");
}

fn print_manifest_template(app_id: &str, display_name: &str) {
    let default_policy = AppSandboxPolicy {
        tick_ms: 100,
        capabilities: AppCapabilities::NONE,
    };
    println!("{{");
    println!("  \"app_id\": \"{}\",", escape_json(app_id));
    println!("  \"display_name\": \"{}\",", escape_json(display_name));
    println!("  \"api_version\": {},", APP_API_VERSION);
    println!(
        "  \"lifecycle\": \"{}\",",
        AppLifecycle::Foreground.as_str()
    );
    println!("  \"tick_ms\": {},", default_policy.tick_ms);
    println!("  \"capabilities\": []");
    println!("}}");
}

fn validate_manifest(path: &str) -> Result<(), String> {
    let manifest_bytes = fs::read(path).map_err(|err| format!("read {path}: {err}"))?;
    let manifest: PackageManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|err| format!("parse {path}: {err}"))?;

    if manifest.app_id.trim().is_empty() {
        return Err(String::from("app_id must not be empty"));
    }
    if manifest.display_name.trim().is_empty() {
        return Err(String::from("display_name must not be empty"));
    }
    if manifest.api_version != APP_API_VERSION {
        return Err(format!(
            "api_version {} does not match stable API {}",
            manifest.api_version, APP_API_VERSION
        ));
    }
    if manifest.lifecycle != AppLifecycle::Foreground.as_str() {
        return Err(format!(
            "unsupported lifecycle {:?}; expected {}",
            manifest.lifecycle,
            AppLifecycle::Foreground.as_str()
        ));
    }
    if manifest.tick_ms == 0 || manifest.tick_ms > 5_000 {
        return Err(format!("tick_ms {} is outside 1..=5000", manifest.tick_ms));
    }

    let mut seen = AppCapabilities::NONE;
    for capability_name in &manifest.capabilities {
        let capability = APP_CAPABILITY_NAMES
            .iter()
            .find_map(|(name, value)| (*name == capability_name).then_some(*value))
            .ok_or_else(|| format!("unknown capability {:?}", capability_name))?;
        if seen.contains(capability) {
            return Err(format!("duplicate capability {:?}", capability_name));
        }
        seen = seen.union(capability);
    }

    let _policy = AppSandboxPolicy {
        tick_ms: manifest.tick_ms,
        capabilities: seen,
    };

    println!(
        "manifest ok: {} ({}) api={} lifecycle={} tick_ms={} capabilities={}",
        manifest.display_name,
        manifest.app_id,
        manifest.api_version,
        manifest.lifecycle,
        manifest.tick_ms,
        manifest.capabilities.len(),
    );
    Ok(())
}

fn print_update_manifest_template(
    kind: &str,
    artifact_name: &str,
    version: &str,
) -> Result<(), String> {
    let manifest = UpdateManifest {
        manifest_version: UPDATE_MANIFEST_VERSION,
        kind: UpdateKind::parse(kind)
            .ok_or_else(|| format!("unsupported update kind {kind:?}"))?
            .as_str()
            .to_string(),
        artifact_name: artifact_name.to_string(),
        version: version.to_string(),
        sequence: 1,
        rollback_floor: 0,
        artifact_size: 0,
        artifact_sha256: String::new(),
        signature_algorithm: UPDATE_SIGNATURE_ALGORITHM.to_string(),
        key_id: String::from("dev"),
        signature_hex: String::new(),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&manifest).map_err(|err| err.to_string())?
    );
    Ok(())
}

fn sign_update_manifest(
    manifest_path: &str,
    artifact_path: &str,
    secret_key_hex: &str,
    output: Option<&str>,
) -> Result<(), String> {
    let manifest_bytes =
        fs::read(manifest_path).map_err(|err| format!("read {manifest_path}: {err}"))?;
    let manifest: UpdateManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|err| format!("parse {manifest_path}: {err}"))?;
    let artifact_bytes =
        fs::read(artifact_path).map_err(|err| format!("read {artifact_path}: {err}"))?;
    let (signed_manifest, public_key_hex) =
        sign_update_manifest_in_memory(manifest, &artifact_bytes, secret_key_hex)?;
    let json = serde_json::to_string_pretty(&signed_manifest).map_err(|err| err.to_string())?;
    write_output(&json, output)?;
    eprintln!("watchctl: signed update manifest with public_key_hex={public_key_hex}");
    Ok(())
}

fn verify_update_manifest(
    manifest_path: &str,
    artifact_path: &str,
    public_key_hex: &str,
) -> Result<(), String> {
    let manifest_bytes =
        fs::read(manifest_path).map_err(|err| format!("read {manifest_path}: {err}"))?;
    let manifest: UpdateManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|err| format!("parse {manifest_path}: {err}"))?;
    let artifact_bytes =
        fs::read(artifact_path).map_err(|err| format!("read {artifact_path}: {err}"))?;
    let kind = verify_signed_update_manifest(&manifest, &artifact_bytes, public_key_hex)?;
    println!(
        "update manifest ok: kind={} version={} sequence={} rollback_floor={} artifact={} bytes",
        kind.as_str(),
        manifest.version,
        manifest.sequence,
        manifest.rollback_floor,
        manifest.artifact_size,
    );
    Ok(())
}

fn sign_update_manifest_in_memory(
    mut manifest: UpdateManifest,
    artifact_bytes: &[u8],
    secret_key_hex: &str,
) -> Result<(UpdateManifest, String), String> {
    let _kind = validate_update_manifest_fields(&manifest, false)?;
    let secret_key = SigningKey::from_bytes(&parse_hex_exact::<32>(secret_key_hex)?);
    let verifying_key = VerifyingKey::from(&secret_key);
    manifest.artifact_size = artifact_bytes.len() as u64;
    manifest.artifact_sha256 = sha256_hex(artifact_bytes);
    manifest.signature_algorithm = UPDATE_SIGNATURE_ALGORITHM.to_string();
    manifest.signature_hex = encode_hex(
        &secret_key
            .sign(&update_signing_message(&manifest))
            .to_bytes(),
    );
    Ok((manifest, encode_hex(&verifying_key.to_bytes())))
}

fn verify_signed_update_manifest(
    manifest: &UpdateManifest,
    artifact_bytes: &[u8],
    public_key_hex: &str,
) -> Result<UpdateKind, String> {
    let kind = validate_update_manifest_fields(manifest, true)?;
    if manifest.artifact_size != artifact_bytes.len() as u64 {
        return Err(format!(
            "artifact_size {} does not match actual size {}",
            manifest.artifact_size,
            artifact_bytes.len()
        ));
    }
    let actual_sha256 = sha256_hex(artifact_bytes);
    if manifest.artifact_sha256 != actual_sha256 {
        return Err(format!(
            "artifact_sha256 mismatch: manifest={} actual={}",
            manifest.artifact_sha256, actual_sha256
        ));
    }
    let verifying_key = VerifyingKey::from_bytes(&parse_hex_exact::<32>(public_key_hex)?)
        .map_err(|err| format!("invalid public key: {err}"))?;
    let signature = Signature::from_bytes(&parse_hex_exact::<64>(&manifest.signature_hex)?);
    verifying_key
        .verify(&update_signing_message(manifest), &signature)
        .map_err(|err| format!("signature verification failed: {err}"))?;
    Ok(kind)
}

fn validate_update_manifest_fields(
    manifest: &UpdateManifest,
    require_signature: bool,
) -> Result<UpdateKind, String> {
    if manifest.manifest_version != UPDATE_MANIFEST_VERSION {
        return Err(format!(
            "manifest_version {} does not match supported version {}",
            manifest.manifest_version, UPDATE_MANIFEST_VERSION
        ));
    }
    let kind = UpdateKind::parse(&manifest.kind)
        .ok_or_else(|| format!("unsupported update kind {:?}", manifest.kind))?;
    if manifest.artifact_name.trim().is_empty() {
        return Err(String::from("artifact_name must not be empty"));
    }
    if manifest.version.trim().is_empty() {
        return Err(String::from("version must not be empty"));
    }
    if manifest.sequence == 0 {
        return Err(String::from("sequence must be greater than zero"));
    }
    if manifest.rollback_floor > manifest.sequence {
        return Err(format!(
            "rollback_floor {} cannot exceed sequence {}",
            manifest.rollback_floor, manifest.sequence
        ));
    }
    if manifest.signature_algorithm != UPDATE_SIGNATURE_ALGORITHM {
        return Err(format!(
            "signature_algorithm {:?} does not match supported {}",
            manifest.signature_algorithm, UPDATE_SIGNATURE_ALGORITHM
        ));
    }
    if manifest.key_id.trim().is_empty() {
        return Err(String::from("key_id must not be empty"));
    }
    if require_signature {
        if manifest.artifact_size == 0 {
            return Err(String::from("artifact_size must be greater than zero"));
        }
        let _ = parse_hex_exact::<32>(&manifest.artifact_sha256)
            .map_err(|_| String::from("artifact_sha256 must be 64 hex characters"))?;
        let _ = parse_hex_exact::<64>(&manifest.signature_hex)
            .map_err(|_| String::from("signature_hex must be 128 hex characters"))?;
    }
    Ok(kind)
}

fn update_signing_message(manifest: &UpdateManifest) -> Vec<u8> {
    format!(
        "watch-update-v1\nmanifest_version={}\nkind={}\nartifact_name={}\nversion={}\nsequence={}\nrollback_floor={}\nartifact_size={}\nartifact_sha256={}\nsignature_algorithm={}\nkey_id={}\n",
        manifest.manifest_version,
        manifest.kind,
        manifest.artifact_name,
        manifest.version,
        manifest.sequence,
        manifest.rollback_floor,
        manifest.artifact_size,
        manifest.artifact_sha256,
        manifest.signature_algorithm,
        manifest.key_id,
    )
    .into_bytes()
}

fn write_output(text: &str, output: Option<&str>) -> Result<(), String> {
    if let Some(path) = output {
        fs::write(path, text).map_err(|err| format!("write {path}: {err}"))
    } else {
        println!("{text}");
        Ok(())
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    encode_hex(&digest)
}

fn parse_hex_exact<const N: usize>(text: &str) -> Result<[u8; N], String> {
    let normalized = text.strip_prefix("0x").unwrap_or(text);
    if normalized.len() != N * 2 {
        return Err(format!(
            "expected {} hex characters, got {}",
            N * 2,
            normalized.len()
        ));
    }
    let mut out = [0u8; N];
    for (index, chunk) in normalized.as_bytes().chunks_exact(2).enumerate() {
        out[index] = (hex_nibble(chunk[0])? << 4) | hex_nibble(chunk[1])?;
    }
    Ok(out)
}

fn hex_nibble(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(format!("invalid hex byte {}", byte as char)),
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(nibble_to_hex(byte >> 4));
        out.push(nibble_to_hex(byte & 0x0f));
    }
    out
}

fn nibble_to_hex(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'a' + value - 10) as char,
    }
}

fn escape_json(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET_KEY_HEX: &str = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";

    fn sample_update_manifest() -> UpdateManifest {
        UpdateManifest {
            manifest_version: UPDATE_MANIFEST_VERSION,
            kind: String::from("firmware"),
            artifact_name: String::from("waveshare-watch-rs.bin"),
            version: String::from("0.4.0"),
            sequence: 7,
            rollback_floor: 5,
            artifact_size: 0,
            artifact_sha256: String::new(),
            signature_algorithm: UPDATE_SIGNATURE_ALGORITHM.to_string(),
            key_id: String::from("dev"),
            signature_hex: String::new(),
        }
    }

    #[test]
    fn update_manifest_sign_and_verify_round_trip() {
        let artifact = b"firmware-image".to_vec();
        let (signed, public_key_hex) =
            sign_update_manifest_in_memory(sample_update_manifest(), &artifact, SECRET_KEY_HEX)
                .expect("sign manifest");
        assert_eq!(signed.artifact_size, artifact.len() as u64);
        assert_eq!(signed.artifact_sha256, sha256_hex(&artifact));
        verify_signed_update_manifest(&signed, &artifact, &public_key_hex)
            .expect("verify manifest");
    }

    #[test]
    fn tampered_artifact_is_rejected() {
        let artifact = b"firmware-image".to_vec();
        let tampered = b"firmware-imagf".to_vec();
        let (signed, public_key_hex) =
            sign_update_manifest_in_memory(sample_update_manifest(), &artifact, SECRET_KEY_HEX)
                .expect("sign manifest");
        let err = verify_signed_update_manifest(&signed, &tampered, &public_key_hex)
            .expect_err("tampered artifact should fail");
        assert!(err.contains("artifact_sha256 mismatch"));
    }

    #[test]
    fn rollback_floor_cannot_exceed_sequence() {
        let mut manifest = sample_update_manifest();
        manifest.rollback_floor = manifest.sequence + 1;
        let err = validate_update_manifest_fields(&manifest, false)
            .expect_err("invalid rollback floor should fail");
        assert!(err.contains("rollback_floor"));
    }

    #[test]
    fn init_update_template_rejects_unknown_kind() {
        let err = print_update_manifest_template("bootloader", "artifact.bin", "1.0.0")
            .expect_err("unknown kind should fail");
        assert!(err.contains("unsupported update kind"));
    }
}
