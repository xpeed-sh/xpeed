# Changelog

## Sin publicar

- README principal reducido a presentación, funciones y enlaces. Referencia técnica en DEVELOPMENT.md y preparación de repositorios en GIT-PUBLISHING.md.
- Codex y Monitor tienen VERSION y CHANGELOG independientes, incluidos en sus proyectos exportados y paquetes.

## 0.10.6 - 2026-10-10

- Terminal sizing ignores transient tiny dock layouts and sizing passes, preventing the shell from printing early output in two-column lines. Disconnected terminals can reflow preserved output without resizing a closed PTY.
- Session status moves below the console with 12 px lateral and 8 px vertical margins. A separate layout region prevents the status card from altering console width; the reserved footer follows expanded technical details.

## 0.10.5 - 2026-10-10

- Task-access menu labels no longer select text; the entire choice shows a pointing cursor and selects its mode.
- Disconnected terminal sessions show a compact status card with Reconnect on the same row, preserved terminal output and collapsible technical details with a copy action. Reconnection progress uses the same card.

## 0.10.4 - 2026-10-10

- Command history always includes earlier runs of the same connection. Click a command to insert it into the visible terminal without executing; pending records flush before tabs close and on app exit. The redundant connection heading is removed and clear history floats inside the panel at bottom right with its SVG icon.
- Terminal focus reserves arrow, Tab and Escape keys for the shell, fixing repeated history navigation losing focus to neighboring controls.
- Codex uses the supplied SVG marker for the selected task access mode. Add and send controls are circular; model selection uses rounded corners.
- The Add menu attaches local UTF-8 text files and source folders through an asynchronous file browser. Attachments show names and sizes and can be removed before sending. Limits: 8 attachments, 256 KB per file, 1 MB total per message including JSON escaping, folder previews up to 32 text files / 1 MB, excluding build directories and symlinks. Binary files are explicitly rejected or marked as omitted. Terminal selection remains available from its context menu.

## 0.10.3 - 2026-10-10

- Codex task access offers Request approval and Full access with duotone SVG shields, a selected marker and theme-aware yellow for full access. The choice is saved immediately and applies to commands and edits on selected sessions for the task.

## 0.10.2 - 2026-10-10

- Codex uses one working indicator and one response footer per turn, including all intermediate assistant messages in its general copy action.
- Theme-aware duotone SVG copy icons switch to a checked state for two seconds after copying; code snippets and tool results use the same feedback.
- Commands submitted through the visible agent console now enter the session command history once, without internal transport wrappers, respecting the Record commands setting.

## 0.10.1 - 2026-10-10

- Codex responses group session actions with their own turn instead of a separate conversation-wide list. Command output, exit codes and file differences remain expandable.
- Response headers show elapsed work time; footers show copy and local date/time. Turn identity and metadata survive reopening saved conversations. Legacy chats remain readable without invented timestamps or action attribution.
- CommonMark formatting for headings, emphasis, links, quotes, lists and tables. Fenced code blocks include language, syntax colors and exact code copying, including incomplete streamed blocks.
- Local file links copy their path; response rendering does not fetch images. English and Spanish labels follow the application language.

## 0.10.0 - 2026-10-10

- Bounded SSH reconnection after authenticated transport failures. Tabs remain open, retries back off and commands are never replayed. Authentication failures and normal exits require manual reconnect.
- Remote directory cache, one read-only SFTP retry and virtualized tree/grid rendering. Refresh invalidates the cache. Monitor shutdown no longer waits on the interface thread.
- Streaming SFTP upload by dropping files into the explorer, contextual download, byte progress and cancellation. Existing destination files are protected.
- Remote UTF-8 editor (64 KiB) with unsaved-change protection and optimistic conflict checks before atomic saving. Double-click a file or choose Edit remote file.
- Project groups for connection profiles. Configuration export/import merges profiles, previews imports and excludes credentials and Codex authentication.
- Search across terminal scrollback with previous/next navigation, configurable scrollback limit, and connection/extension entries in the action palette.
- Clear Codex working/executing/approval status beside its existing stop control.
- Git extension updates with a permissions review and version history for rollback, preserving local chat data and preferences. Disconnect an extension before switching versions.
- Packaging reads VERSION and includes a new Windows installer. Xpeed automatic application updates remain outside this release.

## 0.9.3 — 2026-10-09

- El listado de chats incluye el compositor inferior: escribir y enviar inicia una nueva conversación automáticamente, sin pulsar Nuevo chat.
- Los límites de uso aparecen inmediatamente encima del compositor. El borrador de una nueva tarea se guarda por separado del borrador de la conversación anterior.
- El título editable queda centrado en la misma fila que volver, sesiones y opciones.

## 0.9.2 — 2026-10-09

- Listado de chats con filas compactas, títulos alineados a la izquierda y acciones discretas en la cabecera.
- Papelera al pasar el cursor por una conversación. El borrado también elimina sus datos guardados y se conserva al reiniciar.
- Título editable centrado dentro del chat; se retira el botón de nueva conversación de esa cabecera.

## 0.9.1 — 2026-10-09

- Codex 0.3.1 guarda conversaciones, títulos editables, borradores, modelo y preferencia de acceso. Las conversaciones guardadas pueden retomarse con su hilo original mediante `thread/resume`.
- Restaura la cuenta de ChatGPT al abrir Codex y pasa directamente al listado cuando ya existe una sesión válida.
- Selector de sesiones en la cabecera y controles del compositor en una sola fila: adjuntos y acceso a la izquierda, modelo y envío a la derecha.
- Uso de la cuenta debajo del listado, con porcentajes disponibles, barras y tiempo de restablecimiento en lugar de JSON.
- Las aprobaciones pendientes se cancelan al restaurar; las sesiones de terminal se seleccionan nuevamente después de reiniciar.

## 0.9.0 — 2026-10-09

- Codex 0.3.0 tiene un panel independiente; se retira el panel de IA compartido. Cada extensión de chat recibe su propio panel.
- Los comandos aprobados se ejecutan en la consola visible de PowerShell o SSH/POSIX, conservando directorio, variables y entornos activados. Se puede leer la pantalla con `xpeed_read_terminal`.
- La selección de terminal permite Copiar e Insertar en chat, sin enviar automáticamente ni ampliar permisos.
- Respuestas con encabezados, listas, código en línea y bloques de código copiables.
- Ejecución con límites de salida, timeout, interrupción y comprobación de prompt vacío. Command Prompt aún no admite ejecución visible.

## 0.8.3 — 2026-10-09

- Aprobaciones de comandos y cambios visibles junto al compositor, fuera del historial desplazable.
- Terminar la respuesta del proveedor ya no cancela una acción que está esperando la aprobación del usuario. Tras aprobarla, XPEED entrega el resultado real a Codex para continuar, sin volver a ejecutar la operación.
- Detener, desconectar o comenzar otra conversación sigue cancelando solicitudes pendientes y revocando su continuidad.

## XPEED Codex 0.2.2 — 2026-10-09

- Compatibilidad verificada con Codex 0.162.0-alpha.17.2, además de 0.162.0-alpha.2, para que la actualización del runtime de VS Code no bloquee la conexión.
- Validación explícita de las versiones compatibles y prueba de comandos con Codex real a través del puente de sesiones de XPEED.

## 0.8.2 — 2026-10-09

- Nuevo flujo de Codex: pantalla de conexión, bienvenida con Continuar, lista de conversaciones y vista de chat.
- Mensajes del usuario en burbujas, respuestas en el área central y compositor inferior con modelo, adjuntos, permisos y selección de sesiones.
- Navegación para volver a las conversaciones y consultar las conversaciones archivadas en modo de lectura durante la sesión de XPEED.
- Interfaz en español e inglés, ajustada al ancho del panel y sin mostrar el chat antes de conectar la cuenta.

## XPEED Codex 0.2.1 — 2026-10-09

- Corregido «code-mode host is disabled»: el adaptador habilita el motor interno que despacha herramientas de XPEED, manteniendo los entornos nativos restringidos.
- Prueba de integración con Codex real, aprobación del host y salida de comandos locales y SSH sobre servidores de prueba aislados.
- Tras actualizar la extensión, reiniciar XPEED y comenzar una conversación nueva para aplicar la configuración del hilo.

## 0.8.1 — 2026-10-09

- Selector de sesiones de Codex abierto por defecto, con cantidad, estado y lista desplazable. Una terminal abierta debe seleccionarse explícitamente como destino.
- Aviso junto al chat cuando falta elegir una sesión, para explicar por qué Enviar está deshabilitado.
- Campo de mensaje y botones fuera de la zona desplazable; opciones de cuenta plegadas al estar conectado para evitar que Enviar quede fuera del panel.
- Comprobada una respuesta real de Codex con la cuenta conectada, sin herramientas de ejecución ni acceso a archivos durante el diagnóstico.

## 0.8.0 — 2026-10-09

- XPEED Codex 0.2.0: extensión independiente con Codex App Server local, acceso oficial con ChatGPT y perfil separado del proveedor.
- Selección explícita de una o varias sesiones; cada conversación conserva sus destinos aunque cambie la pestaña activa. Una desconexión revoca el acceso y requiere una conversación nueva.
- Herramientas del host para comandos, consultas y cancelación de tareas, lectura/listado de archivos y cambios con diferencias y aprobación. Canales independientes de la consola interactiva, límites de salida y tiempos de ejecución.
- Acciones visibles por sesión, aprobación individual o por tarea, adjuntos de texto seleccionados, modelos del proveedor, límites de uso e historial de conversaciones durante la ejecución de XPEED.
- Herramientas locales nativas del proveedor deshabilitadas mediante entornos vacíos y configuración restringida; compatibilidad fijada a Codex 0.162.0-alpha.2 por su protocolo experimental.
- Pruebas con procesos locales reales y dos servidores SSH/SFTP de prueba: separación de destinos, edición, conflictos, salida larga, cancelación y tiempos agotados. El inicio de sesión humano y una consulta real a ChatGPT quedan por comprobar con la cuenta del usuario.

## 0.7.1 — 2026-10-09

- Inglés como idioma inicial, con selector English/Español en Ajustes → Idioma y preferencia persistente.
- Catálogos compartidos de traducción para Xpeed, Codex y Monitor; menús, paneles, conexiones, archivos, acciones y mensajes traducidos.
- Cambio de idioma de extensiones en ejecución mediante initialize.language y la notificación locale/change, sin desconectarlas.
- Manifiestos de extensiones con nombres, descripciones, permisos, títulos de vistas y README por idioma; lectura de sus documentos desde Git.
- Codex 0.1.1 y Monitor 0.1.2 con interfaz y documentación en inglés/español. Los proyectos Git independientes incluyen la biblioteca de idiomas.
- Migración de preferencias anteriores al inglés, conservación de otros ajustes y pruebas de catálogos, parámetros de mensajes y rutas de documentación.

## 0.7.0 — 2026-10-09

- Catálogo de extensiones conectado a repositorios públicos de GitHub mediante JSON. Nombre, descripción, README e imagen se leen del proyecto.
- Menú Git para conectar una extensión por URL, conectar el catálogo de Xpeed o refrescar sus fichas; consultas y clonación en segundo plano.
- Instalación directa con Git, sin ZIP: valida el manifiesto y el ejecutable incluido, instala el proyecto sin su historial y no inicia el proveedor automáticamente.
- Caché de fichas para conservar el catálogo sin conexión y registro del commit instalado.
- Script para preparar Codex y Monitor como proyectos Git independientes con código, manifiesto, imagen, licencias y binario Windows.
- Pruebas de lectura de metadatos, imágenes, límites, identidad del paquete e instalación desde un repositorio Git real de prueba.

## 0.6.19 — 2026-10-08

- Controles cuadrados de panel reducidos a 23 px; mantienen separación de 5 px y margen derecho de 8 px.
- Opciones y división de la terminal también tienen margen derecho y tamaños más compactos.
- Menú superior Extensiones con accesos a los paquetes instalados. La marca Activa refleja la pestaña visible de cada extensión; seleccionar una abre su panel, o su ficha si está desactivada.
- Pruebas de activación de pestañas, cierre del panel y apertura de extensiones sin iniciar procesos automáticamente.

## 0.6.18 — 2026-10-08

- La columna derecha inicia oculta, incluso con una distribución guardada. Su acceso superior abre Información inicialmente y después recuerda el último panel abierto durante la sesión.
- Mostrar/ocultar la columna afecta a Información, IA, Servidor y las contribuciones de extensiones ubicadas a la derecha.
- Botones de cerrar y expandir panel cuadrados de 26 px, SVG de 24 px sin deformación, separación de 5 px y margen derecho de 8 px.
- Pruebas del inicio oculto, reapertura del último panel y proporciones del SVG con diferentes escalas de pantalla.

## 0.6.17 — 2026-10-08

- Borde exterior continuo sobre el contenido, con contraste adaptado al tema.
- Los paneles reutilizan los SVG de cerrar y maximizar de los controles de ventana.
- Extensiones muestra Instaladas y Catálogo en dos regiones con scroll independiente y un divisor ajustable, además de búsqueda e instalación desde carpeta.
- Al seleccionar una extensión, su ficha se abre como pestaña central junto a las sesiones: descripción, documentación, requisitos, versión, permisos y controles de instalar, activar, abrir y desinstalar.
- Validación: 68 pruebas del workspace y Clippy sin advertencias; abrir la misma ficha no duplica pestañas ni reinicia las terminales.

Cambios de Xpeed por versión. El proyecto sigue en desarrollo; las versiones 0.x pueden cambiar funcionalidades y formatos de configuración.

## Sin publicar

## 0.6.16 — 2026-10-08

- Nuevo panel Acerca de con logo, versión, tarjetas de conexiones activas y sistema, autor y licencia MIT.
- Accesos a `https://xpeed.ronaldbit.com`, al repositorio previsto `https://github.com/xpeed-sh/xpeed`, su documentación y changelog, y a la organización de extensiones.
- Información técnica copiable y diseño adaptable al ancho del panel y a los temas claros/oscuros. Las conexiones finalizadas no se cuentan como activas.
- Validación: 67 pruebas del workspace, prueba de los botones web/repositorio, Clippy sin advertencias y capturas nativas en oscuro y claro.

## 0.6.15 — 2026-10-08

- Servidor y Monitor comparten panel con dos pestañas internas. Monitor del servidor 0.1.1 deja de añadir un icono al footer.
- El footer conserva Información; las extensiones se abren desde su gestor, salvo que soliciten expresamente un acceso adicional.
- Cerrar y expandir/restaurar se encuentran al extremo derecho de la cabecera de cada panel. Cerrar oculta el grupo completo y restaurar recupera la distribución previa sin cerrar terminales.
- Las extensiones pueden agrupar varias vistas en un panel mediante `panel`; la nueva ubicación `server` permite añadir contenido al panel de Servidor.
- Validación: 66 pruebas del workspace y Clippy sin advertencias; expansión/restauración, cierre del grupo completo y migración del Monitor separado.

## 0.6.14 — 2026-10-08

- La columna derecha aloja paneles independientes de IA, Información, Servidor y extensiones, con cierres individuales y separadores ajustables. Las pestañas de proveedores siguen dentro de IA.
- Icono SVG de Información y accesos a paneles en el footer. Las extensiones pueden declarar un icono del host para abrir su panel.
- Nuevas ubicaciones para pestañas de Archivos/Comandos/Actividad, paneles debajo de esos grupos y secciones de Ajustes/Extensiones; las pestañas centrales siguen disponibles.
- Migración del antiguo grupo de pestañas derecho sin modificar las divisiones de terminal ni las agrupaciones explícitas de la nueva versión.
- Validación: 64 pruebas del workspace y Clippy sin advertencias; prueba de interfaz con paneles simultáneos y migración de pestañas antiguas.

Sin cambios pendientes.

## 0.6.13 — 2026-10-08

- Paquete opcional Monitor del servidor 0.1.0: CPU, RAM, disco raíz, red por interfaz, gráficos y procesos ordenables por CPU/RAM.
- Seguimiento de la pestaña SSH autenticada, limpieza de datos al cambiar de servidor y rechazo de respuestas de contextos anteriores.
- Canal SSH persistente de lectura con el acceso de la sesión; contraseñas dentro del host, muestreo cada tres segundos, pausa al ocultar la vista y cierre del canal al cambiar de conexión.
- Vistas de extensión con dashboards estructurados, datos acotados y colores del tema; detalles de proceso y copia de PID sin acciones destructivas.
- Instalación de paquetes locales mediante --install-extension; catálogo y script de empaquetado del monitor.
- 62 pruebas, Clippy, muestras reales de Linux local, intercambio JSON-RPC y OpenSSH con una sola autenticación y cierre comprobado del canal; captura nativa de los gráficos.

## 0.6.12 — 2026-10-08

- El botón árbol/cuadrícula representa la vista activa, con identidad estable durante la navegación y seguimiento del directorio de la terminal.
- Panel derecho con vistas IA, Xpeed y Servidor; el botón superior muestra u oculta todas sus vistas integradas.
- Proveedores instalados como pestañas dentro de IA, conservando conversaciones independientes; Claude requiere su propio adaptador instalado.
- Vistas de extensiones declaradas en el manifiesto para la izquierda, derecha o centro, con contenido de texto por JSON-RPC, actualización y apertura desde el gestor. Compatible con paquetes de chat existentes.
- Migración del espacio de trabajo sin perder las divisiones de terminal y eliminación de vistas de extensiones desactivadas o desinstaladas.
- 55 pruebas, Clippy y captura nativa de la vista de cuadrícula y del grupo de herramientas derecho.

## 0.6.11 — 2026-10-08

- Ruta y nombre/IP de la conexión alineados junto a sus iconos; casita SVG en la carpeta inicial y flecha de regreso en las demás.
- Navegación SFTP sobre un canal autenticado persistente por conexión, evitando iniciar OpenSSH para cada carpeta. Los cambios rápidos de directorio priorizan el último solicitado.
- El límite de espera SFTP se aplica a cada operación activa, sin cerrar el canal por permanecer inactivo; cierre al terminar la conexión y reinicio al cambiar credenciales o tras errores.
- Mensajes de mantenimiento SSH cada 15 segundos y diagnóstico del código de salida si el proceso termina con error. La causa del corte observado en un servidor externo requiere su mensaje final.
- 51 pruebas, Clippy, capturas nativas y comprobación SSH/SFTP local con canal reutilizado después de 22 segundos de inactividad.

## 0.6.10 — 2026-10-08

- Cabecera del explorador con ruta actual y botón de volver únicamente al disponer de historial; eliminada la fila adicional de navegación y su botón de refrescar.
- Raíz con nombre de conexión y su IP/host configurado, tanto en árbol como en cuadrícula; estado «- cargando…» en la misma línea durante listados y operaciones.
- Contador de selección a la izquierda y acciones al extremo derecho del pie.
- Refrescar permanece disponible en el menú de opciones y el menú contextual.
- Pruebas de historial, presentación de carga e interacción del explorador; Clippy y capturas nativas comprobados.

## 0.6.9 — 2026-10-08

- Controles de minimizar, maximizar/restaurar y cerrar con los SVG suministrados, conservando paths y opacidades.
- Iconos centrados a 22 px, con colores del tema y acento al pasar el cursor.
- Compilación, Clippy y capturas nativas comprobados en modo claro y oscuro.

## 0.6.8 — 2026-10-08

- Explorador remoto con vistas de árbol y cuadrícula, selección múltiple, tamaños alineados y barra de acciones.
- SVG suministrados para carpetas, archivos genéricos, imágenes, audio y vídeo; colores del tema y huecos de las notas conservados.
- Menú de opciones y clic derecho: selección, creación de archivos/carpetas, copiar rutas, cortar/pegar para mover, renombrar, refrescar y eliminar con confirmación.
- Operaciones mediante SFTP, creación exclusiva sin sobrescritura y eliminación de carpetas vacías. La selección y el portapapeles para mover pertenecen a cada conexión.
- Registro de operaciones en Actividad y aislamiento de la entrada de los diálogos respecto a la terminal.
- Pruebas de protocolo y de interacción, Clippy y capturas nativas de ambas vistas.

## 0.6.7 — 2026-10-08

- Selector de modo en Ajustes con los SVG de luna y sol suministrados, reemplazando los textos Oscuro/Claro.
- Opacidades conservadas, colores del tema, indicador de selección, tooltips y etiquetas accesibles.
- Importador de iconos ampliado para múltiples contornos, conservando los ocho rayos del sol.
- Compilación y Clippy comprobados; capturas nativas revisadas en ambos modos.

## 0.6.6 — 2026-10-08

- Ajustes organizado en secciones desplegables: temas, editor/terminal, archivos, comandos/historial, IA/herramientas y distribución.
- Seguimiento de carpeta, registro de comandos e inclusión del historial trasladados desde los paneles a Ajustes.
- Preferencias guardadas y aplicadas a conexiones abiertas y nuevas, con compatibilidad para configuraciones anteriores.
- Opciones de mostrar el panel IA y adjuntar texto visible de la terminal centralizadas en Ajustes.
- Validación: Clippy, 41 pruebas automáticas y captura nativa; comprobado el acordeón de comandos y su efecto en sesiones abiertas y nuevas.

## 0.6.5 — 2026-10-08

- Logo superior centrado verticalmente dentro de la barra.
- Icono de aplicación basado en assets/img/logo.png, con transparencia y tamaños de 16 a 256 px.
- Icono incrustado en el ejecutable de Windows y aplicado a la ventana para Explorador, barra de tareas y Alt+Tab.
- Compilación, Clippy y pruebas comprobados; captura nativa e icono del ejecutable revisados.

## 0.6.4 — 2026-10-08

- Logo de la barra superior más pequeño (14 px de alto).
- Iconos superiores de los paneles más grandes (22 px), conservando el tamaño de los controles de ventana.
- Compilación y Clippy comprobados; revisión de captura nativa.

## 0.6.3 — 2026-10-08

### Cambiado

- Botones superiores para mostrar/ocultar los paneles izquierdo y derecho con los SVG suministrados, en ese orden de izquierda a derecha.
- Icono de servidor suministrado para las conexiones guardadas.
- Logo SVG en la barra superior y en Inicio; margen vacío recortado, proporción conservada y color adaptado al tema.
- Importador del logo con trazos relativos y huecos en las letras; mallas generadas incluidas, sin dependencias SVG nuevas en el ejecutable.

### Validación

- Clippy y 40 pruebas automáticas; revisión de capturas nativas en modo oscuro y claro.

## 0.6.2 — 2026-10-08

### Corregido

- Cuatro columnas: barra de opciones, un único panel de contenido, terminales y herramientas.
- Conexiones/Archivos y Comandos/Actividad comparten la vista de sesiones; Ajustes y Extensiones reemplazan esa vista completa, conservando el ancho y las terminales abiertas.
- Navegación lateral separada de los splits de terminal; migración de distribuciones anteriores y conservación de los separadores del panel de sesiones.
- Icono de Extensiones sustituido por el SVG suministrado, con opacidad original y colores del tema.

### Validación

- 40 pruebas automáticas y Clippy sin avisos. Comprobadas las tres vistas sin mover ni reiniciar las sesiones y revisadas capturas nativas.

## 0.6.1 — 2026-10-08

### Cambiado

- SVG suministrados para el engranaje de Ajustes y el primer botón lateral; rellenos adaptados al tema, estados hover/activo y opacidades conservadas.
- Ajustes pasa de ventana flotante a panel izquierdo redimensionable, con scroll, selección de temas, tamaño de texto y distribución.
- Al cerrar Ajustes se conserva la distribución de conexiones y registros. La consola permanece utilizable mientras el panel está abierto.
- Iconos convertidos a mallas vectoriales antes de compilar, sin añadir un motor SVG al ejecutable. SVG originales y herramienta de importación incluidos.

### Validación

- 40 pruebas automáticas y Clippy sin avisos; captura del panel en modo oscuro y claro.
- Comprobada la entrada a una terminal real con Ajustes abierto y la conservación de paneles y sesión al cerrarlo.

## 0.6.0 — 2026-10-08

### Añadido

- Gestor de extensiones locales: instalación por carpeta, activación, desactivación y desinstalación. Catálogo local y protocolo JSON-RPC v1 documentado.
- Procesos de extensiones separados del ejecutable principal, inicio explícito y limpieza de sus procesos hijos al desconectar.
- Codex 0.1.0 como paquete opcional: detección del CLI o de la extensión oficial de VS Code, login ChatGPT desde navegador y reutilización de la sesión del proveedor.
- Selector de modelos, conversación con streaming, interrupción, modo de lectura/agente y solicitudes de aprobación o preguntas del proveedor en el panel IA.
- Opción para adjuntar explícitamente el texto visible de la terminal. Se conserva API manual/Ollama.
- Empaquetador independiente de Codex, guía de extensiones y comprobaciones del adaptador con proveedor simulado y handshake real sin inferencia.

### Validación

- 39 pruebas del núcleo y del adaptador, Clippy sin avisos y comprobación visual del gestor.
- Host real de Xpeed comprobado con el paquete Codex por stdio, incluyendo su grupo de procesos en Windows.
- Login, streaming, aprobación, errores e interrupción con proveedor de prueba. Inicio, estado de cuenta y modelos con Codex local real; no se inició un login real ni se envió una consulta a un modelo real.

## 0.5.1 — 2026-10-08

### Corregido

- Eliminados los ajustes automáticos de tamaño y posición durante el arrastre normal y el cambio de monitor; Windows gestiona la transición DPI.
- Restaurar mediante arrastre desde una ventana maximizada no la devuelve al monitor anterior.
- El ajuste al área útil ocurre al iniciar o restaurar, y ya no reemplaza el tamaño preferido por el tamaño limitado de una pantalla pequeña.
- Ampliada la prueba nativa para recorrer continuamente ambos monitores y comprobar que no hay saltos de posición ni pérdida progresiva de tamaño.

### Validación

- 32 pruebas automáticas y Clippy sin avisos.
- Recorrido continuo entre monitores Windows al 200 % y 100 %, sin saltos de posición ni pérdida acumulada de tamaño.
- Maximizar/restaurar y cambiar de monitor con la ventana maximizada.
- Los recorridos nativos usan movimientos programáticos de la ventana de prueba; los estados de arrastre desde maximizado tienen cobertura unitaria.

## 0.5.0 — 2026-10-08

### Añadido

- Archivos sigue la carpeta actual de cada terminal SSH mediante OSC 7 o rutas de prompts Unix estándar, con opción para desactivar el seguimiento.
- Iconos vectoriales para los dos paneles laterales, controles de ventana y archivos; se adaptan a los temas y a la escala DPI.

### Corregido

- Retirados los botones duplicados de conexión, color y ajustes de la barra superior y los botones generales + y × de las pestañas.
- Cursor normal al pasar sobre una pestaña; el espacio libre de su barra conserva el cursor de arrastre.
- Controles de ventana alineados al borde derecho del espacio de trabajo.

### Validación

- 30 pruebas automáticas y Clippy sin avisos.
- SSH/SFTP contra servidor local de prueba: contraseña, reutilización del acceso, `cd`, `cd ..`, carpeta inicial y árbol con nombres Unicode.
- Cursor normal en pestañas y cursor de arrastre en el espacio libre comprobados mediante eventos de interfaz.
- Maximizar, restaurar y mover entre dos monitores Windows con escalas de 200 % y 100 %.

## 0.4.1 — 2026-10-08

### Añadido

- Documentación para Git: changelog, VERSION, guía de contribución y convenciones de archivos.
- Instrucciones para abrir Xpeed y prueba nativa reproducible de tamaño de ventana en Windows.

### Corregido

- Tamaño y posición al restaurar la ventana después de moverla entre monitores con distinta escala DPI.
- El tamaño restaurado se conserva en unidades lógicas y se ajusta al área útil del monitor de destino.
- Los límites mínimos se adaptan a pantallas pequeñas; el inicio y los tamaños excesivos quedan dentro de un monitor.
- Minimizar no reemplaza el tamaño restaurado y el arrastre permite cruzar entre pantallas.

### Validación

- 25 pruebas automatizadas y Clippy sin advertencias en Windows.
- Prueba nativa con dos monitores al 200% y 100%: inicio, tamaño excesivo, movimiento, maximizar/restaurar, minimizar/restaurar y traslado de una ventana maximizada.

## 0.4.0 — 2026-10-08

### Añadido

- Modal para elegir SSH o una consola local. Conectar sin elegir abre la consola local predeterminada.
- Árbol de archivos remotos con carpetas desplegables y carga de hijos en segundo plano.
- Autenticación SFTP con la contraseña introducida en la terminal SSH.
- Pregunta para guardar el acceso tras autenticar una sesión SSH.
- Contraseñas almacenadas mediante el almacén de credenciales del sistema y reutilizadas al conectar.
- Opción para olvidar la contraseña guardada desde el menú contextual de una conexión.

### Cambiado

- Conexiones muestra únicamente perfiles guardados y el botón Nueva conexión.
- PowerShell, CMD y WSL se eligen dentro del modal.
- La barra lateral conserva el control de conexiones y registros, con indicador activo, y Ajustes.

### Corregido

- El explorador remoto puede autenticar sesiones por contraseña; antes solo admitía claves o agente SSH.
- Validación de longitud de la respuesta de versión SFTP antes de leer sus campos.

### Validación

- 21 pruebas automatizadas y Clippy sin advertencias en Windows.
- SSH/SFTP probado contra un servidor local: contraseña, acceso automático, rechazo de contraseña incorrecta y nombres con espacios y Unicode.
- Guardado, lectura y borrado de una credencial de prueba en el almacén de Windows.
- Compilación de distribución y revisión visual de capturas nativas.

## 0.3.1 — 2026-10-08

### Cambiado

- Eliminados el título Conexiones repetido y el botón + del interior del panel.

## 0.3.0 — 2026-10-08

### Añadido

- Pantalla Inicio sin abrir terminales automáticamente.
- Terminales locales y SSH en pestañas, con acciones de división junto a las pestañas.
- Registros separados de comandos por conexión y actividad de la aplicación.
- Panel de IA compatible con Ollama y APIs de chat compatibles.
- Explorador SFTP para sesiones remotas con claves o agente SSH.
- Controles de ventana integrados, esquinas redondeadas y redimensionado desde los bordes.

### Cambiado

- Archivos aparece al seleccionar una sesión SSH y se oculta en sesiones locales.
- Eliminados el buscador superior, la barra de ruta y los mensajes de bienvenida dentro de la consola.

## 0.2.0

### Añadido

- Modos oscuro y claro, cada uno con tres temas y preferencias independientes.
- Paneles reorganizables mediante arrastre de pestañas.
- Divisiones en filas y columnas, incluidas divisiones anidadas.
- Perfiles locales y SSH guardados, editables y reutilizables.
- Persistencia de distribución y preferencias.
