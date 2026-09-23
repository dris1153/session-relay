const BUILD_ENV_KEYS: [&str; 2] = ["SR_GITHUB_CLIENT_ID", "SR_GITHUB_APP_SLUG"];
const ENV_FILE: &str = "../.env";

fn main() {
    load_build_env();
    tauri_build::build()
}

// Real environment variables win over `../.env` so CI can inject values.
fn load_build_env() {
    let file = read_env_file();
    if file.is_some() {
        // Watching a missing file would make cargo rerun this script on every build.
        println!("cargo:rerun-if-changed={ENV_FILE}");
    }
    for key in BUILD_ENV_KEYS {
        println!("cargo:rerun-if-env-changed={key}");
        let value = std::env::var(key)
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or_else(|| file.as_deref().and_then(|f| lookup(f, key)));
        match value {
            Some(v) if is_valid_value(&v) => println!("cargo:rustc-env={key}={v}"),
            Some(_) => println!("cargo:warning={key} has invalid characters; expected letters, digits and '-'"),
            None => println!("cargo:warning={key} is not set (see .env.example); GitHub login will be unavailable"),
        }
    }
}

// Tolerates the UTF-16 files that Windows PowerShell 5.1 `>` writes and a UTF-8 BOM.
fn read_env_file() -> Option<String> {
    let bytes = std::fs::read(ENV_FILE).ok()?;
    let text = match bytes.as_slice() {
        [0xFF, 0xFE, rest @ ..] => {
            let units: Vec<u16> = rest.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
            String::from_utf16_lossy(&units)
        }
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        _ => String::from_utf8_lossy(&bytes).into_owned(),
    };
    Some(text)
}

fn lookup(file: &str, key: &str) -> Option<String> {
    file.lines()
        .map(|line| line.trim().trim_start_matches("export ").trim())
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split_once('='))
        .find(|(k, _)| k.trim() == key)
        .map(|(_, v)| v.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|v| !v.is_empty())
}

fn is_valid_value(value: &str) -> bool {
    value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}
