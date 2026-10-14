# Actualizaciones de Xpeed — propuesta

## Estado actual

Xpeed 0.10.6 **no tiene un actualizador de la aplicación**, ni un botón para buscar versiones del sistema. Las actualizaciones desde Git implementadas en el gestor corresponden a las extensiones.

Subir commits o etiquetas a `xpeed-sh/xpeed` no actualiza los equipos de los usuarios: necesitan una distribución compilada.

## Fuente: GitHub Releases

Cada versión tendrá una etiqueta `vX.Y.Z` y una Release con instalador Windows `Xpeed-Setup-X.Y.Z-x64.exe`, sumas SHA-256 y notas del CHANGELOG. La web puede enlazar a esas mismas descargas. Linux/macOS se añadirán cuando sus paquetes estén validados.

## Flujo pendiente de implementar

1. **Ajustes → Actualizaciones**: versión instalada y **Buscar actualizaciones**, con acceso también desde Información.
2. Consultar las releases en segundo plano y comparar versiones estables. Mostrar cambios, errores de red y **Descargar actualización**.
3. Descargar el instalador del sistema/arquitectura y comprobar su suma. La suma comprueba integridad respecto del archivo publicado; la firma del instalador aporta verificación adicional del editor.
4. **Instalar y reiniciar**: avisar de las sesiones activas, guardar el estado y cerrar normalmente tras la decisión del usuario. Una comprobación automática nunca cierra terminales. Los procesos remotos no se reanudan automáticamente.
5. Ejecutar el instalador sobre la instalación existente y volver a abrir Xpeed. Conservar configuración, conversaciones y credenciales fuera de la carpeta de instalación.

Primera implementación propuesta: comprobación manual y actualización de instalaciones Windows con el instalador existente. Las instalaciones portátiles requieren un flujo de reemplazo propio; mientras tanto descargarán y sustituirán su paquete manualmente.

## Publicación

Un flujo de compilación podrá generar instalador y sumas desde una etiqueta y adjuntarlos a la Release. El CI actual comprueba compilación y pruebas; ese flujo de publicación aún no existe.

Cada extensión se actualiza desde su propio repositorio y mantiene su versión. Actualizar Xpeed no debe reinstalar las extensiones ni reemplazar las preferencias del usuario. Los instaladores anteriores pueden conservarse para volver atrás, revisando antes la compatibilidad de las migraciones de datos.

Esta es una propuesta documentada: todavía no se ha implementado el panel ni el actualizador, ni publicado una Release desde esta tarea.
