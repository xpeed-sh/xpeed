# Contribuir a Xpeed

Xpeed es una aplicación nativa en Rust, actualmente en desarrollo. Las comprobaciones locales documentadas se han realizado en Windows; el repositorio también incluye CI para Ubuntu y macOS.

## Preparar el entorno

Necesitas Rust estable y las herramientas de compilación del sistema. En Windows puedes usar Visual Studio Build Tools con MSVC o un entorno GNU compatible. En Ubuntu y macOS consulta las dependencias indicadas en [Desarrollo](DEVELOPMENT.md).

```sh
cargo run
```

En Windows también puedes usar los scripts que detectan el entorno local:

```powershell
.\scripts\dev.ps1
.\scripts\dev.ps1 -Test
.\scripts\package.ps1
```

## Comprobar cambios

Ejecuta las comprobaciones correspondientes al cambio antes de proponerlo:

```sh
cargo fmt --all --check
cargo clippy --workspace --locked --all-targets -- -D warnings
cargo test --workspace --locked
cargo build --locked --release
```

Para cambios visuales, revisa la aplicación o las capturas nativas descritas en DEVELOPMENT.md. Para cambios de terminal o autenticación, comprueba también el comportamiento real con procesos y servidores de prueba locales. Nunca incluyas contraseñas, claves privadas ni datos de servidores personales en pruebas o capturas.

Para comprobar el tamaño de ventana en los monitores conectados a Windows, con Python instalado y la aplicación compilada:

```powershell
py scripts/test-window-size.py target/release/xpeed.exe
```

La prueba crea una instancia temporal de Xpeed, sin cargar perfiles ni guardar preferencias, y comprueba cruces continuos entre pantallas, ausencia de saltos de posición y reducción acumulada, maximización y restauración. El recorrido se simula con movimientos nativos de ventana sin mover el ratón del usuario; los estados de arrastre y la conservación del tamaño preferido también tienen pruebas unitarias. Solo mueve y cierra su propia ventana de prueba.

## Proponer un cambio

- Describe el problema y el comportamiento que modifica.
- Mantén el cambio centrado en ese problema.
- Incluye cómo lo comprobaste y cualquier limitación relevante.
- Actualiza el README cuando cambie el uso de la aplicación.
- Añade una entrada a **Sin publicar** en el changelog.

## Preparar una versión

La versión del paquete se define en `Cargo.toml`. Al preparar una entrega:

1. Actualiza `Cargo.toml` y `VERSION` con el mismo número.
2. Ejecuta Cargo para actualizar la entrada de Xpeed en `Cargo.lock`.
3. Actualiza los nombres del ejecutable en `Xpeed.cmd`, `scripts/package.ps1` y el README.
4. Mueve los cambios de **Sin publicar** a una sección con la versión y la fecha de entrega.
5. Completa las comprobaciones y genera el paquete con `scripts/package.ps1`.

`Cargo.lock` se incluye en Git porque Xpeed es una aplicación. Los binarios generados, dependencias locales y artefactos de prueba se excluyen mediante `.gitignore`.

## Licencia

El código de Xpeed utiliza la [licencia MIT](LICENSE). Las dependencias conservan sus propias licencias, descritas en [THIRD_PARTY.md](THIRD_PARTY.md).

## Extensiones y publicación

Cada adaptador mantiene su propio VERSION, CHANGELOG y manifiesto. La preparación de los repositorios independientes se explica en [GIT-PUBLISHING.md](GIT-PUBLISHING.md).
