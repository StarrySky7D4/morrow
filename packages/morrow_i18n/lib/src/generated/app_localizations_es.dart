// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Spanish Castilian (`es`).
class AppLocalizationsEs extends AppLocalizations {
  AppLocalizationsEs([String locale = 'es']) : super(locale);

  @override
  String get commonAppName => 'Morrow';

  @override
  String get commonCancel => 'Cancelar';

  @override
  String commonCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count elementos',
      one: '$count elemento',
      zero: 'Sin elementos',
    );
    return '$_temp0';
  }

  @override
  String commonGreeting(String name) {
    return 'Hola, $name';
  }

  @override
  String get importsAttachmentLimit =>
      'Puedes importar 20 adjuntos a la vez. Pega el resto por separado.';

  @override
  String get importsClipboardChanged =>
      'El portapapeles cambió durante la lectura. Pega de nuevo.';

  @override
  String get importsEmbeddedImageUnreadable =>
      'No se pudo leer una imagen incrustada.';

  @override
  String get importsEmbeddedImagesSeparate =>
      'Algunas imágenes incrustadas deben importarse por separado.';

  @override
  String get importsExcelValues =>
      'Valores y fórmulas convertidos a Markdown. El formato y las celdas combinadas se conservan en el adjunto XML.';

  @override
  String get importsExcelXmlKept =>
      'La tabla original de Excel se conserva como adjunto XML.';

  @override
  String get importsFileTooLarge =>
      'El archivo del portapapeles supera los 200 MB.';

  @override
  String get importsItemLimit =>
      'Solo se leyeron los primeros 20 elementos. Pega los demás por separado.';

  @override
  String get importsItemUnreadable =>
      'No se pudo leer un elemento del portapapeles. Se conserva el resto del contenido legible.';

  @override
  String get importsLocalImageNotRead =>
      'Las imágenes locales vinculadas no se leen automáticamente. Pega la imagen o importa el archivo original.';

  @override
  String get importsMergedTable =>
      'Celdas combinadas convertidas en tabla legible. El formato original se conserva en el adjunto HTML.';

  @override
  String get importsOfficeBusy =>
      'Otra aplicación está usando el portapapeles. Los objetos de Office no se leyeron.';

  @override
  String get importsOfficeEmbeddedKept =>
      'El objeto de Office se conserva como adjunto original. Edita gráficos, fórmulas y diseño en la aplicación original.';

  @override
  String get importsOfficeExportFailed =>
      'El objeto de Office supera el límite o no pudo exportarse. Guárdalo en la aplicación original e impórtalo.';

  @override
  String get importsOfficeReadFailed =>
      'No se pudo leer el contenido de Office. El resto del portapapeles sigue disponible.';

  @override
  String get importsOfficeUnavailable =>
      'El portapapeles de Office no está disponible temporalmente.';

  @override
  String get importsOfficeUnreadable =>
      'No se pudo leer el objeto original de Office. Se conserva el resto disponible.';

  @override
  String get importsRichFallback =>
      'Parte del formato no se pudo convertir. Se conserva el texto legible.';

  @override
  String get importsRichTooLarge =>
      'El texto enriquecido supera 2 MB. Importa el documento como adjunto.';

  @override
  String get importsRtfTooLarge =>
      'El contenido RTF es demasiado grande. Importa el documento original.';

  @override
  String get importsSpreadsheetTooLarge =>
      'La tabla es demasiado grande. Importa el archivo de Excel.';

  @override
  String get importsTableConverted =>
      'Tabla convertida a Markdown. Los datos completos se conservan en el adjunto TSV.';

  @override
  String get importsTextTooLarge =>
      'El texto supera 2 MB. Impórtalo como archivo.';

  @override
  String get importsTotalTooLarge =>
      'Los archivos pegados superan 200 MB en total. Impórtalos en grupos más pequeños.';

  @override
  String get importsUnsupported =>
      'Aquí no se admite el portapapeles. Importa un archivo.';

  @override
  String get mainActiveProjects => 'En curso';

  @override
  String get mainAdjustCustomTone => 'Ajustar tono';

  @override
  String get mainAmbientDetail => 'La luz fluida aporta un toque de color.';

  @override
  String get mainAppTitle => 'Morrow — Espacio para ideas';

  @override
  String get mainAppearance => 'Apariencia';

  @override
  String get mainArrangeIdeas => 'Ordenar ideas';

  @override
  String mainAttachmentCount(int count) {
    return 'Adjuntos · $count';
  }

  @override
  String mainAttachmentHint(int count, String name) {
    return '$count adjuntos · $name';
  }

  @override
  String get mainAttachmentLimit => 'Hasta 20 adjuntos por registro.';

  @override
  String get mainAutosaveNotice =>
      'La apariencia y las ideas se guardan localmente';

  @override
  String get mainAwaitDiscovery => 'Esperando un descubrimiento';

  @override
  String get mainBackToWorkbench => 'Volver al espacio de trabajo';

  @override
  String get mainBackgroundCanvas => 'Lienzo de fondo';

  @override
  String get mainBackgroundSound => 'Reproducir audio de fondo';

  @override
  String get mainBodyHint =>
      'Escribe tus ideas o pega contenido…\n\nAdmite # títulos, listas, tablas y bloques de código';

  @override
  String get mainBrightWhite => 'Blanco';

  @override
  String get mainBuiltinTexture => 'Textura integrada';

  @override
  String get mainCanvasCompass => 'Rueda de tinte del fondo';

  @override
  String get mainCaptureIdea => 'Guardar idea';

  @override
  String get mainCaptureNow => 'Anotar una idea';

  @override
  String mainCardAttachments(int count, String name) {
    return 'Archivos $count · $name';
  }

  @override
  String get mainCategoryExperiment => 'Experimento';

  @override
  String get mainCategoryIdea => 'Idea';

  @override
  String get mainCategoryProject => 'Proyecto';

  @override
  String get mainCategoryPrompt => 'Dónde guardarla';

  @override
  String get mainChangeFailed =>
      'No se guardó el cambio. El borrador se conserva; puedes reintentar.';

  @override
  String get mainCheckAgain => 'Volver a comprobar';

  @override
  String get mainClearSearch => 'Borrar búsqueda';

  @override
  String get mainClipboardEmpty =>
      'No hay texto ni archivos legibles en el portapapeles. Copia un archivo desde el gestor de archivos o usa Importar archivo.';

  @override
  String get mainClipboardReadFailed =>
      'No se pudo leer el contenido. Importa un archivo o comprueba los permisos del archivo y del portapapeles.';

  @override
  String get mainClipboardSupport =>
      'Admite Markdown, texto enriquecido y tablas de Office, capturas y archivos. Los objetos complejos conservan sus adjuntos originales. Hasta 20 adjuntos de 200 MB cada uno.';

  @override
  String get mainCollapseSidebar => 'Contraer barra lateral';

  @override
  String get mainCompletedProjects => 'Completados';

  @override
  String get mainComponentCompass => 'Rueda de tinte del componente';

  @override
  String get mainComponentEmpty => 'Estado vacío';

  @override
  String get mainComponentFooter => 'Consejos y letras al pie';

  @override
  String get mainComponentHero => 'Tarjeta de resumen';

  @override
  String get mainComponentNavigation => 'Navegación lateral';

  @override
  String get mainComponentQuickCapture => 'Nota rápida';

  @override
  String get mainComponentSearch => 'Barra de búsqueda';

  @override
  String get mainComponentSettings => 'Componentes y tarjetas · Ajustes';

  @override
  String get mainContentProtection => 'Protección del contenido';

  @override
  String get mainContentRead => 'Contenido leído';

  @override
  String mainContentReadFiles(int count) {
    return 'Contenido leído; $count adjuntos conservados';
  }

  @override
  String get mainCornerRadius => 'Radio de esquinas';

  @override
  String get mainCredentialSettings => 'Credenciales';

  @override
  String get mainCrystal => 'Cristalino';

  @override
  String get mainCrystalDetail =>
      'Ligero y transparente, dejando brillar el color.';

  @override
  String get mainCuriosity =>
      'Las cosas interesantes empiezan\ncon un poco de curiosidad.';

  @override
  String get mainCustomCompass => 'Rueda de color · Personalizado';

  @override
  String get mainCustomLightness => 'Luminosidad personalizada';

  @override
  String get mainCustomTheme => 'Personalizado';

  @override
  String get mainDaily => 'Pequeñas cosas';

  @override
  String get mainDailyExplore => 'Dedica diez minutos a explorar';

  @override
  String get mainDailyIdea => 'Anota una idea';

  @override
  String get mainDailyWater => 'Sírvete un vaso de agua';

  @override
  String get mainDarkTheme => 'Oscuro';

  @override
  String get mainDeepBlack => 'Negro';

  @override
  String get mainDefaultCanvas => 'Predeterminado';

  @override
  String get mainDefaultGlobalColor =>
      'Color predeterminado · Todos los controles';

  @override
  String get mainDelete => 'Eliminar';

  @override
  String mainDeleted(String title) {
    return '«$title» eliminado';
  }

  @override
  String get mainDiagnosticDetails => 'Detalles de diagnóstico';

  @override
  String get mainDone => 'Listo';

  @override
  String get mainEdit => 'Editar';

  @override
  String get mainEditIdeaTitle => 'Aclara tu idea';

  @override
  String get mainEditorClosedUnknown =>
      'El editor se cerró, pero el guardado no está confirmado. Reabre el espacio de trabajo y compruébalo antes de crear otra copia.';

  @override
  String get mainEditorSubtitle =>
      'Textos, tablas, imágenes: guárdalos aquí mientras tu idea toma forma.';

  @override
  String get mainEditorUnavailable =>
      'Editor no disponible. Comprueba el servicio de contenido y reintenta.';

  @override
  String get mainEndpointSettings => 'Puntos de conexión salientes';

  @override
  String get mainExpandSettings => 'Expandir ajustes';

  @override
  String get mainExpandSidebar => 'Expandir barra lateral';

  @override
  String get mainExtensionPlugins => 'Extensiones';

  @override
  String get mainFavoriteAttachments => 'Adjuntos favoritos';

  @override
  String get mainFavoriteRecords => 'Registros favoritos';

  @override
  String mainFavoriteTooltip(String title) {
    return 'Añadir $title a favoritos';
  }

  @override
  String get mainFavoritesIntro =>
      'Tus textos, imágenes y archivos favoritos en un lugar.';

  @override
  String mainFieldLimit(int limit) {
    return 'Máximo $limit caracteres. Acorta el contenido o impórtalo como archivo.';
  }

  @override
  String get mainFilterAll => 'Todos';

  @override
  String get mainFilterAttachments => 'Con archivos';

  @override
  String get mainFilterFavorites => 'Solo favoritos';

  @override
  String get mainFilterFile => 'Archivos';

  @override
  String get mainFilterImage => 'Imágenes';

  @override
  String get mainFilterMedia => 'Audio / vídeo';

  @override
  String get mainFilterPending => 'Pendientes';

  @override
  String get mainFilterText => 'Texto';

  @override
  String get mainFollowTheme => 'Seguir tema';

  @override
  String get mainFrostDetail =>
      'Suaviza el fondo y deja espacio para tus ideas.';

  @override
  String get mainFrostEffect => 'Desenfoque';

  @override
  String get mainFrostOpacity => 'Opacidad del cristal';

  @override
  String get mainFrostUnavailable =>
      'El desenfoque del escritorio no está disponible. Puedes ajustar el tinte y la opacidad.';

  @override
  String get mainFrosted => 'Esmerilado';

  @override
  String get mainGlassTexture => 'Estilo de cristal';

  @override
  String mainGlobalColor(String color) {
    return '$color · Todos los controles';
  }

  @override
  String get mainGreeting => 'Deja crecer tus ideas.';

  @override
  String get mainGreetingDetail =>
      'Guarda los detalles cotidianos y las chispas de inspiración.';

  @override
  String get mainHeroBody =>
      'Un pensamiento, una tarea, un «¿y si…?».\nAquí empiezan.';

  @override
  String get mainHeroCaption => 'EL RINCÓN DE LAS POSIBILIDADES';

  @override
  String get mainHeroTitle => 'Está bien empezar poco a poco.';

  @override
  String get mainHideAppearance => 'Ocultar ajustes de apariencia';

  @override
  String get mainHideCustomTone => 'Ocultar tono personalizado';

  @override
  String get mainHidePreview => 'Ocultar vista previa';

  @override
  String get mainHttpSettings => 'Tareas HTTP';

  @override
  String get mainHypothesis => 'Hipótesis';

  @override
  String get mainHypothesisPrompt => 'Hipótesis a probar';

  @override
  String get mainHypothesisSection => 'Hipótesis / Qué probar';

  @override
  String get mainIdeaDetails =>
      'Guarda los detalles. Aclara el siguiente paso.';

  @override
  String get mainIdeaNameHint => 'Dale un nombre';

  @override
  String get mainIdeaNameRequired => 'Escribe primero tu idea';

  @override
  String get mainIdeaSaved => 'Idea guardada.';

  @override
  String get mainImportFailed =>
      'No se pudo importar el archivo multimedia. Comprueba el archivo y el espacio disponible.';

  @override
  String get mainImportFile => 'Importar archivo';

  @override
  String get mainInboxIntro =>
      'Primero captura, después organiza. Convierte buenas ideas en pequeños proyectos.';

  @override
  String get mainIoNoDeclarations =>
      'Ningún complemento instalado declara acceso a archivos o red.';

  @override
  String get mainIoSettings => 'Red y archivos';

  @override
  String get mainIoSettingsGuide =>
      'Gestiona los permisos de archivos y red por separado de la apariencia. Aprobar una capacidad no da acceso a todos los archivos o puntos de conexión; las operaciones dependen del motor actual.';

  @override
  String get mainIoSettingsSummary =>
      'Permisos, credenciales, puntos de conexión y servicios API';

  @override
  String get mainJustNow => 'Ahora mismo';

  @override
  String get mainLabIntro =>
      'Empieza con una hipótesis. Guarda intentos, observaciones y sorpresas.';

  @override
  String get mainLanguage => 'Idioma';

  @override
  String get mainLanguageChinese => '简体中文';

  @override
  String get mainLanguageEnglish => 'English';

  @override
  String get mainLanguageSystem => 'Idioma del sistema';

  @override
  String get mainLavender => 'Lavanda';

  @override
  String get mainLightOpacity => '20 % · Ligero';

  @override
  String get mainLiquidAllCanvases =>
      'Disponible por separado en los cuatro tipos de fondo';

  @override
  String get mainLiquidDetail =>
      'Reflejos fluidos y refracción suave, como una gota de agua suspendida.';

  @override
  String get mainLiquidEffect => 'Efecto de cristal líquido';

  @override
  String get mainLiquidGlass => 'Cristal líquido';

  @override
  String get mainLivePreview => 'Vista previa en vivo';

  @override
  String get mainLocalMedia => 'Medios locales';

  @override
  String get mainMakeYours => 'A TU MANERA';

  @override
  String get mainMarkOrganized => 'Marcar como organizado';

  @override
  String get mainMarkdownBody => 'Texto · Markdown';

  @override
  String get mainMediaLimits => 'Imágenes / GIF ≤ 25 MB; vídeos ≤ 150 MB';

  @override
  String get mainMonochrome => 'Monocromo';

  @override
  String mainMoreSteps(int count) {
    return '$count pasos más; abre para verlos';
  }

  @override
  String mainMovedProject(String title) {
    return '«$title» se movió a Proyectos';
  }

  @override
  String get mainMusic => 'Reproductor de música';

  @override
  String get mainMySpace => 'Mi espacio';

  @override
  String get mainNavigation => 'Navegación';

  @override
  String get mainNewIdea => 'Nueva idea';

  @override
  String get mainNewIdeaTitle => 'Captura una nueva idea';

  @override
  String get mainNoHypothesis => 'Aún no hay hipótesis';

  @override
  String get mainNoMatches => 'No hay ideas coincidentes';

  @override
  String get mainNoResultYet =>
      'El resultado puede esperar. El proceso también merece registrarse.';

  @override
  String get mainNotNow => 'Ahora no';

  @override
  String get mainObservationSection => 'Observaciones / Qué descubriste';

  @override
  String get mainObservations => 'Observaciones y conclusiones';

  @override
  String get mainObservationsPrompt => 'Observaciones, proceso y conclusiones';

  @override
  String get mainOneHourAgo => 'Hace 1 hora';

  @override
  String get mainOnlineMedia => 'Medios en línea';

  @override
  String get mainOpaqueFallback =>
      'Paneles transparentes sobre el color del tema actual.';

  @override
  String get mainOpenNextStep =>
      'Abre el proyecto para editar los próximos pasos';

  @override
  String get mainOrganizedCount => 'Organizados';

  @override
  String get mainOriginalColors => 'Colores originales';

  @override
  String get mainPageFavorites => 'Favoritos';

  @override
  String get mainPageInbox => 'Bandeja de entrada';

  @override
  String get mainPageLaboratory => 'Laboratorio';

  @override
  String get mainPageOverview => 'Resumen';

  @override
  String get mainPageProjects => 'Proyectos';

  @override
  String mainPageSummary(String page) {
    return '$page · Resumen';
  }

  @override
  String get mainPasteChanged =>
      'La entrada cambió durante el pegado. Vuelve a abrir el editor.';

  @override
  String get mainPasteContent => 'Pegar contenido';

  @override
  String get mainPause => 'Pausa';

  @override
  String get mainPersonalWorkspace => 'Espacio personal';

  @override
  String get mainPlay => 'Reproducir';

  @override
  String get mainPluginSettings => 'Complementos y servicios';

  @override
  String get mainPluginSettingsSummary =>
      'Herramientas integradas, extensiones, red, archivos y protección del contenido';

  @override
  String get mainPreviewEmpty => 'La vista previa aparecerá aquí';

  @override
  String mainProgress(int done, int total) {
    return 'Pequeños pasos · $done/$total';
  }

  @override
  String get mainProjectIntro =>
      'Avanza con listas de tareas. Cada pequeño paso te acerca a la meta.';

  @override
  String get mainQueryAgain => 'Nueva consulta';

  @override
  String get mainQueryCapacity => 'Historial de consultas lleno';

  @override
  String get mainQueryCapacityDetail =>
      'El contenido se conserva. Esta versión aún no permite borrar el historial de consultas.';

  @override
  String get mainQueryLoading => 'Buscando ideas…';

  @override
  String get mainQueryRetry => 'Reintentar consulta';

  @override
  String get mainQueryTerminated => 'Esta consulta ha terminado';

  @override
  String get mainQueryUnknown => 'Resultados aún sin confirmar';

  @override
  String get mainQuickHint => '¿Qué se te acaba de ocurrir?';

  @override
  String get mainReadOnlySettings =>
      'El contenido es de solo lectura. Revisa el complemento del espacio de trabajo para restaurar la edición.';

  @override
  String get mainRecentThoughts => 'Ideas recientes';

  @override
  String get mainRecordedCount => 'Con observaciones';

  @override
  String get mainRestoreDefault => 'Restablecer';

  @override
  String get mainRetry => 'Reintentar';

  @override
  String get mainRetrySave => 'Reintentar guardado';

  @override
  String get mainSage => 'Salvia';

  @override
  String get mainSampleBody0 =>
      'Guarda aquí las ideas espontáneas.\nSin prisa por terminar: deja que empiecen.';

  @override
  String get mainSampleBody1 =>
      'Una pequeña página para tus palabras favoritas,\nmúsica y detalles cotidianos.';

  @override
  String get mainSampleBody2 =>
      'Prueba el arte generativo. Deja que el código\ncree formas inesperadas.';

  @override
  String get mainSampleBody3 =>
      'Un compañero tranquilo para recordar\nlas pequeñas cosas que se nos escapan.';

  @override
  String get mainSampleTitle0 => 'Un hogar para las ideas';

  @override
  String get mainSampleTitle1 => 'Un tranquilo jardín digital';

  @override
  String get mainSampleTitle2 => 'Crear algo por diversión';

  @override
  String get mainSampleTitle3 => 'Mi pequeño ayudante';

  @override
  String get mainSampleTodo0 => 'Organizar la primera colección';

  @override
  String get mainSampleTodo1 => 'Diseñar la entrada del jardín';

  @override
  String get mainSampleTodo2 => 'Plantar una nueva idea';

  @override
  String get mainSampleTodo3 => 'Esbozar un pequeño prototipo';

  @override
  String get mainSampleTodo4 => 'Diseñar los recordatorios';

  @override
  String get mainSaveConnectionUnknown =>
      'La conexión se interrumpió; el resultado del guardado es desconocido. Reabre la biblioteca y verifica antes de reintentar.';

  @override
  String get mainSaveFailed =>
      'No se pudo guardar. Los cambios se conservan en esta sesión.';

  @override
  String get mainSaveIdea => 'Guardar idea';

  @override
  String get mainSaveNotSubmitted =>
      'No se ha enviado. El borrador y los adjuntos se conservan; puedes editar y guardar de nuevo.';

  @override
  String get mainSaveReadOnly =>
      'Los cambios no se guardaron. Activa el complemento del espacio de trabajo en Complementos y servicios y reintenta.';

  @override
  String get mainSaveUnknown =>
      'Guardado sin confirmar. El borrador y los adjuntos se conservan. Reintenta este envío; al cerrar se actualizará el espacio de trabajo para comprobarlo.';

  @override
  String get mainSaving => 'Guardando…';

  @override
  String get mainSearchHint => 'Buscar ideas…';

  @override
  String get mainServiceRunSettings => 'Ejecución de servicios';

  @override
  String get mainServiceSettings => 'Servicios API';

  @override
  String get mainSettings => 'Ajustes';

  @override
  String get mainShowAppearance => 'Mostrar ajustes de apariencia';

  @override
  String get mainSidebarMotto => 'Un poco de orden. Espacio para descubrir.';

  @override
  String get mainSlowProgress => 'Los pequeños pasos también te hacen avanzar.';

  @override
  String get mainSolidCanvas => 'Liso';

  @override
  String get mainSolidDetail => 'Un fondo liso y tranquilo.';

  @override
  String get mainSolidOpacity => '100 % · Opaco';

  @override
  String get mainSortFavorites => 'Favoritos primero';

  @override
  String get mainSortRecent => 'Añadidos recientemente';

  @override
  String get mainSortTitle => 'Por título';

  @override
  String get mainSquareCorners => '0 para esquinas rectas';

  @override
  String get mainStageActive => 'En curso';

  @override
  String get mainStageCompleted => 'Completado';

  @override
  String get mainStageOrganized => 'Organizado';

  @override
  String get mainStagePlanned => 'Planificado';

  @override
  String get mainStageRecorded => 'Registrado';

  @override
  String mainStageTooltip(String title) {
    return 'Cambiar etapa de $title';
  }

  @override
  String get mainStageUnsorted => 'Por organizar';

  @override
  String get mainStageUnverified => 'Por probar';

  @override
  String get mainStageVerifying => 'En pruebas';

  @override
  String get mainStayCurious => 'CONSERVA LA CURIOSIDAD. SÉ TÚ.';

  @override
  String mainSteps(int done, int total) {
    return '$done/$total pasos';
  }

  @override
  String get mainStorageUnavailable =>
      'El almacenamiento local no está disponible. Los cambios durarán solo esta sesión.';

  @override
  String get mainStorageUnreadable =>
      'No se pudo leer el contenido guardado. Los datos originales se conservan y no se sobrescribirán.';

  @override
  String get mainTenMinutesAgo => 'Hace 10 minutos';

  @override
  String get mainTextureCanvas => 'Textura';

  @override
  String get mainTextureDetail =>
      'Una fina textura de papel añade una sensación táctil.';

  @override
  String get mainThemeCompass => 'Rueda de color del tema';

  @override
  String get mainThemeGrayscale => 'Escala de grises del tema';

  @override
  String get mainThemeTone => 'Colores del tema';

  @override
  String get mainThreeHoursAgo => 'Hace 3 horas';

  @override
  String get mainTintOpacity => 'Opacidad del tinte';

  @override
  String get mainToProject => 'Mover a proyectos';

  @override
  String get mainTodosPrompt => 'Próximos pasos (uno por línea, opcional)';

  @override
  String get mainTransparencyUnavailable =>
      'No se pudo activar la transparencia del sistema. Puedes usar el fondo predeterminado.';

  @override
  String get mainTransparentCanvas => 'Transparente';

  @override
  String get mainTransparentDetail =>
      'Muestra lo que hay detrás de la ventana; en la web, el fondo de la página.';

  @override
  String get mainUndo => 'Deshacer';

  @override
  String mainUnfavoriteTooltip(String title) {
    return 'Quitar $title de favoritos';
  }

  @override
  String get mainUnsortedCount => 'Por organizar';

  @override
  String get mainUnverifiedCount => 'Por probar';

  @override
  String get mainView => 'Ver';

  @override
  String get mainViewAll => 'Mostrar todo';

  @override
  String get mainWarmSand => 'Arena cálida';

  @override
  String get mainWhiteTheme => 'Claro';

  @override
  String get mainWindowRadius => 'Esquinas de la ventana';

  @override
  String get mainWindowRadiusDetail =>
      'Ajuste independiente del borde; al maximizar, las esquinas son rectas';

  @override
  String get mainWindowsFrostOnly =>
      'El desenfoque del escritorio solo está disponible en Windows';

  @override
  String get mainWorkbench => 'Espacio de trabajo';

  @override
  String get mainWorkbenchPlugin => 'Complemento del espacio de trabajo';

  @override
  String get mainWriteHypothesis =>
      'Abre el registro y anota lo que quieres probar.';

  @override
  String get mainYesterday => 'Ayer';

  @override
  String get pluginsApprovalUnknown =>
      'No se pudo confirmar la activación o los cambios de permisos';

  @override
  String get pluginsApproveEnable => 'Autorizar y activar';

  @override
  String get pluginsApproveWorkbench => 'Permitir lectura y edición y activar';

  @override
  String get pluginsAttachment => 'Leer adjuntos';

  @override
  String get pluginsBackingUp => 'Respaldando…';

  @override
  String get pluginsBackupLibrary => 'Respaldar biblioteca';

  @override
  String get pluginsBackupLibraryType => 'Copia de la biblioteca';

  @override
  String get pluginsBackupProtection => 'Respaldar archivo de protección';

  @override
  String get pluginsBackupUnknown =>
      'La copia aún no está confirmada. Conserve los archivos creados y compruebe el destino.';

  @override
  String pluginsBinaryPreview(String hex) {
    return 'Contenido binario: $hex';
  }

  @override
  String get pluginsBuiltin => 'Espacio integrado';

  @override
  String pluginsBuiltinCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count caracteres',
      one: '$count carácter',
    );
    return '$_temp0 · Solo esta sesión; no guardado como tarjeta';
  }

  @override
  String get pluginsBuiltinEmpty => 'Introduzca texto para verlo en mayúsculas';

  @override
  String get pluginsBuiltinHeading => 'Herramientas de texto';

  @override
  String get pluginsBuiltinInput => 'Introduzca texto';

  @override
  String get pluginsCancel => 'Cancelar';

  @override
  String get pluginsChoosePackage => 'Elegir archivo de complemento';

  @override
  String get pluginsChooseSmallFile => 'Elegir archivo pequeño';

  @override
  String get pluginsCloseTextTool => 'Ocultar herramienta de texto';

  @override
  String get pluginsCloseUnknown =>
      'No se pudo confirmar el cierre de la vista';

  @override
  String get pluginsCloseView => 'Cerrar vista';

  @override
  String get pluginsConnectionLost =>
      'Conexión interrumpida. Vuelva a abrir la vista del complemento.';

  @override
  String get pluginsContentPermissions => 'Permisos de contenido';

  @override
  String get pluginsCreate => 'Crear contenido';

  @override
  String get pluginsCredentialCancel => 'Cerrar formulario';

  @override
  String get pluginsCredentialCreateTitle => 'Nueva credencial';

  @override
  String pluginsCredentialDays(int days) {
    String _temp0 = intl.Intl.pluralLogic(
      days,
      locale: localeName,
      other: '$days días',
      one: '$days día',
    );
    return '$_temp0';
  }

  @override
  String get pluginsCredentialDetails =>
      'Guarde de forma segura credenciales para conexiones API autorizadas. Guardarlas no autoriza servidores ni activa complementos. Los secretos guardados no se pueden consultar.';

  @override
  String get pluginsCredentialDisable => 'Desactivar';

  @override
  String get pluginsCredentialDisabled => 'Desactivada';

  @override
  String get pluginsCredentialDisabledDone => 'Credencial desactivada.';

  @override
  String get pluginsCredentialEmpty => 'No hay credenciales guardadas';

  @override
  String get pluginsCredentialExpired => 'Caducada';

  @override
  String pluginsCredentialExpires(String date) {
    return 'Caduca: $date';
  }

  @override
  String get pluginsCredentialHeader => 'Nombre de cabecera';

  @override
  String get pluginsCredentialInvalid =>
      'Revise el nombre de cabecera e introduzca un nuevo secreto. El campo secreto se ha vaciado.';

  @override
  String get pluginsCredentialLifetime => 'Validez';

  @override
  String get pluginsCredentialLoadFailed =>
      'No se pudieron leer las credenciales de forma coherente. Actualice el estado para reintentar.';

  @override
  String get pluginsCredentialNew => 'Añadir credencial';

  @override
  String get pluginsCredentialReading => 'Leyendo credenciales…';

  @override
  String pluginsCredentialReference(String reference) {
    return 'Credencial $reference';
  }

  @override
  String get pluginsCredentialRefresh => 'Actualizar estado';

  @override
  String get pluginsCredentialReplace => 'Reemplazar secreto';

  @override
  String pluginsCredentialReplaceTitle(String reference) {
    return 'Reemplazar credencial $reference';
  }

  @override
  String get pluginsCredentialSave => 'Guardar credencial';

  @override
  String get pluginsCredentialSaved =>
      'Credencial guardada. Las conexiones API aún requieren autorización aparte.';

  @override
  String get pluginsCredentialSecret => 'Nuevo valor secreto';

  @override
  String get pluginsCredentialStored => 'Guardada';

  @override
  String get pluginsCredentialTitle => 'Credenciales de API';

  @override
  String get pluginsCredentialUnknown =>
      'No se pudo confirmar el resultado. El campo secreto se ha vaciado. Actualice el estado antes de hacer otro cambio.';

  @override
  String pluginsDeclared(String permissions) {
    return 'Permisos declarados: $permissions';
  }

  @override
  String get pluginsDependenciesNotice =>
      'Las dependencias deben configurarse en el host. Esta página no las autoriza.';

  @override
  String get pluginsDisable => 'Desactivar';

  @override
  String get pluginsDisableWorkbench => 'Desactivar complemento del espacio';

  @override
  String get pluginsDisabled => 'Desactivado';

  @override
  String get pluginsDisabledDetails =>
      'Desactivado. Permita leer y editar contenido para usar el editor y las herramientas.';

  @override
  String get pluginsEdit => 'Editar contenido';

  @override
  String get pluginsEmptyLibrary =>
      'Aún no se han importado complementos de terceros.';

  @override
  String get pluginsEmptyResult => '(resultado vacío)';

  @override
  String get pluginsEnabled => 'Activado';

  @override
  String get pluginsEnabledDetails =>
      'Activado. Este complemento puede leer y editar contenido. Desactivarlo conserva sus datos.';

  @override
  String get pluginsEndpointAdvanced =>
      'Límites de política (bytes salvo indicación)';

  @override
  String get pluginsEndpointCertificate => 'Elegir raíz DER';

  @override
  String get pluginsEndpointCertificateDetails =>
      'Raíz de confianza HTTPS opcional: un certificado DER binario (.der o .cer), hasta 32 KiB. No se aceptan PEM ni conjuntos de certificados. Quite la raíz antes de cambiar a HTTP.';

  @override
  String get pluginsEndpointCertificateInvalid =>
      'Elija un certificado DER binario válido (.der o .cer) de hasta 32 KiB.';

  @override
  String pluginsEndpointCertificateSelected(int bytes) {
    return 'Raíz DER seleccionada ($bytes bytes)';
  }

  @override
  String get pluginsEndpointConcurrency => 'Solicitudes simultáneas (1–128)';

  @override
  String get pluginsEndpointCreateTitle =>
      'Nueva autorización de punto de conexión';

  @override
  String get pluginsEndpointCredential => 'Referencia de credencial';

  @override
  String get pluginsEndpointCredentialLifetime =>
      'La credencial debe ser válida durante toda la vigencia del punto de conexión. No se ampliará su caducidad.';

  @override
  String get pluginsEndpointCredentialUnavailable =>
      'Las credenciales requieren que el paquete declare y tenga aprobado su uso, y una referencia guardada válida.';

  @override
  String get pluginsEndpointCredentialsFailed =>
      'No se pudieron leer las referencias de credenciales. Actualice el estado antes de elegir.';

  @override
  String get pluginsEndpointDetails =>
      'Guarde una política de servidor para un paquete y hash específicos. Guardarla no conecta a la red, no activa el complemento ni habilita tareas de red de inmediato.';

  @override
  String get pluginsEndpointDigest => 'Hash del paquete';

  @override
  String get pluginsEndpointDisabledDone =>
      'Autorización del punto de conexión desactivada.';

  @override
  String get pluginsEndpointEmpty =>
      'No hay autorizaciones de puntos de conexión guardadas';

  @override
  String get pluginsEndpointFrameBytes =>
      'Presupuesto de trama (1–131072 bytes)';

  @override
  String get pluginsEndpointHeaderBytes =>
      'Máximo de bytes de cabeceras (1–16384)';

  @override
  String get pluginsEndpointInvalid =>
      'Revise el paquete, origen, métodos, validez de 1–30 días, permiso de credenciales, certificado y límites.';

  @override
  String get pluginsEndpointLifetime => 'Validez (1–30 días)';

  @override
  String get pluginsEndpointLoadFailed =>
      'No se pudieron leer las autorizaciones de forma coherente. Actualice el estado para reintentar.';

  @override
  String get pluginsEndpointLocalHttp => 'HTTP local';

  @override
  String get pluginsEndpointLocalHttps => 'HTTPS local';

  @override
  String get pluginsEndpointMethods => 'Métodos de solicitud permitidos';

  @override
  String get pluginsEndpointNew => 'Añadir punto de conexión';

  @override
  String get pluginsEndpointNoCredential => 'Sin credencial';

  @override
  String get pluginsEndpointOrigin =>
      'Solo origen, por ejemplo https://api.example.com';

  @override
  String get pluginsEndpointPackage => 'Paquete';

  @override
  String get pluginsEndpointPackageUnavailable =>
      'El paquete no está disponible o no tiene permiso HTTP aprobado. Aún se pueden desactivar las autorizaciones existentes.';

  @override
  String get pluginsEndpointProfile => 'Perfil de conexión';

  @override
  String get pluginsEndpointPublicHttps => 'HTTPS público';

  @override
  String get pluginsEndpointRemoveCertificate => 'Quitar raíz de confianza';

  @override
  String get pluginsEndpointReplace => 'Reemplazar autorización';

  @override
  String get pluginsEndpointReplaceTitle =>
      'Reemplazar autorización con el hash actual del paquete';

  @override
  String get pluginsEndpointRequestBytes =>
      'Máximo de bytes de solicitud (1–65536)';

  @override
  String get pluginsEndpointResponseBytes =>
      'Máximo de bytes de respuesta (1–65536)';

  @override
  String get pluginsEndpointSave =>
      'Guardar autorización del punto de conexión';

  @override
  String get pluginsEndpointSaved =>
      'Autorización guardada. No se estableció ninguna conexión de red.';

  @override
  String get pluginsEndpointTimeout =>
      'Tiempo de espera (1–30000 milisegundos)';

  @override
  String get pluginsEndpointTitle => 'Autorizaciones de puntos de conexión API';

  @override
  String get pluginsEndpointUnknown =>
      'No se pudo confirmar el resultado. Actualice el estado antes de otro cambio. La solicitud no se reenviará automáticamente.';

  @override
  String get pluginsEndpointWorking =>
      'Actualizando estado del punto de conexión…';

  @override
  String get pluginsExistingVersion =>
      'Esta versión ya está instalada. Su estado de activación no ha cambiado.';

  @override
  String pluginsFileLimit(int limit) {
    return 'El archivo es demasiado grande. Elija uno de como máximo $limit bytes.';
  }

  @override
  String get pluginsHttpTaskAbandon => 'Terminar observación de este intento';

  @override
  String get pluginsHttpTaskAbandonDetails =>
      'Solo puede terminar la observación si un estado reciente confirma que no hay tarea activa y la biblioteca original está disponible. Esto no prueba ausencia de efectos remotos. La identidad y la incertidumbre permanecen en el historial; otra solicitud exige un envío explícito.';

  @override
  String get pluginsHttpTaskAbsent => 'Sin entrega de resultado';

  @override
  String get pluginsHttpTaskAccepted => 'Aceptado';

  @override
  String get pluginsHttpTaskAcknowledge => 'Confirmar tarea finalizada';

  @override
  String get pluginsHttpTaskArchivedUnknown =>
      'Observación terminada por el usuario. Los efectos remotos anteriores siguen sin confirmar; este intento no se repitió.';

  @override
  String get pluginsHttpTaskBase64 => 'Base64';

  @override
  String get pluginsHttpTaskBody => 'Cuerpo de solicitud';

  @override
  String get pluginsHttpTaskBodyFormat =>
      'Codificación del cuerpo de solicitud';

  @override
  String get pluginsHttpTaskBusy => 'Ocupado';

  @override
  String get pluginsHttpTaskCancel => 'Solicitar cancelación';

  @override
  String get pluginsHttpTaskCancelled =>
      'Cancelación detectada; puede haber efectos remotos';

  @override
  String get pluginsHttpTaskCatalogUnavailable =>
      'El catálogo no está disponible. Actualice la biblioteca de complementos antes de un nuevo envío; los controles de tareas existentes siguen disponibles.';

  @override
  String get pluginsHttpTaskClosed => 'Cerrado';

  @override
  String get pluginsHttpTaskCompleted => 'Completado';

  @override
  String get pluginsHttpTaskConflict => 'Conflicto';

  @override
  String get pluginsHttpTaskConsumed => 'Resultado consumido';

  @override
  String get pluginsHttpTaskControlUnknown =>
      'No se pudo confirmar el resultado del control. Actualice el estado antes de decidir la siguiente acción.';

  @override
  String pluginsHttpTaskCounters(String bytes, String calls) {
    return 'Llamadas E/S: $calls; bytes contabilizados: $bytes';
  }

  @override
  String get pluginsHttpTaskDeadline => 'Plazo superado';

  @override
  String get pluginsHttpTaskDenied => 'Denegado';

  @override
  String get pluginsHttpTaskDetails =>
      'Ejecute una solicitud explícita con un punto autorizado y un complemento activo con el manejador experimental de reenvío HTTP. El estado sigue disponible mientras la biblioteca está ocupada.';

  @override
  String get pluginsHttpTaskDisconnect => 'Error al limpiar la conexión';

  @override
  String get pluginsHttpTaskEndpoint => 'Punto de conexión autorizado';

  @override
  String get pluginsHttpTaskEndpointsFailed =>
      'No se pudieron leer los puntos de conexión de forma coherente, o la biblioteca está ocupada. Los controles siguen disponibles. Actualice los puntos cuando vuelva la biblioteca.';

  @override
  String get pluginsHttpTaskEvidenceUnavailable =>
      'Evidencia del resultado no disponible';

  @override
  String pluginsHttpTaskExecution(int code, String fault) {
    return 'Ejecución del invitado: $fault; código de salida: $code';
  }

  @override
  String pluginsHttpTaskExit(
    String disconnect,
    String execution,
    String maintenance,
  ) {
    return 'Salida del worker — ejecución: $execution; desconexión: $disconnect; mantenimiento: $maintenance';
  }

  @override
  String get pluginsHttpTaskExplicit =>
      'Enviar realiza una solicitud real. Cada clic crea una identidad nueva. Los envíos y lecturas inciertos nunca se repiten automáticamente. Cancelar no prueba que se haya deshecho la operación remota.';

  @override
  String get pluginsHttpTaskFailed => 'Fallido';

  @override
  String get pluginsHttpTaskHeaders =>
      'Cabeceras ordinarias, un Nombre: valor por línea';

  @override
  String get pluginsHttpTaskHeadersHint =>
      'Las cabeceras repetidas se mantienen separadas. Solo el entorno de ejecución aporta las de credenciales y conexión.';

  @override
  String get pluginsHttpTaskHistory => 'Observaciones anteriores (hasta 5)';

  @override
  String pluginsHttpTaskHttpResult(int code, String status) {
    return 'Resultado HTTP: $status; estado remoto: $code';
  }

  @override
  String get pluginsHttpTaskInactive => 'Conexión inactiva';

  @override
  String get pluginsHttpTaskInvalid =>
      'Revise el punto, método, destino relativo, cabeceras ordinarias, codificación del cuerpo y tiempo de espera según los límites aprobados.';

  @override
  String get pluginsHttpTaskInvalidOptions => 'Opciones no válidas';

  @override
  String pluginsHttpTaskKey(String identity) {
    return 'Identidad de tarea: $identity';
  }

  @override
  String get pluginsHttpTaskLimit => 'Cuota o límite alcanzado';

  @override
  String get pluginsHttpTaskLoadingEndpoints => 'Leyendo puntos autorizados…';

  @override
  String get pluginsHttpTaskLocal => 'Biblioteca disponible; sin tarea activa';

  @override
  String get pluginsHttpTaskModule => 'Módulo invitado no válido';

  @override
  String get pluginsHttpTaskNew => 'Preparar nueva solicitud';

  @override
  String get pluginsHttpTaskNoEndpoints =>
      'Ningún punto coincide con un complemento de reenvío HTTP activo y aprobado.';

  @override
  String get pluginsHttpTaskNotFound => 'No encontrado';

  @override
  String get pluginsHttpTaskOk => 'Correcto';

  @override
  String get pluginsHttpTaskOutcomeUnknown =>
      'Resultado remoto desconocido; no suponga reversión ni reenvíe';

  @override
  String get pluginsHttpTaskPackageChanged =>
      'Vinculación del paquete cambiada';

  @override
  String get pluginsHttpTaskPending => 'Resultado pendiente';

  @override
  String get pluginsHttpTaskPoll => 'Consultar tarea';

  @override
  String get pluginsHttpTaskProtocol => 'Error de protocolo de tarea';

  @override
  String get pluginsHttpTaskRead => 'Leer resultado una vez';

  @override
  String get pluginsHttpTaskReadBound => 'Límite de lectura superado';

  @override
  String get pluginsHttpTaskReadPending =>
      'No se devolvió ningún resultado. Revise el estado antes de volver a leer explícitamente.';

  @override
  String get pluginsHttpTaskReadUnknown =>
      'No se confirmó la lectura y puede que ya haya consumido el resultado. No se volverá a leer. Aún se pueden consultar el estado y la salida.';

  @override
  String get pluginsHttpTaskReady =>
      'Resultado listo: léalo explícitamente. Esto no significa que el worker haya finalizado.';

  @override
  String get pluginsHttpTaskReclaimed =>
      'Worker finalizado; biblioteca original devuelta';

  @override
  String get pluginsHttpTaskRecoveryRequired =>
      'Worker finalizado; limpieza o mantenimiento requieren reparación';

  @override
  String get pluginsHttpTaskRefresh => 'Actualizar estado de tarea';

  @override
  String get pluginsHttpTaskRefreshEndpoints => 'Actualizar puntos autorizados';

  @override
  String get pluginsHttpTaskRemoteError =>
      'El servidor remoto devolvió 4xx/5xx. El intercambio HTTP se completó; es distinto de los errores de ejecución del invitado.';

  @override
  String get pluginsHttpTaskRepair => 'Reparar limpieza';

  @override
  String get pluginsHttpTaskResponseBase64 =>
      'Cuerpo de respuesta: Base64 exacto';

  @override
  String get pluginsHttpTaskResponseHeaders =>
      'Cabeceras de respuesta (duplicados conservados; valores binarios en Base64)';

  @override
  String get pluginsHttpTaskResponseText =>
      'Cuerpo de respuesta: vista previa de texto';

  @override
  String get pluginsHttpTaskResultUnavailable =>
      'Entrega del resultado no disponible';

  @override
  String get pluginsHttpTaskRevoked => 'Autorización revocada';

  @override
  String get pluginsHttpTaskRunning =>
      'En ejecución; el worker tiene la biblioteca';

  @override
  String get pluginsHttpTaskSpawn => 'No se pudo iniciar el worker';

  @override
  String get pluginsHttpTaskStart => 'Enviar nueva solicitud';

  @override
  String get pluginsHttpTaskStartUnknown =>
      'El resultado del envío es desconocido. Se conserva su identidad. Consulte el estado para encontrar la misma tarea; la solicitud no se reenviará.';

  @override
  String get pluginsHttpTaskStatusFailed =>
      'No se pudo confirmar el estado. Actualícelo; no se ha repetido ninguna solicitud.';

  @override
  String get pluginsHttpTaskStopping =>
      'Deteniéndose; esperando salida real del worker';

  @override
  String pluginsHttpTaskSubmission(String identity) {
    return 'Identidad del envío: $identity';
  }

  @override
  String get pluginsHttpTaskTarget =>
      'Destino relativo, por ejemplo /v1/items?limit=10';

  @override
  String get pluginsHttpTaskText => 'Texto UTF-8';

  @override
  String get pluginsHttpTaskTimeout =>
      'Tiempo de espera en ms (1–30000, dentro de la autorización)';

  @override
  String get pluginsHttpTaskTitle => 'Tareas HTTP';

  @override
  String get pluginsHttpTaskTrap => 'Trap en la ejecución del invitado';

  @override
  String get pluginsHttpTaskUnavailable =>
      'Biblioteca original no disponible; requiere recuperación';

  @override
  String get pluginsHttpTaskUnsupported => 'Operación no compatible';

  @override
  String get pluginsHttpTaskWorking =>
      'Esperando respuesta de control de tarea…';

  @override
  String get pluginsImport => 'Importar';

  @override
  String get pluginsImportDetails =>
      'Tras importar, usted decide si activa el complemento. Desactivarlo o desinstalarlo conserva el contenido.';

  @override
  String pluginsImportPreview(String name) {
    return 'Vista previa de importación: $name';
  }

  @override
  String get pluginsImportUnknown => 'No se pudo confirmar la importación';

  @override
  String get pluginsImportedDisabled =>
      'Importado y desactivado. Elija qué permisos conceder.';

  @override
  String get pluginsInputFailed => 'No se pudo leer el archivo de entrada';

  @override
  String get pluginsInputTooLong =>
      'Se alcanzó el límite de entrada. Acorte el texto e inténtelo de nuevo.';

  @override
  String get pluginsInspectFailed =>
      'No se pudo cargar la vista previa del complemento';

  @override
  String get pluginsInspectedOnly =>
      'Solo se ha inspeccionado el archivo. Active el complemento por separado tras importarlo.';

  @override
  String get pluginsInsufficientApproval =>
      'El complemento está activado, pero necesita permiso de contenido. El espacio sigue en modo de solo lectura. Desactívelo para revisar los permisos.';

  @override
  String pluginsIoApproved(String permissions) {
    return 'Aprobado: $permissions';
  }

  @override
  String get pluginsIoCredentialUse => 'Usar credenciales autorizadas';

  @override
  String pluginsIoDeclared(String permissions) {
    return 'Permisos de red y archivos solicitados: $permissions';
  }

  @override
  String get pluginsIoFileCreate => 'Crear archivos';

  @override
  String get pluginsIoFileDelete => 'Eliminar archivos';

  @override
  String get pluginsIoFileList => 'Explorar carpetas autorizadas';

  @override
  String get pluginsIoFileRead => 'Leer archivos autorizados';

  @override
  String get pluginsIoFileReplace => 'Reemplazar archivos';

  @override
  String get pluginsIoHttpListen => 'Escuchar conexiones de red';

  @override
  String get pluginsIoHttpPublish => 'Ofrecer un servicio API';

  @override
  String get pluginsIoHttpRequest => 'Invocar API de red';

  @override
  String get pluginsIoNoneApproved =>
      'Sin permisos de red ni archivos aprobados';

  @override
  String get pluginsIoRevoke => 'Revocar todos los permisos de red y archivos';

  @override
  String get pluginsIoSave => 'Guardar permisos de red y archivos';

  @override
  String get pluginsIoScopeNotice =>
      'Estas opciones solo guardan categorías de permisos. Las direcciones, el acceso a archivos y las credenciales requieren aprobación aparte; las funciones no disponibles siguen sin estarlo. Reabra el formulario tras los cambios.';

  @override
  String get pluginsIoTitle => 'Permisos de red y archivos';

  @override
  String get pluginsIoWebSocketConnect => 'Conectar a servicios WebSocket';

  @override
  String get pluginsListUnknown =>
      'No se pudo confirmar la lista de complementos';

  @override
  String get pluginsManageAbove =>
      'Gestione este complemento con los controles del espacio de trabajo de arriba.';

  @override
  String get pluginsManagementUnavailable =>
      'La gestión de complementos no está disponible. El contenido existente sigue siendo legible.';

  @override
  String get pluginsNoPermissions =>
      'No se han declarado permisos de contenido.';

  @override
  String get pluginsOpenTextTool => 'Abrir herramienta de texto';

  @override
  String get pluginsOpenView => 'Abrir vista';

  @override
  String get pluginsOpeningView => 'Abriendo vista del complemento…';

  @override
  String get pluginsOperation => 'Consultar resultados de operaciones';

  @override
  String pluginsOtherCapability(String name) {
    return 'Otro permiso declarado: $name';
  }

  @override
  String get pluginsPackageFile => 'Complemento de Morrow';

  @override
  String get pluginsPreviewOnly =>
      'Solo vista previa. Los resultados no se guardan automáticamente en el contenido existente.';

  @override
  String get pluginsPreviewTruncated =>
      '…solo se muestran los primeros 4096 caracteres';

  @override
  String get pluginsProtection => 'Protección del contenido';

  @override
  String get pluginsProtectionDetails =>
      'Respalde el archivo de protección original para recuperar con esta cuenta del sistema. No contiene tarjetas ni adjuntos.';

  @override
  String get pluginsProtectionFileType => 'Archivo de protección de biblioteca';

  @override
  String get pluginsProtectionSaved =>
      'Archivo de protección respaldado. Elíjalo para recuperar si falla el inicio.';

  @override
  String get pluginsRead => 'Leer contenido';

  @override
  String get pluginsReadingState => 'Cargando estado del complemento…';

  @override
  String pluginsRefreshFailed(String reason) {
    return '$reason. No se pudo actualizar la lista. Seleccione «Actualizar lista» para volver a leerla.';
  }

  @override
  String get pluginsRefreshList => 'Actualizar lista';

  @override
  String get pluginsRefreshState => 'Actualizar estado';

  @override
  String get pluginsRename => 'Renombrar';

  @override
  String pluginsResultBytes(int count, String preview) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count bytes',
      one: '$count byte',
    );
    return '$_temp0\n$preview';
  }

  @override
  String get pluginsSavePermissions => 'Guardar permisos';

  @override
  String pluginsSelectedFile(String name) {
    return 'Archivo seleccionado: $name';
  }

  @override
  String get pluginsServiceAcknowledgeUncertain =>
      'He revisado los registros actualizados';

  @override
  String get pluginsServiceAddScope => 'Añadir ámbito de contenido';

  @override
  String get pluginsServiceAttachmentId => 'Identidad exacta de adjunto';

  @override
  String get pluginsServiceAuthenticationUnavailable =>
      'La autenticación falta, está desactivada, caducada o pertenece a otra identidad. Sus ámbitos provisionales se conservan; elija un reemplazo válido o quítela explícitamente.';

  @override
  String get pluginsServiceAuthorities =>
      'Registros de autenticación y publicación';

  @override
  String get pluginsServiceCardId => 'Identidad exacta de tarjeta';

  @override
  String get pluginsServiceCatalogChanged =>
      'El catálogo cambió o no está disponible. El borrador se conserva. Actualice explícitamente la selección antes de guardar.';

  @override
  String get pluginsServiceClearToken => 'Borrar token';

  @override
  String get pluginsServiceCloseEditor => 'Cerrar editor';

  @override
  String get pluginsServiceConfigDigest => 'Hash de configuración';

  @override
  String get pluginsServiceConfiguration => 'Configuración guardada';

  @override
  String get pluginsServiceConfigurations => 'Configuraciones guardadas';

  @override
  String get pluginsServiceCopyClear => 'Copiar y borrar token';

  @override
  String get pluginsServiceCreated => 'Creación (UTC)';

  @override
  String get pluginsServiceDays => 'Duración solicitada (1–30 días)';

  @override
  String get pluginsServiceDigestFixed =>
      'La edición conserva el hash original. Debe seleccionar un paquete coincidente; esto no lo activa.';

  @override
  String get pluginsServiceDisable => 'Desactivar';

  @override
  String get pluginsServiceDisabled => 'Desactivado';

  @override
  String get pluginsServiceEditConfig => 'Editar configuración';

  @override
  String get pluginsServiceEditPublication => 'Editar publicación';

  @override
  String get pluginsServiceExpired => 'Caducado o aún no válido';

  @override
  String get pluginsServiceExpires => 'Caducidad real (UTC)';

  @override
  String get pluginsServiceHandler => 'Manejador de servicio declarado';

  @override
  String get pluginsServiceIdentity => 'Identidad del servicio';

  @override
  String get pluginsServiceInvalid =>
      'Revise campos, autorizaciones seleccionadas y paquete actual antes de guardar.';

  @override
  String get pluginsServiceIssue => 'Emitir token';

  @override
  String get pluginsServiceIssuedToken =>
      'Token Bearer de una sola visualización';

  @override
  String get pluginsServiceListenAddress =>
      'Dirección numérica y puerto de escucha';

  @override
  String get pluginsServiceLoadFailed =>
      'No se pudieron actualizar los registros. Actualice otra vez antes de hacer cambios.';

  @override
  String get pluginsServiceManagementOnly =>
      'Gestione configuraciones y autorizaciones guardadas. Guardar no inicia un listener, no ejecuta un paquete ni activa un servicio.';

  @override
  String get pluginsServiceMethod => 'Método HTTP';

  @override
  String get pluginsServiceNewAuthentication => 'Nueva autenticación';

  @override
  String get pluginsServiceNewConfig => 'Nueva configuración';

  @override
  String get pluginsServiceNo => 'No';

  @override
  String get pluginsServiceNoAuthentication =>
      'Primero cree un registro de autenticación vigente.';

  @override
  String get pluginsServiceNoAuthorities =>
      'Sin registros de autenticación ni publicación.';

  @override
  String get pluginsServiceNoConfigurations =>
      'Sin configuraciones de servicio.';

  @override
  String get pluginsServicePackage => 'Paquete declarado y aprobado';

  @override
  String get pluginsServicePackageDigest => 'Hash del paquete';

  @override
  String get pluginsServicePackageUnavailable =>
      'El paquete correspondiente o sus permisos de escucha/publicación no están disponibles. Los registros históricos pueden leerse y desactivarse.';

  @override
  String get pluginsServicePath => 'Ruta exacta de solicitud';

  @override
  String get pluginsServicePolicyChanged =>
      'El registro original cambió o ya no es utilizable. Actualice la selección o reabra el editor desde el registro actual. El borrador se conserva.';

  @override
  String get pluginsServicePrincipalId => 'Identificador de identidad';

  @override
  String get pluginsServicePrincipals =>
      'Identidades autorizadas y ámbitos de contenido';

  @override
  String get pluginsServicePublicationEditor => 'Autorización de publicación';

  @override
  String get pluginsServicePublicationHelp =>
      'La autorización se vincula a esta configuración, revisión y referencia exactas. La caducidad está limitada por todas las autenticaciones seleccionadas y puede ser menor de lo solicitado. Guardar no inicia la escucha.';

  @override
  String get pluginsServicePublicationMismatch =>
      'La publicación ya no coincide con la configuración actual. Revise y guarde explícitamente una autorización de reemplazo.';

  @override
  String get pluginsServiceQueryPath =>
      'Ruta separada de consulta del resultado (opcional)';

  @override
  String get pluginsServiceReference => 'Referencia de autorización';

  @override
  String get pluginsServiceRefresh => 'Actualizar registros';

  @override
  String get pluginsServiceRefreshSelection => 'Actualizar esta selección';

  @override
  String get pluginsServiceRemovePrincipal => 'Quitar identidad';

  @override
  String get pluginsServiceRemoveScope => 'Quitar ámbito';

  @override
  String get pluginsServiceRetention =>
      'Retención del historial (ms, hasta 30 días)';

  @override
  String get pluginsServiceRevision => 'Revisión';

  @override
  String get pluginsServiceRotate => 'Rotar token';

  @override
  String get pluginsServiceRotateAuthentication => 'Rotar autenticación';

  @override
  String get pluginsServiceRunAbandon =>
      'Conservar registro y terminar intento';

  @override
  String get pluginsServiceRunAdvanced => 'Límites de solicitudes y worker';

  @override
  String get pluginsServiceRunAttempt => 'Intento de inicio sin resolver';

  @override
  String get pluginsServiceRunBoundsHint =>
      'Los límites también deben cumplir la declaración del complemento y las autorizaciones guardadas. El trabajo reservado consume presupuesto acumulado incluso si se cancela. La caducidad detiene la ejecución; no hay renovación automática.';

  @override
  String get pluginsServiceRunBytes =>
      'Presupuesto de ejecución (bytes, hasta 67 108 864)';

  @override
  String get pluginsServiceRunCalls => 'Llamadas por tarea (hasta 1024)';

  @override
  String get pluginsServiceRunCancelled => 'Cancelado';

  @override
  String get pluginsServiceRunClosed => 'Cerrado';

  @override
  String get pluginsServiceRunConcurrent => 'Tareas simultáneas (hasta 128)';

  @override
  String get pluginsServiceRunControlUnknown =>
      'El resultado del control es desconocido. Actualice el estado del servicio original antes de otra operación.';

  @override
  String get pluginsServiceRunDenied => 'Denegado';

  @override
  String get pluginsServiceRunExited => 'Servicio finalizado';

  @override
  String get pluginsServiceRunHeaderBytes =>
      'Tamaño máximo de cabeceras (bytes, hasta 65 536)';

  @override
  String get pluginsServiceRunHint =>
      'Elija una publicación aprobada y límites finitos, luego inicie explícitamente el servicio. Tras detenerlo, espere a que vuelva el propietario original del espacio antes de confirmar el resultado.';

  @override
  String pluginsServiceRunHostFailure(String detail) {
    return 'Diagnóstico del host: $detail';
  }

  @override
  String get pluginsServiceRunHttpPanel =>
      'Un servicio API posee esta tarea. Use el panel superior para detenerlo o confirmar su salida. El borrador HTTP se conserva.';

  @override
  String get pluginsServiceRunIdentityChanged =>
      'Otra tarea posee el contenido. Este panel no la controlará con la identidad anterior del servicio.';

  @override
  String get pluginsServiceRunInvalid =>
      'Revise el servicio y los límites numéricos. No se envió una nueva ejecución.';

  @override
  String get pluginsServiceRunInvalidOutcome => 'Configuración no válida';

  @override
  String get pluginsServiceRunJobBytes => 'Bytes por tarea (hasta 16 777 216)';

  @override
  String get pluginsServiceRunJobs =>
      'Reservas totales de tareas (hasta 1 000 000)';

  @override
  String get pluginsServiceRunLastObservation =>
      'Se muestra la última observación; estado actual no verificado.';

  @override
  String get pluginsServiceRunLifetime => 'Duración (ms, hasta 3 600 000)';

  @override
  String get pluginsServiceRunLimit => 'Límite alcanzado';

  @override
  String get pluginsServiceRunLocal => 'Contenido disponible localmente';

  @override
  String pluginsServiceRunNetwork(
    String bind,
    String listener,
    String supervision,
  ) {
    return 'Enlace: $bind; listener: $listener; supervisión: $supervision';
  }

  @override
  String get pluginsServiceRunNextSettings =>
      'Ajustes de la próxima ejecución explícita';

  @override
  String get pluginsServiceRunNoSelection =>
      'No hay publicaciones aprobadas disponibles. Revise complemento, configuración y autenticación.';

  @override
  String get pluginsServiceRunOutboundAttempt =>
      'Puntos vinculados a este intento de inicio';

  @override
  String get pluginsServiceRunOutboundClear => 'Borrar selección de puntos';

  @override
  String get pluginsServiceRunOutboundFailed =>
      'No se pudo verificar la lista de puntos. Actualice antes de usar los seleccionados.';

  @override
  String get pluginsServiceRunOutboundHint =>
      'API salientes (opcional, hasta 8). Solo se muestran puntos aprobados para este paquete. Sin selección, se bloquean las llamadas salientes.';

  @override
  String get pluginsServiceRunOutboundStale =>
      'Un punto seleccionado cambió o ya no está disponible. Elija explícitamente su versión actual o borre la selección.';

  @override
  String get pluginsServiceRunOwned =>
      'El servicio activo gestiona el contenido';

  @override
  String get pluginsServiceRunPending => 'Pendiente';

  @override
  String get pluginsServiceRunReclaimed =>
      'Propiedad del contenido recuperada; requiere confirmación';

  @override
  String get pluginsServiceRunReclaiming =>
      'Esperando recuperar propiedad del contenido';

  @override
  String get pluginsServiceRunRecovery => 'La limpieza requiere reparación';

  @override
  String get pluginsServiceRunRequestBytes =>
      'Tamaño máximo de solicitud (bytes)';

  @override
  String get pluginsServiceRunResponseBytes =>
      'Tamaño máximo de respuesta (bytes)';

  @override
  String get pluginsServiceRunRunning => 'Servicio en ejecución';

  @override
  String get pluginsServiceRunSelection => 'Publicación de servicio aprobada';

  @override
  String get pluginsServiceRunStale =>
      'El paquete, configuración o autorización cambió. Actualice los registros y selecciónelo de nuevo antes de iniciar.';

  @override
  String get pluginsServiceRunStart => 'Iniciar servicio limitado';

  @override
  String get pluginsServiceRunStartRejected =>
      'La respuesta de inicio indicó un error. Se comprobó la tarea actual; revise la causa y el estado de limpieza antes de continuar.';

  @override
  String get pluginsServiceRunStartUnknown =>
      'El resultado del inicio es desconocido. Se conserva la identidad del intento; actualice para localizarlo. No se iniciará otra vez automáticamente.';

  @override
  String get pluginsServiceRunStarting => 'Iniciando servicio';

  @override
  String get pluginsServiceRunStatusFailed =>
      'No se pudo verificar el estado del servicio. Actualice antes de actuar.';

  @override
  String get pluginsServiceRunStop => 'Detener servicio';

  @override
  String get pluginsServiceRunStopping =>
      'Deteniéndose; esperando salida del listener y worker';

  @override
  String get pluginsServiceRunSucceeded => 'Correcto';

  @override
  String get pluginsServiceRunTask => 'Identidad de tarea actual';

  @override
  String get pluginsServiceRunTimeout =>
      'Tiempo límite por tarea (ms, hasta 30 000)';

  @override
  String get pluginsServiceRunTimeoutOutcome => 'Tiempo agotado';

  @override
  String get pluginsServiceRunTitle => 'Ejecutar servicio API';

  @override
  String get pluginsServiceRunTotalBytes =>
      'Presupuesto de bytes del worker (hasta 67 108 864)';

  @override
  String get pluginsServiceRunTransport => 'Error de transporte';

  @override
  String get pluginsServiceRunUnavailable =>
      'Almacenamiento de contenido no disponible';

  @override
  String get pluginsServiceSaveConfig => 'Guardar configuración';

  @override
  String get pluginsServiceSavePublication =>
      'Guardar autorización de publicación';

  @override
  String get pluginsServiceSaved =>
      'Guardado. Revise abajo la revisión devuelta y la caducidad real.';

  @override
  String get pluginsServiceScopeAttachment => 'Leer adjunto';

  @override
  String get pluginsServiceScopeCreate => 'Crear contenido';

  @override
  String get pluginsServiceScopeEdit => 'Editar contenido';

  @override
  String get pluginsServiceScopeKind => 'Operación de contenido permitida';

  @override
  String get pluginsServiceScopeQuery => 'Consultar operación';

  @override
  String get pluginsServiceScopeRead => 'Leer contenido';

  @override
  String get pluginsServiceScopeRename => 'Renombrar tarjeta';

  @override
  String get pluginsServiceScopeSummary => 'Leer resumen';

  @override
  String get pluginsServiceScopesHelp =>
      'Seleccione la autenticación explícitamente. Añada cada operación permitida y la identidad exacta del objeto. Para quitar un ámbito o identidad use su botón; los ámbitos existentes se conservan al editar.';

  @override
  String get pluginsServiceTitle => 'Configuración del servicio';

  @override
  String get pluginsServiceTls => 'Exigir TLS';

  @override
  String get pluginsServiceTlsAttempt =>
      'Hash del certificado PEM vinculado al intento';

  @override
  String get pluginsServiceTlsCertificate => 'Elegir cadena de certificados';

  @override
  String get pluginsServiceTlsChecked =>
      'Certificado y clave comprobados. El SHA-256 del PEM se muestra abajo. Los clientes aún deben verificar nombre del host, validez y cadena de confianza.';

  @override
  String get pluginsServiceTlsChecking =>
      'Procesando selección de certificado…';

  @override
  String get pluginsServiceTlsFailed =>
      'Falló la comprobación. Revise archivos PEM, correspondencia de clave y rutas locales antes de reintentar.';

  @override
  String get pluginsServiceTlsHelp =>
      'Las direcciones fuera de loopback requieren TLS. Solo se guarda el requisito; aquí no se crea un listener ni una identidad TLS.';

  @override
  String get pluginsServiceTlsHint =>
      'Seleccione una cadena PEM y clave privada, luego compruébelas. Se revisan otra vez al iniciar; el certificado activo no rota automáticamente.';

  @override
  String get pluginsServiceTlsInspect => 'Comprobar certificado';

  @override
  String get pluginsServiceTlsOutsideValidity =>
      'La cadena aún no es válida o ha caducado. Revise o reemplace el certificado y compruébelo otra vez antes de iniciar.';

  @override
  String get pluginsServiceTlsPrivateKey => 'Elegir clave privada';

  @override
  String get pluginsServiceTlsRecheck =>
      'Compruebe otra vez el certificado antes de iniciar. Cambiar el reloj no restaura la selección anterior.';

  @override
  String get pluginsServiceTlsUnavailable =>
      'Este backend no admite seleccionar certificados TLS locales.';

  @override
  String pluginsServiceTlsValidity(String end, String start) {
    return 'Validez común de la cadena (UTC): de $start a $end. El servicio se detiene al caducar.';
  }

  @override
  String get pluginsServiceTokenDiscarded =>
      'El token de una sola visualización se borró al cerrar el panel. Emita otro explícitamente si lo necesita.';

  @override
  String get pluginsServiceTokenHelp =>
      'Este token solo se muestra ahora. Cópielo explícitamente si lo necesita. Borrarlo o cerrar el panel lo elimina de la sesión; no puede recuperarse de la lista. La rotación reemplaza el token anterior.';

  @override
  String get pluginsServiceUncertainHelp =>
      'Primero actualice y revise los registros originales. Confirmar este aviso solo permite otra acción explícita; no prueba que el cambio anterior fallara ni lo repite.';

  @override
  String get pluginsServiceUnsupported => 'Valor histórico no compatible';

  @override
  String get pluginsServiceWorking => 'Procesando…';

  @override
  String get pluginsServiceWriteUnknown =>
      'El resultado del último cambio es desconocido. No se ha reenviado.';

  @override
  String get pluginsServiceYes => 'Sí';

  @override
  String get pluginsSettingsUnknown =>
      'El ajuste no está confirmado. Actualice el estado antes de volver a elegir.';

  @override
  String get pluginsSnapshotDetails =>
      'La copia incluye tarjetas, adjuntos y registros de auditoría. Los recursos externos siguen siendo referencias. La recuperación requiere la cuenta original del sistema.';

  @override
  String get pluginsSnapshotSaved =>
      'Biblioteca respaldada, incluidos adjuntos y archivo de protección original.';

  @override
  String get pluginsStateUnavailable =>
      'No se pudo cargar el estado del complemento. Inténtelo de nuevo.';

  @override
  String get pluginsSummary => 'Leer resúmenes';

  @override
  String get pluginsTextInput => 'Texto de entrada';

  @override
  String get pluginsThirdParty => 'Complementos de terceros';

  @override
  String get pluginsTlsIdentitiesDisable => 'Desactivar identidad';

  @override
  String get pluginsTlsIdentitiesEmpty =>
      'No hay identidades guardadas en esta biblioteca.';

  @override
  String get pluginsTlsIdentitiesFileMode =>
      'Próximo inicio: archivos locales comprobados.';

  @override
  String get pluginsTlsIdentitiesHint =>
      'Seleccione una identidad explícitamente. Reemplazarla o desactivarla detiene los servicios que la usan; todo nuevo inicio es explícito.';

  @override
  String get pluginsTlsIdentitiesImport =>
      'Preparar certificados para importar o reemplazar';

  @override
  String get pluginsTlsIdentitiesReplace =>
      'Reemplazar con archivos comprobados';

  @override
  String get pluginsTlsIdentitiesSave => 'Guardar como nueva identidad';

  @override
  String get pluginsTlsIdentitiesSaved =>
      'Guardado. Revise identidad y revisión abajo, luego selecciónela para un nuevo inicio.';

  @override
  String get pluginsTlsIdentitiesSavedMode =>
      'Próximo inicio: identidad guardada. El host verifica la validez al iniciar.';

  @override
  String get pluginsTlsIdentitiesSelect => 'Usar en el próximo inicio';

  @override
  String get pluginsTlsIdentitiesStale =>
      'La identidad cambió, se desactivó o no se ha actualizado. Seleccione otra vez una identidad actual.';

  @override
  String get pluginsTlsIdentitiesTitle => 'Identidades TLS guardadas';

  @override
  String get pluginsTlsIdentitiesUnknownHint =>
      'Actualice y revise los registros antes de confirmar. La falta de recibo no significa fallo; no vuelva a crear sin comprobar.';

  @override
  String get pluginsTlsIdentitiesUseFile =>
      'Usar archivos comprobados en el próximo inicio';

  @override
  String get pluginsTransform => 'Transformar';

  @override
  String get pluginsTransformUnknown =>
      'No se pudo confirmar la transformación';

  @override
  String get pluginsUiExecution =>
      'La ejecución del complemento no terminó. Vuelva a abrir la vista e inténtelo de nuevo.';

  @override
  String get pluginsUiRejected =>
      'La acción del complemento no fue aceptada. Revise los datos y los permisos actuales.';

  @override
  String get pluginsUiUnavailable =>
      'El complemento no está disponible. Revise su estado y vuelva a abrir la vista.';

  @override
  String get pluginsUnavailableView => 'Vista del complemento no disponible';

  @override
  String pluginsUnconfirmed(String reason) {
    return '$reason. La operación no está confirmada. Compruebe el estado actualizado antes de volver a elegir.';
  }

  @override
  String get pluginsUninstallKeepContent => 'Desinstalar (conservar contenido)';

  @override
  String get pluginsUninstallUnknown =>
      'No se pudo confirmar la desinstalación';

  @override
  String get pluginsUninstalled =>
      'Desinstalado. Se ha conservado el contenido existente.';

  @override
  String get pluginsUpdatingView => 'Actualizando vista previa…';

  @override
  String get pluginsUseText => 'Usar texto';

  @override
  String get pluginsUseTransform => 'Usar transformación';

  @override
  String get pluginsViewFailed => 'No se pudo abrir la vista del complemento';

  @override
  String get pluginsWorkbench => 'Complemento del espacio de trabajo';

  @override
  String get pluginsWorkbenchReadOnly =>
      'El complemento está autorizado, pero el espacio es de solo lectura. Resuelva el problema de la biblioteca o del complemento y actualice el estado.';

  @override
  String get recoveryAllFiles => 'Todos los archivos';

  @override
  String get recoveryBackupExists =>
      'Ya existe un archivo en el destino. Elige otro nombre.';

  @override
  String get recoveryBackupFile => 'Copia de biblioteca';

  @override
  String get recoveryBackupUnknown =>
      'Verifica el resultado de la copia. Conserva el archivo actual y revisa la ubicación.';

  @override
  String get recoveryBindingMissing =>
      'La biblioteca no tiene un archivo de protección vinculado. No se puede asociar el archivo elegido.';

  @override
  String get recoveryBusy =>
      'Otro proceso usa esta biblioteca. Cierra la otra ventana y reintenta.';

  @override
  String get recoveryChooseKey => 'Elegir archivo de recuperación';

  @override
  String get recoveryCloseFirst =>
      'El espacio de trabajo sigue abierto. Ciérralo antes de cambiar de biblioteca.';

  @override
  String get recoveryFailed =>
      'La recuperación no terminó. Conserva los archivos originales y reintenta.';

  @override
  String get recoveryIdentityBusy =>
      'Otra copia de esta biblioteca está abierta. Ciérrala antes de abrir esta.';

  @override
  String get recoveryIdentityMismatch =>
      'La identidad registrada no coincide. Conserva los datos originales y restaura la copia correcta.';

  @override
  String get recoveryKeyFile => 'Archivo de protección de biblioteca';

  @override
  String get recoveryKeyGuide =>
      'Si falta el archivo de protección o está dañado, elige su copia. Debe pertenecer a esta biblioteca y requiere la cuenta de sistema original.';

  @override
  String get recoveryKeyMismatch =>
      'La clave no coincide o no se puede descifrar. Usa el archivo y la cuenta de sistema originales.';

  @override
  String get recoveryKeyUnknown =>
      'Verifica el resultado de la recuperación. Intenta abrir de nuevo; se guardó una copia del archivo de protección anterior, si existía.';

  @override
  String get recoveryLibraryInvalid =>
      'No se pudo verificar o abrir la biblioteca. Conserva la biblioteca y la clave originales y reintenta.';

  @override
  String get recoveryMaintenance =>
      'La biblioteca requiere atención. Conserva los originales y revisa el diagnóstico.';

  @override
  String get recoveryMissingKey =>
      'Falta la clave de protección. Restaura el archivo .audit-key original y reintenta.';

  @override
  String get recoveryMissingLibrary =>
      'La clave existe, pero la biblioteca falta o está vacía. Restaura la biblioteca original.';

  @override
  String get recoveryOpenFailed =>
      'No se pudo abrir el espacio de trabajo. Revisa los complementos y la carpeta de datos y reintenta.';

  @override
  String get recoveryPluginUnavailable =>
      'Complemento del espacio de trabajo no disponible. El contenido existente se puede ver y exportar.';

  @override
  String get recoveryRegistryInvalid =>
      'El registro de biblioteca activa está dañado o no es compatible. Se detuvo la apertura para proteger los datos.';

  @override
  String get recoveryRegistryUnreadable =>
      'No se puede leer la biblioteca activa o su registro. Revisa la ubicación original; no se creará otra biblioteca automáticamente.';

  @override
  String get recoveryRetry => 'Reintentar';

  @override
  String get recoverySnapshot => 'Restaurar copia de biblioteca';

  @override
  String get recoverySnapshotGuide =>
      'Restaura la copia de la biblioteca en una carpeta nueva y cambia a ella. La carpeta original se conserva. Se recupera el estado de la copia y se requiere la cuenta de sistema original.';

  @override
  String get recoverySnapshotInvalid =>
      'Formato o integridad de la copia no válidos. Conserva el archivo original.';

  @override
  String get recoverySnapshotUnknown =>
      'Verifica el resultado en la carpeta de destino; la biblioteca original no se reemplazó.';

  @override
  String recoverySwitchUnconfirmed(String path) {
    return 'Copia restaurada en $path, pero el cambio no está confirmado. Conserva esa carpeta y reabre el espacio para comprobarlo.';
  }

  @override
  String get recoverySwitchUnknown =>
      'Cambio de biblioteca sin confirmar. Reabre el espacio de trabajo para verificar.';

  @override
  String get recoveryTargetExists =>
      'El destino ya existe. Elige una carpeta nueva que aún no exista.';

  @override
  String get recoveryTitle => 'Reabrir espacio de trabajo';

  @override
  String get visualApplyColor => 'Aplicar color';

  @override
  String get visualApplyComponent => 'Aplicar a este componente';

  @override
  String get visualApplyTexture => 'Aplicar medio';

  @override
  String visualAttachmentDetails(String action, String extension, String size) {
    return '$extension · $size · $action';
  }

  @override
  String get visualAttachmentFailure =>
      'La operación de archivo falló. Comprueba el archivo y el espacio disponible.';

  @override
  String get visualAttachmentPreview => 'Vista previa del adjunto local';

  @override
  String get visualAttachmentReadFailure =>
      'No se pudo leer el adjunto. Impórtalo de nuevo.';

  @override
  String get visualAudio => 'Audio';

  @override
  String get visualAudioStateFailure =>
      'No se pudo confirmar el estado del audio. Reintenta.';

  @override
  String get visualAutoLyrics =>
      'Buscar automáticamente letras faltantes en línea';

  @override
  String get visualCancel => 'Cancelar';

  @override
  String get visualChangeCover => 'Cambiar portada';

  @override
  String get visualChooseAudio =>
      'Elige un archivo de audio o un LRC con el mismo nombre.';

  @override
  String get visualChooseLyrics => 'Elige un archivo de letras LRC o TXT.';

  @override
  String get visualClickPreview => 'Selecciona para previsualizar';

  @override
  String get visualClose => 'Cerrar';

  @override
  String get visualCloseDialog => 'Cerrar diálogo';

  @override
  String get visualCloseWindow => 'Cerrar ventana';

  @override
  String get visualCollapsePlaylist => 'Contraer lista';

  @override
  String get visualColorGuide =>
      'Arrastra la rueda para elegir tono y saturación; después ajusta el brillo. También puedes introducir un valor de color.';

  @override
  String get visualColorTitle => 'Da color a tu espacio';

  @override
  String visualComponentCompass(String title) {
    return '$title · Rueda de color';
  }

  @override
  String get visualComponents => 'Componentes y tarjetas';

  @override
  String get visualComponentsGuide =>
      'Cada elemento sigue el tema de forma predeterminada. Personaliza una tarjeta sin cambiar las demás.';

  @override
  String get visualCornerTips1 =>
      'No todas las ideas deben ser útiles.\nAlgunas solo hacen el día más interesante.';

  @override
  String get visualCornerTips2 =>
      'Anótala y déjala crecer.\nUna idea no tiene que llegar completa.';

  @override
  String get visualCornerTips3 =>
      'Deja un poco de espacio para ti.\nLa curiosidad necesita respirar.';

  @override
  String get visualCornerTips4 =>
      'Prueba algo nuevo hoy.\nUn pequeño desvío puede sorprenderte.';

  @override
  String get visualCornerTips5 =>
      'Soñar despierto puede llevarte lejos.\nDeja que tus pensamientos paseen.';

  @override
  String get visualCornerTips6 =>
      'Haz tiempo para lo que te gusta.\nNo necesitas demostrar su valor.';

  @override
  String get visualCornerTips7 =>
      'El progreso puede ser pequeño.\nQuerer empezar ya importa.';

  @override
  String get visualCornerTips8 =>
      'Mira por la ventana de vez en cuando.\nLa vida también inspira.';

  @override
  String get visualCover => 'Portada';

  @override
  String get visualCustomCompass => 'Rueda de color · Personalizado';

  @override
  String get visualCustomMaterialGuide =>
      'Desactiva para seguir el tema y conservar los ajustes personalizados de este elemento.';

  @override
  String get visualDefaultOpen => 'Abrir con aplicación predeterminada';

  @override
  String get visualDownloadOpen => 'Descargar para abrir';

  @override
  String get visualEmbeddedLyrics => 'Incrustadas en el audio';

  @override
  String get visualExpandPlaylist => 'Expandir lista';

  @override
  String get visualFile => 'Archivo';

  @override
  String get visualFileOpenFailure =>
      'No se pudo abrir el archivo. Instala una aplicación compatible o guarda el adjunto y ábrelo allí.';

  @override
  String get visualFileRetry => 'La operación de archivo falló. Reintenta.';

  @override
  String get visualFindLyrics => 'Encontrar letras';

  @override
  String get visualFindLyricsGuide =>
      'Busca en LRCLIB por canción y artista y elige la versión correcta.';

  @override
  String get visualFollowTheme => 'Seguir tema';

  @override
  String get visualFooterLyrics => 'Mostrar letras al pie';

  @override
  String get visualFooterTips => 'Mostrar consejos al pie';

  @override
  String get visualFooterTips1 => 'No hay prisa. Dale tiempo a la curiosidad.';

  @override
  String get visualFooterTips10 =>
      'No hace falta llenar cada minuto. Deja un poco de espacio.';

  @override
  String get visualFooterTips2 => 'Anota una idea. Ya la organizarás después.';

  @override
  String get visualFooterTips3 =>
      'Convierte una gran idea en un pequeño paso para hoy.';

  @override
  String get visualFooterTips4 => 'Estírate un poco y descansa la vista.';

  @override
  String get visualFooterTips5 =>
      'Una idea puede quedarse sin respuesta por ahora.';

  @override
  String get visualFooterTips6 =>
      'Algunos descubrimientos llegan al bajar el ritmo.';

  @override
  String get visualFooterTips7 =>
      'Guardar un detalle ayuda a cultivar una idea.';

  @override
  String get visualFooterTips8 =>
      'Una nota de hoy puede ser el comienzo de mañana.';

  @override
  String get visualFooterTips9 =>
      'Deja vagar la mente y vuelve a lo que disfrutas.';

  @override
  String get visualFrosting => 'Desenfoque';

  @override
  String get visualGif => 'GIF animado';

  @override
  String get visualHexColor => 'Color HEX';

  @override
  String get visualHexInvalid =>
      'Introduce un color hexadecimal de seis dígitos.';

  @override
  String get visualImage => 'Imagen';

  @override
  String get visualImageDecodeFailure =>
      'No se puede decodificar la imagen. Guárdala y ábrela con otra aplicación.';

  @override
  String visualImageLoadFailure(String name) {
    return 'No se pudo cargar la imagen: $name';
  }

  @override
  String visualImageNotImported(String name) {
    return '$name (imagen no importada)';
  }

  @override
  String visualImageUnavailable(String name) {
    return 'Imagen no disponible: $name';
  }

  @override
  String get visualImportFailure =>
      'La importación falló. Comprueba archivo, codificación y espacio disponible.';

  @override
  String get visualImportLyrics => 'Importar letras';

  @override
  String get visualImportMusic => 'Importar música';

  @override
  String get visualImportMusicHint =>
      'Selecciona + para importar canciones locales';

  @override
  String get visualIndependentMaterial => 'Material personalizado';

  @override
  String get visualInheritColor => 'Usar tinte del tema';

  @override
  String get visualLinkFailure =>
      'No se pudo abrir el enlace. Copia la dirección e inténtalo de nuevo.';

  @override
  String visualLoadImage(String name) {
    return 'Cargar imagen · $name';
  }

  @override
  String get visualLoading => 'Cargando…';

  @override
  String get visualLyricsEmpty => 'El archivo de letras está vacío.';

  @override
  String get visualLyricsFile => 'Archivo de letras';

  @override
  String get visualLyricsImportHint =>
      'Importa un archivo de letras o busca en línea.';

  @override
  String get visualLyricsLoading => 'Cargando letras…';

  @override
  String visualLyricsMatch(String album, String kind, int seconds) {
    String _temp0 = intl.Intl.pluralLogic(
      seconds,
      locale: localeName,
      other: '$seconds segundos',
      one: '$seconds segundo',
    );
    return '$album\n$kind · $_temp0';
  }

  @override
  String get visualLyricsMissing =>
      'No se encontraron letras. Importa un archivo o busca de nuevo.';

  @override
  String get visualLyricsNotFound =>
      'No se encontraron letras. Prueba con otro título o artista.';

  @override
  String get visualLyricsOnPlay => 'Cargar letras durante la reproducción';

  @override
  String get visualLyricsParseFailure =>
      'No se pudieron interpretar las letras. Impórtalas de nuevo.';

  @override
  String get visualLyricsReadFailure =>
      'No se pudieron cargar las letras. Impórtalas manualmente o reintenta.';

  @override
  String get visualLyricsServiceFailure =>
      'No se pudo conectar al servicio de letras. Reintenta más tarde o importa letras locales.';

  @override
  String get visualLyricsSize =>
      'El archivo de letras debe ocupar como máximo 1 MB.';

  @override
  String get visualLyricsSources => 'Archivo local → Incrustadas → LRCLIB';

  @override
  String get visualLyricsVersions =>
      'Se encontraron varias versiones. Elige una en la búsqueda.';

  @override
  String get visualMaterialPreview => 'Vista previa del material';

  @override
  String get visualMaximize => 'Maximizar';

  @override
  String get visualMediaAddress => 'Dirección del medio';

  @override
  String get visualMediaAddressInvalid =>
      'Introduce una dirección HTTP o HTTPS válida sin datos de acceso.';

  @override
  String get visualMediaPreviewFailure =>
      'No se puede previsualizar este medio. Guárdalo y ábrelo con otra aplicación.';

  @override
  String get visualMediaType => 'Tipo de medio';

  @override
  String get visualMinimize => 'Minimizar';

  @override
  String get visualMusic => 'Música';

  @override
  String get visualMusicEmptyTitle => 'Haz espacio para la música';

  @override
  String get visualMusicPlayer => 'Reproductor de música';

  @override
  String get visualNextTrack => 'Pista siguiente';

  @override
  String get visualNoLyricsRead => 'No hay letras cargadas';

  @override
  String visualNoLyricsTitle(String title) {
    return '♪ $title · Sin letras';
  }

  @override
  String get visualNoTimeline => 'Sin datos de tiempo';

  @override
  String get visualOpacity => 'Opacidad';

  @override
  String get visualOptionalArtist => 'Artista (opcional)';

  @override
  String get visualPauseMusic => 'Pausar música';

  @override
  String get visualPaused => 'En pausa';

  @override
  String get visualPlainLyrics => 'Letras sin sincronizar';

  @override
  String get visualPlayMusic => 'Reproducir música';

  @override
  String get visualPlaybackFailure =>
      'No se puede reproducir la canción. Revisa el archivo o prueba otro formato.';

  @override
  String visualPlaybackPosition(int count, int index, String state) {
    return '$index / $count · $state';
  }

  @override
  String get visualPlaybackRequestFailure =>
      'No se pudo iniciar la reproducción. Reintenta.';

  @override
  String get visualPlaying => 'Reproduciendo';

  @override
  String get visualPlaylistEmpty => 'Tu lista está vacía';

  @override
  String get visualPlaylistLyricsHint =>
      'Importa letras LRC desde el menú de la lista';

  @override
  String get visualPlaylistSaved =>
      'La lista y las letras se guardan automáticamente';

  @override
  String get visualPlaylistUpdateFailure =>
      'No se pudo actualizar la lista. Reintenta.';

  @override
  String get visualPreviewColor => 'Vista previa del color';

  @override
  String get visualPreviousTrack => 'Pista anterior';

  @override
  String get visualRemoveAttachment => 'Quitar adjunto';

  @override
  String get visualRemoveTrack => 'Quitar de la lista';

  @override
  String get visualResetMaterial => 'Restablecer al tema';

  @override
  String get visualRestoreWindow => 'Restaurar';

  @override
  String get visualSaveAttachment => 'Guardar adjunto como';

  @override
  String get visualSearch => 'Buscar';

  @override
  String get visualSearchLyrics => 'Buscar letras';

  @override
  String get visualSongCover => 'Portada del álbum';

  @override
  String get visualSongTitle => 'Título de la canción';

  @override
  String get visualSyncedLyrics => 'Letras sincronizadas';

  @override
  String get visualTextureFailure =>
      'No se pudo cargar el medio. Revisa el archivo, la dirección y el formato. En la web también se requiere acceso entre orígenes.';

  @override
  String get visualTextureLinkGuide =>
      'Pega un enlace HTTP o HTTPS directo a una imagen, GIF o vídeo. Para páginas compartidas, busca primero la dirección original del medio.';

  @override
  String get visualTextureLinkTitle => 'Trae un poco de inspiración';

  @override
  String get visualTexturePlaybackGuide =>
      'Los vídeos se repiten sin sonido por defecto; actívalo en ajustes. Los medios en línea deben permitir acceso, incluida la carga entre orígenes en la web.';

  @override
  String get visualTipsMaterialGuide =>
      'Desactivado: consejos transparentes; activado: material de abajo. Los valores se conservan.';

  @override
  String get visualTransparentTips =>
      'Superposición transparente (predeterminada)';

  @override
  String get visualUseCustomMaterial => 'Usar material personalizado';

  @override
  String get visualVideo => 'Vídeo';

  @override
  String get visualViewLyrics => 'Ver letras';
}
