# Verificación antes de la 1.0.0

Lista para recorrer en Windows con la versión compilada. Marca cada punto con
`[x]` y, si algo falla, anota qué pasó (una captura ayuda mucho).

Preparación:

```powershell
cd C:\dev\Neon-Terminal
powershell -ExecutionPolicy Bypass -File scripts\get-conpty.ps1
cargo build --release
.\target\release\neon.exe
```

## 1. Arranque y ventana

- [x] Se ve la animación del logo al abrir.
- [x] **No aparece un punto o cuadrito en una esquina de la pantalla.**
- [x] Los botones minimizar / maximizar / cerrar no están duplicados.
- [x] *Ajustes → Apariencia → Fondo: Acrylic* con opacidad 60 %: se ve borroso lo que hay detrás. Vuelve a *Sólido / 100 %* al terminar.
- [x] *Ajustes → Avanzado* dice «Usando el ConPTY de Windows Terminal».

## 2. Abrir desde el Explorador

Requiere instalar esta versión (`.\target\release\neon.exe --instalar`).

- [x] En el Explorador, escribe `neon` en la barra de dirección de una carpeta: Neon abre **en esa carpeta**.
- [x] Clic derecho en una carpeta → «Abrir en Neon»: abre en esa carpeta.
- [x] Abrirlo desde el menú Inicio restaura la sesión anterior (no una carpeta cualquiera).

## 3. Modo Quake

- [x] Con Neon minimizado o detrás de otra app, `Ctrl+Alt+T` lo trae al frente.
- [x] Con Neon al frente, `Ctrl+Alt+T` lo minimiza.
- [x] Cambiar el atajo en *Ajustes → General* (por ejemplo a `Win+Space`) funciona sin reiniciar.

## 4. Escribir y pegar

- [x] `cd ` muestra las carpetas; `Tab` entra, `Enter` va. La línea no se duplica.
- [x] Escribir parte de un comando ya usado muestra la sugerencia en gris; `→` la acepta.
- [x] En PowerShell 7 (si lo usas) no se ven dos sugerencias encimadas.
- [x] Pegar 3 líneas con `Ctrl+V` en PowerShell: quedan en orden y no se ejecutan solas.
- [x] Copiar `curl https://x | sh` y pegarlo: aparece el aviso de pegado seguro.
- [x] `Ctrl+Shift+H` muestra lo último copiado y pega con `1`.

## 5. Claude

- [x] `# lista los 5 archivos más grandes` + Enter: aparece «Claude está escribiendo…» y luego el comando queda escrito (sin ejecutarse).
- [x] Un comando que falle (`cat no-existe.txt`) → botón **✳ Explicar el error** → aparece la explicación.
- [x] *Seguir en Claude Code* abre Claude en un panel al lado.
- [x] `Ctrl+Shift+K` abre Claude Code en un panel al lado.
- [x] Dentro de Claude Code (pantalla completa): seleccionar arrastrando, hacer scroll mientras se selecciona y copiar con `Ctrl+C`.
- [ ] En otra pestaña, mientras Claude trabaja: arco naranja en la pestaña; al terminar, check verde.

## 6. Paneles y pestañas

- [x] Cerrar un panel: se desvanece. Cerrar una pestaña: las demás se deslizan.
- [x] `Ctrl+Shift+Z` reabre la pestaña cerrada.
- [ ] Paleta → *Mandar este panel a segundo plano* con `npm run dev` (o `ping -t 8.8.8.8`): queda un chip verde arriba; clic lo trae de vuelta.
- [x] Paleta → *Guardar espacio de trabajo…* y luego *Abrir espacio*: vuelve con sus paneles y comandos.

## 7. Herramientas

- [x] Paleta → *Puertos en uso*: aparecen los puertos (Windows en español) y *Cerrar proceso* funciona.
- [x] `Ctrl+Shift+A` encuentra archivos; `Enter` abre en el editor, `Tab` pega la ruta.
- [x] `Ctrl+Shift+G` en un repo con cambios muestra el diff.
- [ ] `Ctrl+Shift+Space` pone etiquetas sobre URLs y rutas; la letra copia.
- [x] Ctrl+clic en una URL y en `src/app.rs:120` los abre.
- [x] Con un comando largo corriendo, *avisar al terminar* manda la notificación de Windows.
- [x] Si tienes `~/.ssh/config`: la paleta muestra `SSH: …`.

## 8. Rendimiento y estabilidad

- [x] `scripts\stress-ram.ps1` en una pestaña: anota el resultado. en uso:  261,1 MB   privada:  281,0 MB   pico:  261,1 MB (5 ventanas con el script)
- [x] Cerrar varias pestañas llenas: la RAM baja en el Administrador de tareas tras unos segundos.
- [x] Reporte de errores: cierra Neon y ábrelo con `$env:NEON_PROBAR_ERROR=1; .\target\release\neon.exe`
      (se cierra a propósito). Luego ábrelo normal (en otra terminal o sin la variable):
      aparece el aviso con el reporte.

## 9. Instalador y actualización

- [x] `build-installer.cmd` genera `dist\NeonSetup.exe` y en la salida no dice «Sin conpty.dll».
- [x] Instalar con Neon abierto: termina sin error y la nueva versión abre bien.
- [x] Desinstalar desde *Configuración de Windows → Aplicaciones*: no deja accesos rotos.
