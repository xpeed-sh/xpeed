# Icono de aplicación

`logo.png` es la imagen suministrada, RGBA de 1254 × 1254. `app-icon.png` es la versión de 256 px para la ventana; `xpeed.ico` contiene 16, 24, 32, 48, 64, 128 y 256 px para Windows. Los archivos derivados conservan la transparencia y están incluidos en el proyecto.

Para regenerarlos: `py scripts/prepare-app-icon.py` (requiere Pillow). Cargo no necesita Python: incrusta el PNG en la aplicación y, en Windows, compila `assets/xpeed.rc` con `rc.exe` del Windows SDK para MSVC o `windres` para GNU. Ambos deben estar disponibles en el PATH del entorno de compilación. `WINDRES` permite indicar otra ruta al compilador de recursos GNU.

El icono del sistema utiliza los colores de la imagen original. El logo SVG dentro de la interfaz sigue los colores del tema.
