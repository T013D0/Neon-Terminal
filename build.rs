//! Incrusta el ícono y los datos del programa en neon.exe (solo en Windows).
fn main() {
    println!("cargo:rerun-if-changed=assets/neon.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/neon.ico")
            .set("ProductName", "Neon")
            .set("FileDescription", "Neon")
            .set("CompanyName", "Neon");
        // Si falta el compilador de recursos, no frenamos el build: solo no hay ícono.
        if let Err(e) = res.compile() {
            println!("cargo:warning=No se pudo incrustar el ícono: {e}");
        }
    }
}
