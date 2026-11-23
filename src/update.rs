//! Actualizaciones desde GitHub Releases.
//!
//! Consulta `releases/latest` del repositorio configurado, compara la versión con
//! la instalada y, si hay una más nueva, descarga `NeonSetup.exe` de ese release
//! (verificando su SHA-256 cuando GitHub lo informa) y lo lanza con `--auto`:
//! el instalador reemplaza a Neon sin preguntar y vuelve a abrirlo.
//!
//! Usa `curl` (viene con Windows 10 y 11) para no sumar un cliente HTTP ni su TLS.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::Sender;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Repositorio de GitHub donde se publican los releases. Fijo: no se puede cambiar.
pub const DEFAULT_REPO: &str = "T013D0/Neon-Terminal";

/// Una versión publicada.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub page: String,
    pub asset_name: String,
    pub asset_url: String,
    /// SHA-256 en hexadecimal, si GitHub lo informó.
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum UpdateState {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    /// Descargando la versión indicada.
    Downloading(String),
    Error(String),
}

/// Respuestas de los hilos de fondo.
pub enum UpdateMsg {
    Checked {
        result: Result<Option<Release>, String>,
        /// La pidió el usuario (si no, es el chequeo automático: los errores no se muestran).
        manual: bool,
    },
    Downloaded(Result<PathBuf, String>),
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

/// "v1.2.3" / "1.2.3-beta" → (1, 2, 3).
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let core = s.trim().trim_start_matches(['v', 'V']);
    let core = core.split(['-', '+']).next()?;
    let mut it = core.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next().map_or(Some(0), |p| p.parse().ok())?;
    let patch = it.next().map_or(Some(0), |p| p.parse().ok())?;
    Some((major, minor, patch))
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// `usuario/repositorio`, con los caracteres que permite GitHub.
pub fn valid_repo(repo: &str) -> bool {
    let mut parts = repo.split('/');
    let ok = |p: &str| {
        !p.is_empty()
            && !p.chars().all(|c| c == '.')
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    matches!((parts.next(), parts.next(), parts.next()), (Some(a), Some(b), None) if ok(a) && ok(b))
}

/// Para pruebas: `NEON_UPDATE_URL` reemplaza la dirección de la API de GitHub.
pub fn override_url() -> Option<String> {
    std::env::var("NEON_UPDATE_URL")
        .ok()
        .filter(|u| !u.is_empty())
}

fn api_url(repo: &str) -> Result<String, String> {
    if let Some(u) = override_url() {
        return Ok(u);
    }
    if !valid_repo(repo) {
        return Err("Escribe el repositorio como usuario/nombre.".into());
    }
    Ok(format!(
        "https://api.github.com/repos/{repo}/releases/latest"
    ))
}

fn curl() -> Command {
    let mut c = Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // sin ventana de consola
    }
    c.args(["--fail", "--silent", "--show-error", "--location"]);
    c
}

fn curl_error(out: &std::process::Output) -> String {
    let e = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if e.contains("404") {
        "No hay releases publicados en ese repositorio.".into()
    } else if e.is_empty() {
        "No pude conectarme a GitHub.".into()
    } else {
        format!("No pude conectarme a GitHub: {e}")
    }
}

/// Solo se descarga desde GitHub (o desde localhost si se está probando).
fn url_allowed(url: &str) -> bool {
    url.starts_with("https://github.com/")
        || (override_url().is_some() && url.starts_with("http://127.0.0.1"))
}

/// Interpreta la respuesta de la API. `None` = no hay nada más nuevo.
pub fn parse_release(json: &str, current: &str) -> Result<Option<Release>, String> {
    let r: ApiRelease = serde_json::from_str(json)
        .map_err(|_| "La respuesta de GitHub no se entiende.".to_string())?;
    let version = r.tag_name.trim_start_matches(['v', 'V']).to_string();
    if !is_newer(&version, current) {
        return Ok(None);
    }
    let asset = r
        .assets
        .iter()
        .find(|a| a.name.to_lowercase().starts_with("neonsetup"))
        .ok_or_else(|| format!("La versión {version} no trae el instalador (NeonSetup)."))?;
    if !url_allowed(&asset.browser_download_url) {
        return Err("El instalador no está alojado en GitHub; no lo descargo.".into());
    }
    Ok(Some(Release {
        version,
        notes: r.body.unwrap_or_default().trim().to_string(),
        page: r.html_url,
        asset_name: asset.name.clone(),
        asset_url: asset.browser_download_url.clone(),
        sha256: asset
            .digest
            .as_deref()
            .and_then(|d| d.strip_prefix("sha256:"))
            .map(|d| d.to_lowercase()),
    }))
}

/// Consulta GitHub (bloquea: llamar desde un hilo).
pub fn fetch_latest(repo: &str) -> Result<Option<Release>, String> {
    let url = api_url(repo)?;
    let out = curl()
        .args(["--max-time", "15"])
        .args(["-H", "Accept: application/vnd.github+json"])
        .args(["-H", "User-Agent: neon-terminal"])
        .arg(&url)
        .output()
        .map_err(|_| "No encontré curl en este equipo.".to_string())?;
    if !out.status.success() {
        return Err(curl_error(&out));
    }
    parse_release(&String::from_utf8_lossy(&out.stdout), VERSION)
}

pub fn sha256_hex(path: &std::path::Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    std::io::copy(&mut f, &mut h)?;
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Descarga el instalador a una carpeta temporal y comprueba su SHA-256.
pub fn download(r: &Release) -> Result<PathBuf, String> {
    if !url_allowed(&r.asset_url) {
        return Err("Dirección de descarga no permitida.".into());
    }
    let dir = std::env::temp_dir().join("neon-update");
    std::fs::create_dir_all(&dir).map_err(|e| format!("No pude crear la carpeta temporal: {e}"))?;
    // Solo el nombre del archivo: nada de rutas que vengan de afuera.
    let name = std::path::Path::new(&r.asset_name)
        .file_name()
        .map(|n| n.to_owned())
        .ok_or("Nombre de archivo inválido.")?;
    let path = dir.join(name);
    let _ = std::fs::remove_file(&path);
    let out = curl()
        .args(["--max-time", "600", "-o"])
        .arg(&path)
        .arg(&r.asset_url)
        .output()
        .map_err(|_| "No encontré curl en este equipo.".to_string())?;
    if !out.status.success() {
        let _ = std::fs::remove_file(&path);
        return Err(curl_error(&out));
    }
    if std::fs::metadata(&path).map_or(0, |m| m.len()) == 0 {
        return Err("La descarga llegó vacía.".into());
    }
    if let Some(expected) = &r.sha256 {
        let got = sha256_hex(&path).map_err(|e| format!("No pude leer lo descargado: {e}"))?;
        if &got != expected {
            let _ = std::fs::remove_file(&path);
            return Err("La descarga no coincide con el SHA-256 de GitHub; la descarté.".into());
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
    }
    Ok(path)
}

/// Lanza el instalador descargado en modo automático.
pub fn launch_installer(path: &std::path::Path) -> std::io::Result<()> {
    let mut c = Command::new(path);
    c.arg("--auto");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0000_0008); // DETACHED_PROCESS: sigue vivo cuando Neon se cierra
    }
    c.spawn().map(|_| ())
}

pub fn check_async(repo: String, manual: bool, tx: Sender<UpdateMsg>, ctx: egui::Context) {
    std::thread::spawn(move || {
        let result = fetch_latest(&repo);
        let _ = tx.send(UpdateMsg::Checked { result, manual });
        ctx.request_repaint();
    });
}

pub fn download_async(r: Release, tx: Sender<UpdateMsg>, ctx: egui::Context) {
    std::thread::spawn(move || {
        let _ = tx.send(UpdateMsg::Downloaded(download(&r)));
        ctx.request_repaint();
    });
}

// ── Chequeo automático: como mucho una vez al día ──

fn stamp_path() -> PathBuf {
    crate::shell::data_dir().join("update-check.txt")
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub fn auto_check_due() -> bool {
    if override_url().is_some() {
        return true; // pruebas
    }
    let last = std::fs::read_to_string(stamp_path())
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);
    now_secs().saturating_sub(last) >= 24 * 3600
}

pub fn mark_checked() {
    let _ = std::fs::create_dir_all(crate::shell::data_dir());
    let _ = std::fs::write(stamp_path(), now_secs().to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("0.10"), Some((0, 10, 0)));
        assert_eq!(parse_version("2.0.0-beta.1"), Some((2, 0, 0)));
        assert_eq!(parse_version("hola"), None);
        assert!(is_newer("v0.2.0", "0.1.9"));
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.0.9", "0.1.0"));
        assert!(!is_newer("basura", "0.1.0"));
    }

    #[test]
    fn default_repo_is_valid() {
        assert!(valid_repo(DEFAULT_REPO));
    }

    #[test]
    fn repo_names() {
        assert!(valid_repo("usuario/neon"));
        assert!(valid_repo("a-b/c_d.e"));
        assert!(!valid_repo("neon"));
        assert!(!valid_repo("a/b/c"));
        assert!(!valid_repo("a b/c"));
        assert!(!valid_repo("../x"));
        assert!(!valid_repo("/"));
    }

    const SAMPLE: &str = r#"{
        "tag_name": "v0.3.0", "body": "Novedades", "html_url": "https://github.com/u/neon/releases/tag/v0.3.0",
        "assets": [
          {"name": "notas.txt", "browser_download_url": "https://github.com/u/neon/releases/download/v0.3.0/notas.txt"},
          {"name": "NeonSetup.exe", "browser_download_url": "https://github.com/u/neon/releases/download/v0.3.0/NeonSetup.exe",
           "digest": "sha256:ABCDEF"}
        ]}"#;

    #[test]
    fn release_parsing() {
        let r = parse_release(SAMPLE, "0.1.0").unwrap().unwrap();
        assert_eq!(r.version, "0.3.0");
        assert_eq!(r.asset_name, "NeonSetup.exe");
        assert_eq!(r.sha256.as_deref(), Some("abcdef"));
        // Ya estamos al día.
        assert_eq!(parse_release(SAMPLE, "0.3.0").unwrap(), None);
        assert_eq!(parse_release(SAMPLE, "1.0.0").unwrap(), None);
    }

    #[test]
    fn rejects_foreign_hosts_and_missing_asset() {
        let evil = SAMPLE.replace(
            "https://github.com/u/neon/releases/download/v0.3.0/NeonSetup.exe",
            "https://evil.example/NeonSetup.exe",
        );
        assert!(parse_release(&evil, "0.1.0").is_err());
        let none = r#"{"tag_name":"v9.0.0","assets":[]}"#;
        assert!(parse_release(none, "0.1.0").is_err());
        assert!(parse_release("no es json", "0.1.0").is_err());
    }

    #[test]
    fn checksum() {
        let p = std::env::temp_dir().join("neon-sha-test.bin");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(
            sha256_hex(&p).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = std::fs::remove_file(p);
    }
}
