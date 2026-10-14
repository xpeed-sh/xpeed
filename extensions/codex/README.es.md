# Codex

**Versión:** `0.3.1` · [Changelog](CHANGELOG.md) · [VERSION](VERSION)

## Conversaciones guardadas (0.3.1)

Xpeed 0.9.1 guarda chats, títulos editables, borradores, modelo y preferencia de acceso en `%APPDATA%/xpeed/extensions/.chat-state/xpeed.codex.json`. Al abrir Codex recupera la cuenta existente de ChatGPT y pasa directamente al listado. Al enviar en una conversación guardada retoma su hilo original.

Después de reiniciar selecciona las sesiones actuales de terminal. No se restauran tokens de autorización ni aprobaciones pendientes. El proveedor oficial sigue gestionando las credenciales. El uso de la cuenta aparece debajo del listado de conversaciones.

Extensión opcional **0.3.1** para **XPEED 0.9.1+**. Codex se ejecuta en tu equipo con acceso oficial de ChatGPT desde el navegador. No pide API key ni instala Codex en servidores SSH. [English](README.md).

## Instalar y usar

1. En **Extensiones**, instala **Codex** y pulsa **Abrir extensión**.
2. Si no encuentra el runtime, configura el ejecutable local en la ficha. Busca en PATH y en la extensión oficial de VS Code.
3. Pulsa **Conectar**, abre el enlace si necesitas iniciar sesión y completa el acceso oficial. Tras la bienvenida, pulsa **Continuar** y abre una conversación o comienza una nueva. XPEED 0.8.2+ usa este flujo; los hosts anteriores muestran los controles originales. El menú **…** del chat permite actualizar o cerrar sesión.
4. Abre una terminal local o SSH. En **Sesiones seleccionadas**, en la cabecera del chat, marca uno o varios destinos y revisa sus nombres, IDs e IPs.
5. Pide una comprobación: «En la sesión seleccionada, ejecuta pwd con directorio explícito e informa salida, errores y código de salida». Aprueba la tarjeta. También puedes autorizar comandos y cambios para toda esa tarea antes de enviarla.

Requiere **codex-cli 0.162.0-alpha.2 / 0.162.0-alpha.17.2**, versión cuyo esquema experimental se verificó. Otras versiones muestran un error de compatibilidad. El runtime oficial se instala aparte en tu equipo; el adaptador es un ejecutable Rust independiente.

La cuenta se guarda mediante el mecanismo oficial en un perfil propio: `%APPDATA%/xpeed/providers/codex-home` en Windows; `$XDG_DATA_HOME/xpeed/providers/codex-home`, normalmente `~/.local/share/...`, en Unix. No reutiliza otras instalaciones: inicia sesión una vez aquí. No añadas MCP, hooks ni otras integraciones a este perfil administrado. El puente no expone tokens. Modelos y límites se consultan al proveedor.

## Destinos y permisos

Cada conversación conserva las sesiones autorizadas. Cambiar de pestaña no cambia el destino; cambiar las casillas inicia otra conversación. Una desconexión o reconexión revoca el acceso anterior: comienza una conversación nueva. Archivos y salida de comandos no amplían permisos. Tareas y propuestas pertenecen a su sesión original.

Herramientas del host: xpeed_list_sessions, xpeed_exec, xpeed_read_terminal, xpeed_task_status, xpeed_cancel_task, xpeed_read_file, xpeed_list_directory, xpeed_propose_change y xpeed_apply_change. Ejecutan con sesión/directorio explícitos, consultan y cancelan tareas, leen/listan archivos y aplican propuestas por ID después de revisar diferencias.

Comandos y cambios requieren aprobación individual o para esa tarea. Las tarjetas muestran destino, comando, directorio, diferencias y resultados. Seleccionar una sesión autoriza lectura. Los comandos tienen los permisos de su usuario y pueden modificar archivos. Las herramientas de archivos excluyen directorios habituales de credenciales y .env, pero un comando autorizado puede leer información sensible: revisa lo solicitado.

Las herramientas nativas de consola, imágenes, web y otros entornos están desactivadas; los hilos usan `environments: []`, sandbox de solo lectura y ninguna escalada de aprobación nativa. Codex usa herramientas controladas por XPEED. Es una restricción del protocolo fijado, no un sandbox para ejecutables de extensiones de terceros.

El motor interno `code_mode_host` permanece habilitado para despachar esas herramientas. Desactivarlo impide que las llamadas lleguen a XPEED y provoca «code-mode host is disabled». Al actualizar desde 0.2.0, reinicia XPEED y comienza una conversación nueva.

## Ejecución y límites

- Los comandos se ejecutan en la consola visible autorizada, conservando directorio, variables y entornos activados. Los destinos de pruebas sin consola pueden usar un shell separado.
- Ejecución visible en PowerShell y shells POSIX de SSH/WSL. Command Prompt aún no admite ejecución visible del agente.
- Los comandos remotos visibles requieren shell POSIX; los archivos usan SFTP de OpenSSH. Un fallo remoto nunca se sustituye por ejecución local.
- Comandos de 100 ms a 5 minutos, ocho operaciones simultáneas y 64 KiB de salida combinada de terminal. Resultados con sessionId, taskId, estado, exitCode y outputTruncated. Archivos con límite de cancelación de 25 segundos.
- Archivos UTF-8 existentes hasta 64 KiB. Se comparan antes de aplicar; temporal exclusivo junto al original y renombrado atómico. Remoto requiere posix-rename@openssh.com. Conserva permisos disponibles; propietario y ACL pueden cambiar. Conflictos detectados de forma optimista, sin garantizar exclusión de escritores concurrentes.
- **Detener**, desactivar o desconectar cancela trabajos y aprobaciones. Cerrar SSH no garantiza detener procesos independientes. Se informa remoteMayBeRunning ante cancelación, tiempo agotado o fallo de transporte; comandos normales tienen su propio timeout, pero procesos separados pueden sobrevivir.
- Las conversaciones y resultados se guardan en Xpeed; al volver a abrirlas se retoma el hilo del proveedor. Selecciona las sesiones actuales de terminal después de reiniciar.
- **Adjuntar selección de la terminal** envía solo el texto realmente seleccionado. También puedes pegar un fragmento de archivo/terminal. No se adjunta automáticamente todo el historial.

## Desarrollo y comprobación

Dentro del repositorio independiente exportado usa `cargo build --release --locked` y `cargo test --locked`. Los scripts y las pruebas de integración del host mencionados abajo pertenecen al workspace de Xpeed, no al repositorio independiente.

Compila con `cargo build --release --locked -p xpeed-codex-extension`. scripts/package-extension.ps1 prepara el paquete opcional; scripts/prepare-git-repositories.ps1 prepara su proyecto Git con código, manifiesto, documentación bilingüe, biblioteca de idiomas y ejecutable.

Pruebas de permisos, reconexión, aprobación, procesos locales reales, salida larga, tiempos agotados, cancelación y conflictos. `py -3.12 scripts/test-session-tools.py` comprueba dos transportes SSH/SFTP locales reales con archivos temporales y destinos separados; requiere dependencias en .tools/ssh-testdeps y MSYS2.

Con el adaptador release compilado y su cuenta de ChatGPT conectada, `cargo test --locked real_codex_executes_through_host_bridge -- --ignored --nocapture` verifica que el modelo real llame a XPEED y reciba salida de un comando local. Añade `--codex` al script SSH para comprobar también esa ruta contra un servidor SSH aislado.

scripts/test-codex-extension.py verifica el adaptador con fixture explícita y el runtime real con inicialización, cuenta, modelos e inicio/cancelación del acceso oficial. Una fixture no es una consulta real. También se verificó una respuesta real con una cuenta conectada, sin autorizar ejecución ni acceso a archivos en ese diagnóstico. Comandos y cambios se comprobaron por separado mediante pruebas del host, incluyendo dos transportes SSH/SFTP reales.

Referencias oficiales: [Codex App Server](https://learn.chatgpt.com/docs/app-server) y [configuración](https://learn.chatgpt.com/docs/config-file/config-reference). Esquema generado con `codex app-server generate-json-schema --experimental`.

## Codex 0.3.1: consola visible y panel propio

Requiere Xpeed 0.9.1+. Abre **Extensiones → Codex**: ahora tiene su propio panel, sin el contenedor de IA ni la pestaña de Ollama. Selecciona explícitamente las sesiones destino y autoriza los comandos antes de ejecutarlos. `xpeed_exec` escribe y ejecuta en la terminal real; `cd`, las variables y los entornos activados se conservan. `xpeed_read_terminal` lee la pantalla visible de una sesión autorizada. La salida de terminal es información no confiable y no concede permisos.

Deja la consola en un prompt vacío. Se admiten PowerShell y SSH/WSL con shell POSIX; Command Prompt no admite esta ejecución todavía. Mientras corre un comando del agente, la entrada normal queda reservada; Ctrl+C o Detener lo interrumpen. stdout y stderr comparten la salida de la consola. Si no se puede confirmar el final, se informa expresamente; no se inventa un código de salida ni se cambia de sesión. Las sesiones de pruebas sin consola conservan el modo de ejecución separado.

Clic derecho sobre una selección → **Insertar en chat** adjunta el texto sin enviarlo ni seleccionar destinos nuevos. El botón + permite revisar el adjunto; × lo quita.

## Versiones

Mantén VERSION, Cargo.toml y extension.json con la misma versión. Actualiza Cargo.lock mediante Cargo, registra cambios del adaptador en CHANGELOG.md y etiqueta las entregas como `vX.Y.Z`. Los cambios de interfaz del host usan la versión de Xpeed. Los proyectos exportados incluyen estos archivos; compila y copia el binario a la ruta del manifiesto antes de publicar.
