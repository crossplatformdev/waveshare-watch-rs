use std::env;
use std::fs;
use std::process::ExitCode;

use serde::Deserialize;

#[path = "../../../src/app_sdk.rs"]
mod app_sdk;

use app_sdk::{AppCapabilities, AppLifecycle, AppSandboxPolicy, APP_API_VERSION, APP_CAPABILITY_NAMES};

#[derive(Deserialize)]
struct PackageManifest {
    app_id: String,
    display_name: String,
    api_version: u16,
    lifecycle: String,
    tick_ms: u16,
    capabilities: Vec<String>,
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
            let app_id = args.get(2).ok_or("usage: watchctl init-manifest <app-id> <display-name>")?;
            let display_name = args.get(3).ok_or("usage: watchctl init-manifest <app-id> <display-name>")?;
            print_manifest_template(app_id, display_name);
            Ok(())
        }
        Some("validate-manifest") => {
            let path = args.get(2).ok_or("usage: watchctl validate-manifest <manifest.json>")?;
            validate_manifest(path)
        }
        _ => Err(String::from(
            "usage: watchctl <sdk|init-manifest|validate-manifest> [args]",
        )),
    }
}

fn print_sdk() {
    println!("{{");
    println!("  \"api_version\": {},", APP_API_VERSION);
    println!("  \"lifecycle\": \"{}\",", AppLifecycle::Foreground.as_str());
    println!("  \"capabilities\": [");
    for (index, (name, capability)) in APP_CAPABILITY_NAMES.iter().enumerate() {
        let suffix = if index + 1 == APP_CAPABILITY_NAMES.len() { "" } else { "," };
        println!(
            "    {{ \"name\": \"{}\", \"bit\": {} }}{}",
            name,
            capability.bits(),
            suffix
        );
    }
    println!("  ]");
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
    println!("  \"lifecycle\": \"{}\",", AppLifecycle::Foreground.as_str());
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
