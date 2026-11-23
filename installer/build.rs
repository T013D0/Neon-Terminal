//! Mete neon.exe (ya compilado en release) dentro del instalador.
//!
//! Por defecto lo busca en ../target/release/neon(.exe). Se puede indicar otra
//! ruta con la variable de entorno NEON_EXE.
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let exe_name = if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        "neon.exe"
    } else {
        "neon"
    };
    let payload = std::env::var("NEON_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            manifest
                .join("..")
                .join("target")
                .join("release")
                .join(exe_name)
        });
    if !payload.is_file() {
        panic!(
            "\n\nNo encontré {}.\nCompila Neon primero:  cargo build --release -p neon\n\
             (o usa el script build-installer.ps1, que hace todo)\n\n",
            payload.display()
        );
    }
    let payload = payload.canonicalize().unwrap();
    println!("cargo:rustc-env=NEON_PAYLOAD={}", payload.display());
    println!("cargo:rerun-if-changed={}", payload.display());
    println!("cargo:rerun-if-env-changed=NEON_EXE");

    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../assets/neon.ico")
            .set("ProductName", "Neon")
            .set("FileDescription", "Instalador de Neon");
        if let Err(e) = res.compile() {
            println!("cargo:warning=No se pudo incrustar el ícono: {e}");
        }
    }
}
