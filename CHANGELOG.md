# Cambios

Todas las versiones de Neon. La más nueva va arriba.

## 1.0.0 — 2026-11-27

La primera versión estable. Desde la 0.2.4 cambió casi todo: Neon pasó de
ser una terminal bonita a una pensada para trabajar con Claude Code.

### Claude
- **`# pedido` + Enter**: Claude escribe el comando para tu shell y lo deja escrito para revisarlo (no se ejecuta solo). Usa tu Claude Code instalado, sin API key.
- **✳ Explicar el error**: en un comando que falló, Claude explica por qué y cómo arreglarlo; los comandos sugeridos se escriben con un clic y la conversación sigue en Claude Code.
- **`Ctrl+Shift+K`** abre Claude Code en un panel al lado.
- **Panel de cambios (`Ctrl+Shift+G`)**: archivos modificados con su diff, marcando los que cambió Claude.
- **Estados en las pestañas**: Claude trabajando, terminó o te necesita; también para cualquier comando (corriendo, terminó, falló).
- Selección dentro de Claude Code en pantalla completa: arrastrar, seguir seleccionando con scroll y copiar.

### Escribir y pegar
- **`cd `** muestra las carpetas para elegir con el teclado.
- **Sugerencias en gris** desde tu historial; `→` las acepta.
- **Pegado seguro**: confirma antes de pegar comandos riesgosos, caracteres invisibles o varias líneas que se ejecutarían.
- **Historial del portapapeles (`Ctrl+Shift+H`)**.
- Pegar varias líneas en PowerShell ya no las invierte.

### Bloques y navegación
- **Bloques de comandos**: barra de estado, botones para copiar comando o salida, el comando queda fijo arriba al hacer scroll.
- **Links clicables**, incluidos los hipervínculos que manda Claude Code y `archivo:línea`.
- **Selección rápida (`Ctrl+Shift+Space`)**: etiquetas sobre URLs, rutas y hashes para copiarlos con una letra.
- **Buscar archivo (`Ctrl+Shift+A`)** con búsqueda difusa.

### Pestañas, paneles y ventanas
- **Espacios de trabajo con nombre**: guarda pestañas, paneles y lo que corría, y ábrelos desde la paleta.
- **Segundo plano**: un panel sale de las pestañas y queda como un chip con su estado.
- **Reabrir la pestaña cerrada (`Ctrl+Shift+Z`)**.
- **Modo Quake**: `Ctrl+Alt+T` muestra u oculta Neon desde cualquier app.
- **`.neon.toml` por proyecto**: nombre y color de pestaña, y comandos del proyecto en la paleta.
- Animaciones al abrir y cerrar paneles y pestañas, y logo animado al arrancar.

### Herramientas
- **Puertos en uso**: qué programa ocupa cada puerto, con botón para cerrarlo.
- **Avisarme cuando termine** un comando largo.
- **SSH** desde la paleta, con los servidores de `~/.ssh/config`.

### Ayuda
- **Recorrido de bienvenida** en el primer arranque y **pantalla de atajos (`F1`)**.
- **Reporte de errores**: si Neon se cae, al abrir de nuevo ofrece copiar el reporte o abrir un issue (no se envía nada solo).

### Por dentro
- **Más rápido con mucha salida**: ya no copia todo el historial en cada cuadro (de ~15 ms a 2–7 ms de CPU por cuadro).
- **Menos RAM**: fuentes del sistema mapeadas en vez de copiadas (−44 MB al arrancar) y la memoria se devuelve al cerrar pestañas.
- **ConPTY de Windows Terminal** incluido en el instalador: pegado, colores y mouse más fieles.
- Mica y Acrylic dejan ver lo que hay detrás de la ventana.
- Abrir Neon desde la barra del Explorador abre en esa carpeta.
- Ya no aparece un punto en la esquina de la pantalla al abrir.
- Compilación y pruebas automáticas en cada push; firma de código lista para cuando haya certificado.

## 0.2.x

- 0.2.4: arreglos de selección y copiado en Windows.
- 0.2.0 – 0.2.3: integración con Claude Code (avisos, Shift+Enter, arrastrar archivos, pegar imágenes), selección y animaciones.

## 0.1.0

- Primera versión: pestañas, paneles, paleta de comandos, temas, historial y actualizaciones automáticas.
