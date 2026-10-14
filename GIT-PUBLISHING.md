# Publicar los proyectos en Git

Se preparan tres repositorios públicos de la organización `xpeed-sh`:

| Proyecto | Repositorio previsto | Carpeta que se sube | Versión |
| --- | --- | --- | --- |
| Xpeed | `xpeed-sh/xpeed` | Raíz de este proyecto | `VERSION` |
| Codex | `xpeed-sh/extension-codex` | `dist/git-repositories/xpeed.codex` | Su propio `VERSION` |
| Server monitor | `xpeed-sh/extension-server-monitor` | `dist/git-repositories/xpeed.server-monitor` | Su propio `VERSION` |

## Preparar

Desde la raíz de Xpeed:

```powershell
.\scripts\package-extension.ps1
.\scripts\package-monitor.ps1
.\scripts\prepare-git-repositories.ps1
```

El último script genera proyectos independientes con fuente, Cargo.lock, README en inglés/español, VERSION, CHANGELOG, manifiesto, biblioteca de traducción, licencias y ejecutable Windows. No crea repositorios ni publica archivos. Los binarios de Linux/macOS requieren compilar en su plataforma.

Por ahora los fuentes de ambas extensiones siguen en el workspace de Xpeed para conservar la compilación y sus pruebas. Las carpetas exportadas funcionan sin el repositorio principal; no copies todo Xpeed dentro de los repositorios de extensiones. La separación definitiva del workspace puede hacerse después de la primera publicación.

## Primera publicación

Crea los tres repositorios vacíos en GitHub. Dentro de cada carpeta de la tabla, usa la URL correspondiente:

```powershell
git init -b main
git add .
git diff --cached --stat
git status --short
git commit -m "Initial public source"
git remote add origin https://github.com/xpeed-sh/REPOSITORY.git
git push -u origin main
```

Sustituye `REPOSITORY` antes de ejecutar. Revisa lo preparado para el commit: no añadas perfiles personales, claves, tokens ni archivos de configuración del usuario.

En Xpeed, `.gitignore` excluye `.tools`, `target`, `artifacts`, `dist` y los paquetes generados. `Cargo.lock` sí se sube. Publica el instalador `.exe` como archivo de una GitHub Release, fuera del historial del código.

En las extensiones, **sí se sube `bin/`**, porque Xpeed instala directamente desde Git sin compilar. Incluye sus licencias y publica solo los ejecutables indicados por `extension.json`.

## Versiones futuras

Cada proyecto mantiene VERSION, la versión de `[package]` en Cargo.toml y su entrada en Cargo.lock. Cada extensión también mantiene `extension.json` con la misma versión. Su CHANGELOG recoge los cambios del adaptador; los cambios de la interfaz que dibuja el host pertenecen a Xpeed.

Anota los cambios pendientes en **Sin publicar**. Para una entrega, crea una sección con versión y fecha, compila, comprueba y etiqueta ese repositorio con `vX.Y.Z`. Cambiar Xpeed no exige aumentar la versión de ambas extensiones si sus adaptadores no cambian.

Cuando los repositorios de extensiones estén publicados y comprobados, usa `extensions/catalog.git.example.json` como base para el catálogo remoto `extensions/catalog.json`. Hasta entonces conserva el catálogo local. El diseño de las actualizaciones se documenta en [UPDATES-PROPOSAL.md](UPDATES-PROPOSAL.md).

## Qué se incluye en el repositorio principal

La carpeta `extensions/` sí se sube: contiene los fuentes del workspace y el catálogo. `extensions/packages/` no se sube: son paquetes generados. Los proyectos independientes para los dos repositorios se toman de `dist/git-repositories/`, no de las carpetas fuente sin exportar. El contrato técnico del host está en [docs/EXTENSIONS.md](docs/EXTENSIONS.md).

Subir commits o crear una etiqueta no actualiza una aplicación instalada. Las futuras actualizaciones de Xpeed usarán el instalador de una GitHub Release; consulta [UPDATES-PROPOSAL.md](UPDATES-PROPOSAL.md).
