# Iconos SVG

`settings.svg`, `workspace.svg` y `extensions.svg` son los SVG suministrados para la barra lateral. `sidebar-left.svg` y `sidebar-right.svg` corresponden a los botones superiores, `connection.svg` a las conexiones guardadas y `logo.svg` a la marca. Se conservan sus paths y opacidades; la aplicación sustituye el color de relleno por el del tema y utiliza el acento para hover y selección. El logo conserva su proporción y su viewBox recorta el margen vacío.

`home.svg` indica la carpeta inicial del explorador; al navegar se sustituye por la flecha de volver.

`minimize.svg`, `maximize.svg` y `close.svg` son los controles de ventana suministrados. Se dibujan a 22 px, con opacidad original y color del tema.

`moon.svg` y `sun.svg` sustituyen los textos Oscuro/Claro en Ajustes. El selector mantiene el estado activo, etiquetas accesibles y tooltips; el sol conserva sus ocho rayos como contornos independientes.

`file.svg`, `folder.svg`, `image.svg`, `audio.svg` y `video.svg` se usan en el explorador de archivos, según el tipo y la extensión. Se conservan los dos huecos de las notas musicales mediante triangulación de contornos. Los colores se adaptan al tema; las carpetas usan el acento.

`py scripts/compile-icons.py` convierte estos paths en mallas vectoriales para `src/icon_meshes.rs`; después ejecuta `cargo fmt --all`. Para regenerar el logo o el icono de audio instala primero `py -m pip install -r scripts/svg-build-requirements.txt`. Son herramientas de generación: compilar o usar Xpeed no las requiere, porque las mallas están incluidas. No requiere añadir un motor SVG al ejecutable ni convertirlos a imágenes de tamaño fijo.

El importador de iconos admite paths con comandos absolutos M/L/H/V/C/Z y polígonos sin agujeros; el icono de audio suministrado incluye soporte explícito para sus dos huecos. `compile_logo.py` añade M/L/C/Z relativos, múltiples contornos con huecos y la transformación concreta del logo suministrado. Otros SVG con trazos, transformaciones o curvas distintas necesitan ampliar el importador antes de usarlos. Los demás iconos siguen dibujándose como antes.
