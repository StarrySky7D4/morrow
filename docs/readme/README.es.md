# Morrow · 明隙

[简体中文（默认）](../../README.md) · [English](../../docs/readme/README.en.md) · [Русский](../../docs/readme/README.ru.md) · [Français](../../docs/readme/README.fr.md) · [Deutsch](../../docs/readme/README.de.md) · **Español** · [日本語](../../docs/readme/README.ja.md) · [한국어](../../docs/readme/README.ko.md) · [Português](../../docs/readme/README.pt.md)

Deja un poco de espacio para las ideas de mañana.

Morrow (明隙) es un espacio de trabajo local basado en tarjetas que evoluciona hacia una arquitectura de plugins multiplataforma. Antes se llamaba daemon; el paquete es `morrow_studio`. En Windows, Flutter proporciona la interfaz y un anfitrión Rust con plugins Wasm aislados ejecuta la lógica. Web y Android mantienen la implementación anterior; el sistema de plugins aún no está validado en todas las plataformas.

## Descarga y compatibilidad

Versión actual: **0.1.9-test.54+58**, una **versión preliminar de prueba para Windows x64**, no estable. Descarga el ZIP de Windows, el ZIP del código fuente correspondiente y la lista SHA-256. Extrae todo y ejecuta `morrow_studio.exe`; conserva las DLL, el anfitrión, `data`, `plugins` y las licencias.

[Descargar test.54](https://github.com/StarrySky7D4/morrow/releases/tag/v0.1.9-test.54) · [Versión compatible test.1](https://github.com/StarrySky7D4/morrow/releases/tag/v0.1.9-test.1)

`test.1` es la última versión de prueba `0.1.x` compatible con los tipos de datos originales. Las posteriores avanzan en la reescritura y pueden introducir incompatibilidades. `0.2.0` llegará tras estabilizar y validar la arquitectura y el modelo de datos. Los datos de test.1 no se importan ni sobrescriben automáticamente. Antes de actualizar, guarda la biblioteca y su archivo de protección original. La protección está vinculada al usuario de Windows; copiar solo la base de datos no permite migrarla entre cuentas.

## Funciones

- Tarjetas: ideas, proyectos, experimentos, favoritos, búsquedas, listas y deshacer eliminaciones; edición Markdown con vista previa y copias independientes de los adjuntos.
- Captura enriquecida: texto, capturas de pantalla, archivos, texto con formato y tablas de Office. Los formatos complejos o propietarios pueden conservarse como vistas previas o adjuntos, sin garantizar el diseño original.
- Aspecto: cristal esmerilado, ultratransparente y líquido; fondos predeterminados, lisos, texturizados o transparentes; rueda cromática y ajustes por tarjeta o componente, con movimiento reducido.
- Multimedia: fondos de imagen, GIF y vídeo, música local, letras y consejos flotantes. Los formatos dependen de la plataforma y el decodificador. La transparencia web muestra la página anfitriona, no el escritorio.
- Diseño e idiomas: contenido adaptable y páginas de ajustes separadas; chino, inglés, ruso, francés, alemán, español, japonés, coreano y portugués.
- Protección en Windows: almacenamiento unificado en Rust, sellado de registros de auditoría, instantáneas, copia y recuperación de la identidad original, y uso simultáneo limitado de una misma identidad.

## Optimización y validación

test.54 elimina comprobaciones completas repetidas al abrir bibliotecas, decodifica evidencias con paralelismo acotado y reutiliza tamaños y resúmenes de archivos verificados en una misma transacción. Cada apertura sigue verificándose por completo. No cambian el formato de almacenamiento ni el plugin integrado; no se conserva una caché de validación entre inicios.

La última optimización de lectura se comparó con la versión paralela anterior mediante cuatro inicios alternos por versión. En el mismo equipo y con una copia de unos 100 MB, la mediana hasta retirar la pantalla de carga pasó de 2.222 a 1.299 s desde la entrada de Dart. Registrar la copia puede precalentar la caché; no garantiza un arranque en frío con caché vacía. Superadas: 568 pruebas Core, 97 Audit y 2 de integración con anfitrión real; una prueba Core preexistente omitida. Consulta las condiciones en las notas de versión.

## Ejecutar desde el código fuente

Requiere Flutter 3.44 / Dart 3.12 o versiones compatibles. Windows también necesita las herramientas de compilación de escritorio C++ de Visual Studio, Windows SDK, Rust y el compilador Cap’n Proto en PATH.

Compilación completa de Windows Rust, pruebas de integración y empaquetado:

```powershell
flutter pub get
rustup target add wasm32-unknown-unknown
pwsh -File tool/build_rust_workbench_windows.ps1
```

Los artefactos no se sobrescriben: utiliza una nueva versión o un directorio de salida nuevo. `-RefreshArtifact` ya no permite reemplazarlos. Distribuye todo el directorio de ejecución. La documentación de cada plataforma define la validación Web/Android; compilar para Windows no la sustituye.

Desarrollo y comprobaciones básicas:

```powershell
flutter run -d windows
flutter run -d chrome
flutter analyze
flutter test
flutter build web --no-web-resources-cdn
```

## Arquitectura, SDK y próximos pasos

La arquitectura objetivo combina interfaz Flutter/Dart, núcleo Rust portátil y motores de plugins intercambiables. Las fronteras de ejecución usan contratos fijos; la persistencia y los intercambios propios usan Protobuf + LZ4. Los SDK C/C++/Rust y la interfaz declarativa de plugins siguen en desarrollo. No se admiten plugins TS/JS ni se requieren plugins Dart dinámicos.

El SDK completo aún no está congelado. Se han conectado solicitudes HTTP/HTTPS controladas, nodos de servicio API limitados y gestión de identidades TLS. Quedan la conciliación de resultados Unknown entre reinicios, el sistema de archivos completo, los SDK IO en tres lenguajes y la validación multiplataforma. La apertura aún recorre todo el historial; faltan la canalización completa de lectura/cálculo, los niveles de almacenamiento histórico y la limpieza automática.

## Documentación

- [Notas de versión y validación](../../reports/0.1.9-test.54-release.md)
- [Tablero de desarrollo](../../docs/DEVELOPMENT_BOARD.md)
- [Hoja de ruta de arquitectura](../../docs/FUTURE_ROADMAP.md)
- [Migración de funciones y límites](../../docs/TEST1_RUST_PARITY.md)
- [SDK C/C++/Rust](../../sdk/README.md)
- [Interfaz de plugins](../../docs/PLUGIN_SDK_AND_UI.md)
- [Captura y formatos](../../docs/RICH_CAPTURE.md)
- [Compilación Android](../../docs/ANDROID.md)
- [Compatibilidad del cambio de nombre](../../docs/RENAMING.md)
- [README histórico e hitos](../../docs/history/README-before-test54.md)

Los diseños detallados y los informes están principalmente en chino. Los README describen la misma versión actual; las traducciones aún no cuentan con una revisión exhaustiva de hablantes nativos.

## Licencia

Desde `0.1.9-test.2`, el código, SDK, documentación, configuración y recursos propios usan **AGPL-3.0-only**. test.1 y versiones anteriores conservan Apache-2.0; el contenido de terceros mantiene sus licencias. Los paquetes incluyen licencias, avisos de derechos de autor y acceso al código fuente correspondiente.

[AGPL-3.0-only](../../LICENSE) · [NOTICE](../../NOTICE) · [Third-party licenses](../../packaging/THIRD_PARTY_NOTICES.txt)
