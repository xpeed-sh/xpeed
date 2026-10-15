# Extensiones de Xpeed

## Proyectos y versiones

| Extensión | Versión del adaptador | Documentación | Historial |
| --- | --- | --- | --- |
| Codex | 0.3.1 | [README](../extensions/codex/README.es.md) | [CHANGELOG](../extensions/codex/CHANGELOG.md) |
| Monitor del servidor | 0.1.2 | [README](../extensions/server-monitor/README.es.md) | [CHANGELOG](../extensions/server-monitor/CHANGELOG.md) |

Cada extensión mantiene VERSION, Cargo.toml y extension.json sincronizados. El script de exportación comprueba esos números e incluye su changelog. Los proyectos publicados viven en `xpeed-sh/extension-codex` y `xpeed-sh/extension-server-monitor`. La carpeta fuente `extensions/` permanece en el workspace de Xpeed para compilar y probar el contrato junto al host; no se incluye como paquete en el instalador. `extensions/packages/` contiene artefactos locales generados y excluidos de Git. Los cambios de interfaz implementados en el host se registran en el changelog de Xpeed.

Las secciones con números de versión describen la evolución del contrato. Actualmente cada extensión de chat tiene su propio panel; no existe un contenedor IA compartido. Las extensiones se abren desde el gestor o el menú Extensiones.

## Herramientas de sesiones (0.8.0)

Codex 0.3.1 usa ChatGPT y herramientas del host para sesiones seleccionadas. Consulta [instalación, permisos y limitaciones](../extensions/codex/README.es.md), incluidas las versiones compatibles del runtime oficial.

Una extensión solicita acceso con la capacidad `sessionTools` y el permiso `sessions.tools`. El host genera `scopeToken` cuando el usuario selecciona destinos para una conversación. Sus metadatos públicos contienen ID, nombre, tipo, estado, host, shell y directorio; no incluyen contraseñas, claves ni objetos de terminal.

El adaptador recibe ese ámbito en `chat/start.params.scope`. Envía solicitudes JSON-RPC: `{"id":"tool-1","method":"tools/call","params":{"scopeToken":"...","tool":"xpeed_exec","arguments":{"sessionId":1,"cwd":"/tmp","command":"pwd","timeoutMs":10000}}}`. El host valida token, destino y encarnación de la sesión, muestra aprobaciones y devuelve resultado o error con el mismo ID. El proveedor no autoriza sesiones por sí mismo.

Herramientas: `xpeed_list_sessions`, `xpeed_exec`, `xpeed_read_terminal`, `xpeed_task_status`, `xpeed_cancel_task`, `xpeed_read_file`, `xpeed_list_directory`, `xpeed_propose_change`, `xpeed_apply_change`. Las tareas incluyen estado, `sessionId`, `taskId`, stdout/stderr, `exitCode` y `outputTruncated`. Las propuestas pertenecen al host, se revisan como diferencias y se aplican por ID. Cada adaptador traduce su protocolo; el núcleo no implementa el de Codex.

Desde **0.7.1**, la interfaz inicia en inglés; cambia a español en **Settings → Language**. Codex 0.1.1 y Monitor 0.1.2 siguen ese ajuste, incluso conectados. Cada extensión puede declarar metadatos y documentación por idioma en `localizations`; consulta [LOCALIZATION.md](LOCALIZATION.md) para integrar más idiomas y el protocolo de actualización.

En **Xpeed 0.7.0**, el gestor divide la columna en **Instaladas** y **Catálogo**, con búsqueda, scroll independiente y un separador ajustable. Selecciona una extensión para abrir su ficha como pestaña central: allí puedes instalarla, activarla, abrir su panel, configurar el proveedor y desinstalarla. El menú **Git** conecta repositorios y catálogos; también conserva la instalación desde una carpeta local.

Desde **Xpeed 0.6.13**, el catálogo incluye **Monitor del servidor 0.1.0**, una vista derecha opcional con gráficos y procesos de la pestaña SSH activa. Consulta [requisitos, instalación e interpretación de métricas](../extensions/server-monitor/README.md).

Xpeed 0.6.0 incorpora paquetes locales independientes del ejecutable principal. La primera extensión es **Codex 0.1.0**. El icono Extensiones reemplaza el contenido de la segunda columna por un gestor para instalar, activar, desactivar y desinstalar. Instalar y activar no inicia procesos: el proveedor arranca al pulsar **Conectar** en IA.

La distribución Git instala el proyecto directamente, sin ZIP ni VSIX. Claude y una tienda web todavía no están implementados. El contrato permite crear sus adaptadores sin añadir lógica del proveedor al núcleo.

## Repositorios y catálogo Git

En Extensiones → **Git**, pega el enlace HTTPS de un repositorio público de GitHub y pulsa **Conectar repositorio**. La rama o etiqueta y la subcarpeta del paquete son opcionales; en blanco usa la rama predeterminada y la raíz. Xpeed lee `extension.json`, `README.md` y la imagen indicada por `icon`, abre la ficha y permite **Instalar**. Los metadatos se leen por HTTP; instalar requiere `git` disponible en PATH.

Cada proyecto debe incluir el ejecutable listo para su plataforma, además del código fuente si se desea. Xpeed no compila proyectos ni ejecuta scripts de instalación. Una estructura mínima:

```text
extension.json
README.md
LICENSE
assets/icon.png
bin/mi-extension.exe
src/                    # opcional
```

En el manifiesto, `"icon": "assets/icon.png"` es opcional. Por ahora admite PNG de hasta 2 MiB y 1024 × 1024 píxeles. Sin imagen utiliza el icono integrado. El título, editor, descripción y versión proceden del manifiesto, y el contenido de la ficha del README.

El índice `extensions/catalog.json` puede estar junto al proyecto Xpeed o dentro de su repositorio. Cada entrada solo necesita el ID y la URL; `reference`, `packagePath` y `requirements` son opcionales:

```json
[
  {
    "id": "xpeed.codex",
    "repository": "https://github.com/xpeed-sh/extension-codex",
    "reference": "main",
    "requirements": "Codex CLI instalado para conectar el proveedor"
  }
]
```

Para leer el índice remoto, usa **Conectar catálogo** con `https://github.com/xpeed-sh/xpeed`. Busca `extensions/catalog.json` en la rama predeterminada. **Actualizar catálogo Git** vuelve a consultar sus entradas y fichas. Las fuentes conectadas y los metadatos se conservan localmente, de modo que un fallo de red no vacía el catálogo. La caché está en `%APPDATA%\xpeed\extension-cache` en Windows. Para una extensión instalada desde Git, abre su ficha, desconéctala y usa Actualizar desde Git; revisa los permisos antes de aplicar el cambio. Puedes restaurar una versión conservada localmente. Refrescar el catálogo solo actualiza las fichas.

El archivo [catalog.json](../extensions/catalog.json) es el único índice de extensiones disponibles y apunta a los repositorios públicos de Codex y Monitor. La lista de instaladas no usa otro catálogo: Xpeed escanea `%APPDATA%\xpeed\extensions` en Windows y la carpeta de datos equivalente en Linux/macOS. Activación, rutas de proveedores y origen Git se guardan en las preferencias locales del usuario.

Ejecuta `powershell -ExecutionPolicy Bypass -File scripts/prepare-git-repositories.ps1` para regenerar proyectos independientes en `dist/git-repositories/xpeed.codex` y `dist/git-repositories/xpeed.server-monitor`. Incluyen código, Cargo.lock propio, imagen, manifiesto, binario Windows, licencias y CI. Conserva `bin/` versionado para que la instalación desde Git sea directa.

La instalación clona la rama o etiqueta, valida el ID contra el catálogo y comprueba que el ejecutable existe dentro del proyecto. Copia el paquete sin `.git`, registra el commit en `.xpeed-source.json` y limpia la descarga temporal. Instalar no inicia el proveedor. Esta primera versión admite enlaces públicos `https://github.com/autor/repo`, sin autenticación para repositorios privados.

## Paquete

Ejemplo: `extensions/codex/extension.json`. Usa un ID como `publisher.extension`, protocolo 1, versión propia, nombre, descripción, editor, licencia, capacidades y rutas de ejecutables por plataforma (`windows`, `linux`, `macos`). Incluye todos los archivos necesarios dentro del paquete; no se admiten rutas absolutas, enlaces simbólicos ni rutas que salgan de él. Límite de instalación: 64 MiB y 512 entradas. El adaptador Codex no incluye el CLI oficial.

Los paquetes se copian a la carpeta de configuración de Xpeed, en `extensions/<id>`. En Windows: `%APPDATA%\xpeed\extensions`. `preferences.json` guarda activación y rutas opcionales a proveedores; no guarda tokens ni contraseñas. La distribución de paneles se migra conservando el espacio de trabajo anterior.

Las extensiones son ejecutables locales con los permisos del usuario. Las descripciones del manifiesto son informativas; el host no ofrece un sandbox de plugins. El sandbox de trabajo de Codex lo gestiona Codex. Instala paquetes de una fuente que conozcas.

## Contrato JSON-RPC v1

Cada mensaje es un objeto JSON UTF-8 terminado en `\n`, máximo 2 MiB. El host inicia el ejecutable en el directorio del paquete y comunica por stdin/stdout. Reserva stdout para el protocolo. Las solicitudes incluyen `jsonrpc: "2.0"`, `id`, `method` y `params`. Las respuestas conservan el ID y devuelven `result` o `error: {code, message}`. Las notificaciones no incluyen ID. El proceso y sus hijos se terminan al desconectar.

| Método del host | Parámetros / resultado |
| --- | --- |
| `initialize` | `{protocolVersion:1, clientVersion, providerPath}` → `{protocolVersion:1}` |
| `account/read` | `{}` → `{account:null \| {type,email,planType}, requiresAuth}` |
| `account/login` | `{}` → `{loginId, authUrl}` o `{verificationUrl,userCode}` |
| `account/login/cancel` | `{loginId}` → `{}` |
| `account/logout` | `{}` → `{}`; reservado, el panel no lo invoca |
| `models/list` | `{}` → `{models:[{id,name}]}` |
| `conversation/reset` | `{}` → `{}` |
| `chat/start` | `{text,cwd,model,mode:"readOnly"\|"agent"}` → `{conversationId,turnId}` |
| `chat/interrupt` | `{}` → `{}` |
| `approval/respond` | `{requestId,decision:"accept"\|"decline"}` → `{}` |
| `input/respond` | `{requestId,answers:{questionId:{answers:[string]}}}` → `{}` |
| `shutdown` | `{}` → `{}` y salida del proceso |

| Notificación al host | Parámetros |
| --- | --- |
| `account/updated` | `{account,requiresAuth}` |
| `account/login/completed` | `{loginId,success,error}` |
| `chat/delta` | `{messageId,delta}` |
| `chat/message` | `{messageId,role:"assistant",text}`; reemplaza el texto parcial |
| `chat/completed` | `{status,error:null\|{message}}` |
| `approval/request` | `{requestId,kind,details}` |
| `approval/resolved` | `{requestId}` |
| `input/request` | `{requestId,questions:[{id,question,isSecret,options:[{label,description}]}]}` |
| `status` / `error` | `{text}` / `{message}` |

Responde a las solicitudes en menos de 60 segundos. Las respuestas de chat llegan por notificaciones después de aceptar `chat/start`. Los IDs de mensajes deben distinguir turnos. Autenticación, llamadas al proveedor y transporte LSP/MCP adicional pertenecen al adaptador. No confundir este contrato de chat con LSP, que está orientado a herramientas de edición; Codex utiliza su protocolo app-server.

## Desarrollo y comprobaciones

`cargo test --workspace --locked` y `cargo clippy --workspace --locked --all-targets -- -D warnings` comprueban núcleo y adaptador.

`cargo build --release -p xpeed-codex-extension --features test-provider --locked` genera también `mock-codex`, un proveedor de prueba que no se empaqueta.

`py scripts/test-codex-extension.py target/release/xpeed-codex-extension.exe --provider target/release/mock-codex.exe --mock` comprueba login simulado, streaming, aprobaciones, errores, interrupción y apagado.

Sin `--mock`, el script solo comprueba initialize, estado de cuenta y modelos contra Codex real. No inicia login ni envía consultas. No imprime datos de cuenta.

`target/release/xpeed.exe --extension-smoke-test extensions/packages/xpeed.codex` verifica el mismo inicio a través del host real de Xpeed, sin cargar perfiles ni abrir una ventana. Comprueba el código de salida: 0 indica éxito.

## Vistas de extensiones (Xpeed 0.6.12)

La columna derecha contiene vistas independientes **IA**, **Xpeed** y **Servidor**. IA agrupa los proveedores de chat instalados en pestañas. Claude aparece cuando se instala un adaptador compatible; no viene integrado. Servidor muestra metadatos de la conexión seleccionada, no métricas remotas de CPU o memoria.

Una extensión puede añadir vistas sin declarar la capacidad `chat`. El campo opcional `views` del manifiesto contiene hasta 16 vistas con ID único por paquete, título y ubicación inicial:

```json
"views": [
  {"id":"status", "title":"Estado", "location":"right"},
  {"id":"navigation", "title":"Navegación", "location":"left"},
  {"id":"report", "title":"Informe", "location":"center"}
]
```

Desde Xpeed 0.6.15, Monitor comparte panel con Servidor, mediante dos pestañas internas. Los demás paneles mantienen su independencia. La cabecera tiene acciones de expandir/restaurar y cerrar al extremo derecho; cerrar oculta el panel completo, sin eliminar sus pestañas ni detener las terminales. Expandir ocupa el espacio de su anfitrión y restaurar recupera la distribución anterior. La expansión es temporal y no se guarda como distribución permanente.

El footer ofrece Información. Las extensiones se abren desde su gestor mediante **Abrir <título>** o **Abrir chat**; no reciben botones en el footer por defecto. El icono superior derecho muestra u oculta la columna de herramientas; sin una extensión abierta muestra Información.

| `location` | Ubicación |
| --- | --- |
| `right` | Panel independiente en la columna derecha |
| `left`, `leftbelow` | Panel independiente debajo de la navegación de sesiones |
| `files` | Pestaña junto a Archivos y Conexiones |
| `commands`, `activity` | Pestaña junto a Comandos y Actividad |
| `filesbelow` | Panel debajo del grupo de Archivos y Conexiones |
| `commandsbelow` | Panel debajo del grupo de Comandos y Actividad |
| `settings` | Sección desplegable dentro de Ajustes |
| `extensions` | Sección desplegable dentro de Extensiones |
| `center` | Pestaña junto a la terminal seleccionada o Inicio |
| `righttab` | Pestaña dentro del panel IA, únicamente cuando se solicita explícitamente |
| `server` | Pestaña dentro del panel Servidor; usada por Monitor del servidor |

Una vista puede solicitar explícitamente `"icon":"server"` para obtener un botón en el footer que abre u oculta su panel completo. También se admiten `info`, `extensions` y `workspace`, usando los SVG del host y los colores del tema. Omitir `icon` mantiene la vista accesible desde el gestor. Monitor no declara un icono. Los SVG personalizados suministrados por paquetes todavía no se admiten.

Para agrupar varias vistas de una misma extensión como pestañas internas de un panel propio, declara el mismo `panel` y `location` en ellas:

```json
"views": [
  {"id":"summary", "title":"Resumen", "location":"right", "panel":"tools"},
  {"id":"details", "title":"Detalles", "location":"right", "panel":"tools"}
]
```

`panel` admite hasta 80 caracteres alfanuméricos, guiones y guiones bajos. Su identidad pertenece al paquete: dos extensiones que usan `tools` no comparten panel accidentalmente.

Los paneles de los árboles de sesiones y terminales pueden reorganizarse con el sistema de docking; las secciones de Ajustes y Extensiones permanecen en su anfitrión. La distribución se conserva entre reinicios y las distribuciones antiguas se migran sin cambiar las divisiones de terminal. Desactivar o desinstalar un paquete retira sus vistas. En Extensiones, **Abrir <título>** abre la vista. Publicar una vista no ejecuta el paquete: se inicia desde **Conectar extensión**. Las ubicaciones disponibles son las de esta tabla; no se permite inyectar interfaz arbitraria en cualquier control de Xpeed.

Tras `initialize`, el host envía `views/read` con `{viewId:"status"}`. La respuesta debe incluir `result:{viewId:"status",text:"Contenido de la vista"}`. El botón **Actualizar** repite la solicitud. Para publicar cambios, la extensión envía la notificación `views/updated` con los mismos campos `viewId` y `text`. Se aceptan únicamente IDs declarados en el manifiesto y hasta 64 KiB de texto por vista. El contenido de texto se muestra sin HTML ni scripts; el transporte no envía contraseñas ni contexto de terminal a `views/read`.

## Métricas y dashboards (0.6.13)

Una extensión puede publicar `{viewId,dashboard}` desde `views/read` o `views/updated`. El dashboard incluye `contextId`, `caption`, `status`, `metrics:[{label,display,max,history}]` y `processes:[{pid,start,name,cpu,memory,memoryBytes}]`. Se admiten hasta 12 métricas, 120 puntos por serie y 100 procesos; todos los valores numéricos deben ser finitos. El host dibuja gráficos y controles de consulta; no ejecuta código de interfaz suministrado por el paquete.

Solo los paquetes con capacidad `serverMetrics` y permiso `server.metrics.read` reciben las notificaciones `server/context` y `server/sample`. `server/context` incluye `{id,sessionId,name,host,port,user,observing}` para una sesión SSH autenticada, o `{id:null,observing:false,name}` cuando no existe tal sesión. `id` identifica una generación, y cambia al seleccionar otra sesión o pausar/reanudar. `server/sample` contiene `{contextId,sample}` o `{contextId,error}`. `sample` es el texto de las secciones CPU, MEMORY, UPTIME, NETWORK, DISK, PAGESIZE y PROCESSES, delimitadas por sus encabezados. CPU/procesos/memoria/red proceden de procfs; DISK de `df -Pk /`, y PAGESIZE de `getconf PAGESIZE`.

El host ejecuta un recolector fijo de lectura sobre un canal SSH separado, sin admitir comandos arbitrarios de la extensión ni transferirle contraseñas, claves o tokens. Muestrea únicamente cuando una vista de un paquete autorizado está abierta y el adaptador está conectado. El paquete calcula las tasas, genera el dashboard y debe conservar el `contextId`; el host rechaza dashboards de una generación anterior. En las vistas ocultas se detiene el recolector. La extensión no necesita la capacidad chat.

## Paneles independientes y herramientas de consola (0.9.0)

La capacidad `chat` aporta automáticamente una vista derecha `chat`, con clave estable por ID de extensión. Cada proveedor posee su panel y su estado; no hay panel compartido de IA. Las demás contribuciones siguen usando `views` y `panel` para agrupar únicamente las vistas de la misma extensión.

`xpeed_read_terminal` recibe `sessionId` y devuelve `text` de la pantalla visible. `xpeed_exec` usa `executionMode: visibleTerminal` si existe consola real; exige prompt vacío y conserva el shell. Sus resultados indican `streamsCombined: true` y no separan stderr. El host conserva los handles de PTY; nunca los transmite por JSON-RPC. Los targets de pruebas sin consola informan `separateShell`. Se mantienen scopeToken, aprobación y revocación por encarnación.
