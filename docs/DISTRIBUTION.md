# Distribuir Xpeed para Windows

## Instalador: compartir un solo .exe

Comparte `dist/installer/Xpeed-Setup-0.11.0-x64.exe`. Instala Xpeed en `%LOCALAPPDATA%\Programs\Xpeed`, crea un acceso en Inicio y permite añadir otro al escritorio. Incluye el catálogo de extensiones, desinstalador, interfaz en inglés y español, documentación y licencias. Las extensiones se instalan después desde sus repositorios Git públicos. No solicita permisos de administrador.

Para generar el instalador instala [Inno Setup 6](https://jrsoftware.org/isdl.php) en el equipo de desarrollo y ejecuta:

```powershell
.\scripts\package-installer.ps1
```

Si ya preparaste la carpeta portátil, usa `.\scripts\package-installer.ps1 -SkipBuild`. El script lee la versión de `VERSION`. También admite `-CompilerPath` con la ruta de `ISCC.exe`. El usuario final solo necesita el instalador; no necesita Inno Setup, Rust ni los archivos del proyecto.

Las actualizaciones usan la misma identidad de instalación. El desinstalador conserva las cuentas, conexiones y chats en `%APPDATA%\xpeed`. Los runtimes de proveedores, como Codex, siguen siendo requisitos opcionales independientes.

La entrega actual está en `dist/Xpeed-0.11.0.exe`. La carpeta `dist/portable/Xpeed-0.11.0` reúne `Xpeed.exe`, documentación, licencias y el catálogo Git: puedes compartirla directamente. Es una aplicación portátil para Windows x64; no necesitas instalar Rust en los equipos que la usarán. El icono y los recursos de interfaz van dentro del ejecutable.

## Generar otra entrega

Abre PowerShell en la carpeta del proyecto:

```powershell
.\scripts\package.ps1
.\scripts\prepare-git-repositories.ps1 -SkipBuild
```

El primer comando compila la aplicación y prepara la carpeta portátil con su catálogo. El segundo regenera los proyectos independientes de Codex y Monitor para sus repositorios. Usa `-SkipBuild` únicamente si los binarios correspondientes ya están compilados.

Para compilar todo de una vez:

```powershell
cargo build --workspace --release --locked
.\scripts\package.ps1 -SkipBuild
.\scripts\prepare-git-repositories.ps1 -SkipBuild
```

En este equipo, los scripts detectan Rust en `.tools` y MinGW en `C:\msys64\mingw64\bin`. En otro equipo de desarrollo instala Rust y un compilador compatible (MSVC con Visual Studio Build Tools o GNU/MinGW); para los usuarios finales no son necesarios.

## Qué compartir

- `Xpeed-0.11.0.exe`, junto con `LICENSE`, `THIRD_PARTY.md`, `DEPENDENCIES.txt` y la carpeta `licenses` de `dist`, para conservar los avisos de dependencias.
- `extensions.json` contiene las extensiones disponibles y sus repositorios. La entrega no incluye una carpeta `extensions`: Xpeed la crea en los datos del usuario cuando instala la primera extensión.
- Codex y Monitor se instalan desde sus repositorios públicos mediante el catálogo incluido. La aplicación principal funciona sin extensiones instaladas.

Cada persona inicia sesión en su propia cuenta de ChatGPT. Codex necesita un runtime oficial compatible instalado en ese equipo; Xpeed busca el ejecutable y permite configurar su ruta desde la ficha de la extensión. Las conexiones SSH requieren OpenSSH Client disponible en Windows.

Los datos personales se crean en `%APPDATA%\xpeed` en cada equipo. Comparte los archivos de distribución, sin copiar esa carpeta personal ni credenciales, `.tools` o `target`.

También se conserva la entrega portátil. El instalador no tiene firma Authenticode. Las pruebas locales se realizaron en Windows; todavía falta comprobarla en un equipo limpio antes de una publicación general.

## Actualizar una instalación

Desde Xpeed 0.11.0, Ajustes → Actualizaciones consulta automáticamente las Releases estables de `xpeed-sh/xpeed`. Si existe una versión nueva, descarga el instalador oficial con progreso, comprueba el tamaño y la suma SHA-256 publicada por GitHub y permite abrirlo desde Xpeed. Los ajustes, conexiones y extensiones del usuario se conservan. No necesita Rust, Git ni recompilar la aplicación.

Al enviar una etiqueta `vX.Y.Z`, `.github/workflows/release.yml` comprueba que coincida con `VERSION`, compila el instalador, genera `SHA256SUMS.txt` y publica ambos archivos en GitHub Releases. El actualizador solo acepta el nombre exacto `Xpeed-Setup-X.Y.Z-x64.exe`, el repositorio oficial y una suma SHA-256 válida.
