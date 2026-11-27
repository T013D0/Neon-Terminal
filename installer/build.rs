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

    // Archivos que van junto a neon.exe (ConPTY de Windows Terminal), si están.
    // Los deja scripts/get-conpty.ps1 en la misma carpeta que neon.exe.
    let dir = payload.parent().unwrap().to_path_buf();
    let mut extra = String::from("static EXTRA: &[(&str, &[u8])] = &[\n");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        for name in ["conpty.dll", "OpenConsole.exe"] {
            let f = dir.join(name);
            println!("cargo:rerun-if-changed={}", f.display());
            if f.is_file() {
                extra += &format!("    ({name:?}, include_bytes!({:?})),\n", f.display().to_string());
            } else {
                println!("cargo:warning=Sin {name}: Neon usará el ConPTY de Windows (corre scripts/get-conpty.ps1)");
            }
        }
    }
    extra += "];\n";
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("extra.rs");
    std::fs::write(out, extra).unwrap();

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
