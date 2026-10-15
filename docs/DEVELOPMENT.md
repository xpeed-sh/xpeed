# Desarrollo de Xpeed

## Atajos

| Atajo | Acción |
| --- | --- |
| Ctrl+Shift+T | Nueva terminal local |
| Ctrl+Shift+N | Nueva conexión |
| Ctrl+Shift+D | Dividir en columnas |
| Ctrl+Shift+E | Dividir en filas |
| Ctrl+Shift+W | Cerrar la terminal enfocada |
| Ctrl+Tab / Ctrl+Shift+Tab | Cambiar de terminal |
| Ctrl+Shift+P | Acciones rápidas |
| Ctrl+Shift+F | Buscar en la terminal y su historial |
| Ctrl+Shift+B | Mostrar/ocultar el panel lateral |
| Ctrl+, | Ajustes |
| Ctrl+Shift+C | Copiar selección |
| Ctrl+V / Ctrl+Shift+V | Pegar |
| Ctrl+C | Interrumpir el comando |
| Shift+PageUp / Shift+PageDown | Desplazar historial |
| Esc | Cerrar búsqueda o ventanas de ajustes/acciones |

Haz clic en una terminal para dirigir allí el teclado. Cerrar una terminal termina su proceso. La búsqueda resalta el texto de la pantalla visible, incluido el historial al desplazarse.

## Desarrollo

```powershell
.\scripts\dev.ps1
.\scripts\dev.ps1 -Test
.\scripts\dev.ps1 -Release
.\scripts\package.ps1
```

En este equipo Rust está instalado en `.tools`, sin modificar el PATH del usuario. Los scripts detectan esa instalación y las herramientas GNU en `C:\msys64\mingw64\bin`. En otro equipo se necesita Rust y un compilador compatible; MSVC con Visual Studio Build Tools también es válido.

El icono de Windows se compila con `windres` (GNU) o `rc.exe` del Windows SDK (MSVC), disponibles en el PATH. La imagen y los iconos derivados están en [`assets/img`](../assets/img/README.md); no se requiere Python para compilar.

En Linux/macOS:

```sh
cargo run --release
cargo test --workspace --locked
cargo run -- --smoke-test
```

En Ubuntu se necesitan las dependencias de ventana y gráficos de eframe, por ejemplo: `build-essential pkg-config libx11-dev libxi-dev libxcursor-dev libxrandr-dev libxinerama-dev libgl1-mesa-dev libegl1-mesa-dev libwayland-dev libxkbcommon-dev`. En macOS se necesitan las Command Line Tools de Xcode.

El proyecto tiene un flujo de CI para Windows, Ubuntu y macOS. La validación local de esta entrega se realizó en Windows; todavía no hay paquetes validados en los otros dos sistemas.

## Arquitectura y alcance

La interfaz usa egui/eframe con OpenGL y egui_tiles para las divisiones y el arrastre. portable-pty conecta con ConPTY en Windows y PTY en Unix. alacritty_terminal interpreta ANSI, colores 16/256/RGB, pantalla alternativa y el historial. La lectura/escritura de terminal, la navegación SFTP y las peticiones HTTP de IA se hacen fuera del hilo de interfaz.

`terminal.rs` administra los procesos y PTY; `terminal_view.rs` dibuja las celdas y recibe entrada; `workspace.rs` guarda el árbol de paneles; `connections.rs` gestiona perfiles; `palette.rs` define los temas; `history.rs` guarda comandos; `sftp.rs` implementa navegación y operaciones SFTP; `file_explorer.rs` organiza las vistas, selección y menús; `credentials.rs` integra el almacén de credenciales y el helper askpass de OpenSSH; `assistant.rs` conecta el chat HTTP; `src/extensions/` gestiona paquetes y procesos JSON-RPC; `app.rs` organiza la interfaz.

Los proyectos de extensiones no forman parte del repositorio principal. `extensions/` está ignorada para que puedas clonar allí `extension-codex`, `extension-server-monitor` y futuras extensiones sin duplicarlas en Git. El núcleo conserva únicamente manifiestos de prueba reducidos en `src/extensions/fixtures/`.

Los archivos `settings.json`, `connections.json`, `commands.json` y `workspace.json` se guardan en la carpeta de configuración del usuario, dentro de `xpeed`. En Windows normalmente es `%APPDATA%\xpeed`. Para comenzar con una distribución nueva usa `Xpeed-0.10.6.exe --fresh-layout`.

Quedan pendientes protocolo de ratón para aplicaciones TUI, validación amplia de tipografía compleja/emoji/IME y empaquetadores Linux/macOS. Las pruebas verifican inicio sin procesos, consolas, divisiones, arrastre, captura de comandos, intercambio SFTP, acceso SSH por contraseña y reutilización del acceso, almacenamiento/borrado en el almacén de Windows y chat HTTP con servidores/protocolos de prueba. No validan tus servidores externos ni una respuesta de un modelo de IA real. Las comparativas de velocidad requieren mediciones con cargas representativas.

Para capturas del renderizado nativo:

```powershell
dist\Xpeed-0.10.6.exe --capture artifacts\dark.png
dist\Xpeed-0.10.6.exe --capture artifacts\light.png --light
dist\Xpeed-0.10.6.exe --capture artifacts\settings.png --show-settings
```

La carpeta de salida debe existir. `--theme 0`, `1` o `2` selecciona un tema del modo activo; las capturas usan preferencias predeterminadas y no cargan perfiles personales ni guardan cambios.
`--terminal` permite abrir una consola al iniciar; `--split` abre dos para revisar la distribución.

## Iconos

Los controles y el árbol usan vectores dibujados por la interfaz en `src/app.rs`: mantienen nitidez con distintas escalas y usan los colores del tema. No necesitan fuentes de iconos ni descargas. Para añadir una colección propia, SVG es un buen formato de origen; usa un `viewBox` uniforme (por ejemplo 24×24), sin textos ni recursos externos. El proyecto todavía no incluye un cargador de SVG arbitrarios: hay que adaptar sus trazos al dibujado vectorial o incorporar un renderizador antes de usarlos directamente.

## Contrato de extensiones

Consulta [docs/EXTENSIONS.md](EXTENSIONS.md) para desarrollar adaptadores y contribuciones de paneles. El uso de cada extensión se documenta en su propio proyecto.

## Capturas del README

Con el ejecutable compilado, ejecuta `scripts/capture-previews.ps1`. Genera capturas nativas en `assets/previews/` para los seis temas, conexión/archivos y extensiones, con datos ficticios y sin consultar perfiles ni conectar servidores reales.

## Archivos remotos

El menú del explorador permite subir varios archivos a la ruta actual. Se procesan uno a uno por SFTP con buffers acotados, sin un límite de tamaño del producto; dependen del espacio disponible, permisos y conexión. Se muestra archivo actual, cantidad y bytes transferidos. Una colisión o error detiene la cola; los archivos previos completados permanecen. Cancelar no deshace lo ya completado. Se pueden seguir arrastrando archivos al explorador.

Comprimir y extraer son trabajos separados por SSH sobre el servidor actual. Requieren `python3` y shell POSIX. ZIP/TAR usan la biblioteca estándar; crear RAR requiere `rar`, y extraer RAR requiere el módulo Python `rarfile` y `unrar` o `unar`. No se instalan herramientas automáticamente. Se rechazan enlaces y entradas especiales; extraer crea una carpeta nueva sin sobrescribir. Un error o cancelación puede dejar una carpeta parcial, y cerrar el transporte no garantiza detener todos los procesos remotos. La compresión usa un archivo temporal y publica con un enlace duro exclusivo, que debe admitir el sistema de archivos remoto.

Pruebas aisladas del motor: `py -3.12 scripts/test-remote-archive.py`. Las pruebas SSH/SFTP están en `scripts/test-session-tools.py`.
