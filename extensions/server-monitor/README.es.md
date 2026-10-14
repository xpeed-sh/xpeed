# Monitor del servidor 0.1.2

**Versión:** `0.1.2` · [Changelog](CHANGELOG.md) · [VERSION](VERSION)

La interfaz y la ficha siguen **Settings → Language** de Xpeed 0.7.1: inglés inicialmente y español al seleccionarlo. Puedes cambiar de idioma mientras está conectado; los gráficos y procesos se conservan. La documentación en inglés está en [README.md](README.md).

Extensión opcional para **Xpeed 0.6.15 o posterior**, licencia MIT. Muestra CPU, RAM, uso del disco raíz, tráfico de las primeras cuatro interfaces de red distintas de `lo` y los procesos que más CPU o RAM consumen, con gráficos de hasta seis minutos.

## Usar

Instala **Monitor del servidor** desde Extensiones. Pulsa **Abrir Monitor** y después **Conectar extensión**. El panel contiene las pestañas **Servidor**, con los datos de la conexión, y **Monitor**, con métricas y gráficos. Tiene una sola X para cerrar todo el panel y una acción de expandir/restaurar; no añade un icono al footer. Selecciona una pestaña SSH autenticada: el monitor usa su host, puerto, usuario y acceso. Al cambiar de sesión limpia los gráficos anteriores y cambia de servidor. Una terminal local o una sesión finalizada muestra un estado sin métricas. Al ocultar Monitor o seleccionar Servidor, el muestreo se detiene; reaparece al volver.

Recoge muestras cada tres segundos sobre un canal SSH persistente independiente de la consola, sin introducir comandos en ella. El host conserva las credenciales; la extensión recibe solamente muestras y datos de identificación del servidor. Utiliza la clave privada, el agente SSH o la contraseña ya introducida en la sesión. Si OpenSSH exige otra autenticación interactiva, como MFA, ese canal adicional puede no estar disponible y muestra el error.

Requiere Linux con `/proc`, `sh`, `cat`, `df` y `getconf`. No instala agentes ni necesita `sudo`. Los procesos visibles dependen de los permisos del usuario SSH y de las restricciones de `/proc` del servidor. No modifica archivos, no mata procesos y no recopila argumentos de comandos. Selecciona una fila para consultar sus detalles y copiar el PID; usa los botones CPU/RAM para cambiar el orden.

## Interpretación

- CPU del servidor y de procesos: porcentajes entre muestras, normalizados respecto a la capacidad total del servidor. La primera muestra espera la siguiente para calcular tasas.
- RAM: total menos memoria disponible; RSS para procesos, en bytes según el tamaño de página del servidor.
- Disco: espacio utilizado en el sistema de archivos de `/`; no mide latencia ni operaciones por segundo.
- Red: bytes por segundo de recepción y transmisión, por interfaz, evitando sumar interfaces virtuales y físicas duplicadas.
- Procesos: unión de los 50 mayores por CPU y los 50 mayores por RAM, con 20 filas visibles según el orden elegido. El filtro busca dentro de ese conjunto. Los PIDs reciclados se distinguen por su hora de inicio.
- Los errores se muestran como errores, sin convertir una muestra fallida en valores cero. El canal se reintenta a los diez segundos.

Los contadores y campos se interpretan según la [documentación de procfs del kernel Linux](https://www.kernel.org/doc/html/latest/filesystems/proc.html) y la [referencia de `/proc/PID/stat`](https://www.man7.org/linux/man-pages/man5/proc_pid_stat.5.html).

## Construir

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/prepare-git-repositories.ps1
```

Desde el código de Xpeed, genera `dist/git-repositories/xpeed.server-monitor` con código y binario Windows. Sube esa carpeta a un repositorio público, incluyendo `bin/`. En Xpeed 0.7.1 usa **Extensiones → Git → Conectar repositorio** para ver la ficha e instalar directamente.

```powershell
cargo build --release --locked
```

Este último comando recompila dentro del proyecto independiente. En Windows copia `target/release/xpeed-server-monitor-extension.exe` a `bin/xpeed-server-monitor-extension.exe` antes de publicar. Para Linux/macOS coloca el binario en la ruta de `entrypoints` con permisos de ejecución. Xpeed no compila el código durante la instalación.

## Protocolo

Declara `serverMetrics`, el permiso `server.metrics.read` y la vista derecha `monitor`. El host envía notificaciones `server/context` y `server/sample`; el adaptador responde con `views/updated` y un `dashboard` estructurado. Los identificadores de contexto cambian al seleccionar otro servidor o pausar/reanudar el muestreo, y el host rechaza respuestas del contexto anterior. El núcleo solo acepta datos acotados y finitos para los gráficos. El adaptador no inicia conexiones SSH por su cuenta.

## Versiones

Mantén VERSION, Cargo.toml y extension.json con la misma versión. Actualiza Cargo.lock mediante Cargo, registra cambios del adaptador en CHANGELOG.md y etiqueta las entregas como `vX.Y.Z`. Los cambios de interfaz del host usan la versión de Xpeed. Los proyectos exportados incluyen estos archivos; compila y copia el binario a la ruta del manifiesto antes de publicar.
