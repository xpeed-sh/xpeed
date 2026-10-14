# Distribuir Xpeed para Windows

## Instalador: compartir un solo .exe

Comparte `dist/installer/Xpeed-Setup-0.10.8-x64.exe`. Instala Xpeed y sus paquetes opcionales en `%LOCALAPPDATA%\Programs\Xpeed`, crea un acceso en Inicio y permite añadir otro al escritorio. Incluye desinstalador, interfaz en inglés y español, documentación y licencias. No solicita permisos de administrador.

Para generar el instalador instala [Inno Setup 6](https://jrsoftware.org/isdl.php) en el equipo de desarrollo y ejecuta:

```powershell
.\scripts\package-installer.ps1
```

Si ya preparaste la carpeta portátil, usa `.\scripts\package-installer.ps1 -SkipBuild`. El script lee la versión de `VERSION`. También admite `-CompilerPath` con la ruta de `ISCC.exe`. El usuario final solo necesita el instalador; no necesita Inno Setup, Rust ni los archivos del proyecto.

Las actualizaciones usan la misma identidad de instalación. El desinstalador conserva las cuentas, conexiones y chats en `%APPDATA%\xpeed`. Los runtimes de proveedores, como Codex, siguen siendo requisitos opcionales independientes.

La entrega actual está en `dist/Xpeed-0.10.8.exe`. La carpeta `dist/portable/Xpeed-0.10.8` reúne `Xpeed.exe`, documentación, licencias y los paquetes opcionales disponibles: puedes compartirla directamente. Es una aplicación portátil para Windows x64: no necesitas instalar Rust en los equipos que la usarán. El icono y los recursos de interfaz van dentro del ejecutable.

## Generar otra entrega

Abre PowerShell en la carpeta del proyecto:

```powershell
.\scripts\package-extension.ps1
.\scripts\package.ps1
.\scripts\prepare-git-repositories.ps1 -SkipBuild
```

El primer comando prepara el paquete independiente de Codex. El segundo compila la aplicación en modo release y copia el ejecutable, documentación, licencias y paquetes opcionales a la carpeta portátil de `dist`. El tercero prepara los repositorios de extensiones para publicarlos por separado. Usa `-SkipBuild` en los scripts de empaquetado solo si ya compilaste esos binarios.

Para compilar todo de una vez:

```powershell
cargo build --workspace --release --locked
.\scripts\package-extension.ps1 -SkipBuild
.\scripts\package.ps1 -SkipBuild
```

En este equipo, los scripts detectan Rust en `.tools` y MinGW en `C:\msys64\mingw64\bin`. En otro equipo de desarrollo instala Rust y un compilador compatible (MSVC con Visual Studio Build Tools o GNU/MinGW); para los usuarios finales no son necesarios.

## Qué compartir

- `Xpeed-0.10.8.exe`, junto con `LICENSE`, `THIRD_PARTY.md`, `DEPENDENCIES.txt` y la carpeta `licenses` de `dist`, para conservar los avisos de dependencias.
- Si quieres distribuir las extensiones locales, incluye `extensions/catalog.json` y `extensions/packages` manteniendo esa estructura junto al ejecutable. La app principal funciona sin Codex instalado.
- Para la distribución por Git, publica `dist/git-repositories/xpeed.codex` como un repositorio de extensión y conecta su enlace desde Extensiones → Git. El usuario instala y abre Codex desde allí.

Cada persona inicia sesión en su propia cuenta de ChatGPT. Codex necesita un runtime oficial compatible instalado en ese equipo; Xpeed busca el ejecutable y permite configurar su ruta desde la ficha de la extensión. Las conexiones SSH requieren OpenSSH Client disponible en Windows.

Los datos personales se crean en `%APPDATA%\xpeed` en cada equipo. Comparte los archivos de distribución, sin copiar esa carpeta personal ni credenciales, `.tools` o `target`.

También se conserva la entrega portátil. El instalador no tiene firma Authenticode. Las pruebas locales se realizaron en Windows; todavía falta comprobarla en un equipo limpio antes de una publicación general.

## Actualizar una instalación

Desde Xpeed 0.10.8, Ajustes → Actualizaciones consulta las Releases estables de `xpeed-sh/xpeed`. Para ofrecer la descarga, adjunta a la Release el instalador con nombre `Xpeed-Setup-X.Y.Z-x64.exe` y usa una etiqueta `vX.Y.Z`. El usuario descarga desde su navegador y ejecuta el instalador cuando esté listo para cerrar las sesiones. No necesita Rust, Git ni recompilar la aplicación. La descarga/instalación automáticas quedan pendientes.
