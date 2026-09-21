// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Russian (`ru`).
class AppLocalizationsRu extends AppLocalizations {
  AppLocalizationsRu([String locale = 'ru']) : super(locale);

  @override
  String get commonAppName => 'Morrow';

  @override
  String get commonCancel => 'Отмена';

  @override
  String commonCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count элемента',
      many: '$count элементов',
      few: '$count элемента',
      one: '$count элемент',
      zero: 'Нет элементов',
    );
    return '$_temp0';
  }

  @override
  String commonGreeting(String name) {
    return 'Здравствуйте, $name';
  }

  @override
  String get importsAttachmentLimit =>
      'За раз можно импортировать 20 вложений. Остальные вставьте отдельно.';

  @override
  String get importsClipboardChanged =>
      'Буфер обмена изменился во время чтения. Вставьте заново.';

  @override
  String get importsEmbeddedImageUnreadable =>
      'Не удалось прочитать встроенное изображение.';

  @override
  String get importsEmbeddedImagesSeparate =>
      'Некоторые встроенные изображения нужно импортировать отдельными файлами.';

  @override
  String get importsExcelValues =>
      'Значения и формулы преобразованы в Markdown. Оформление и объединённые ячейки сохранены во вложении XML.';

  @override
  String get importsExcelXmlKept =>
      'Исходная таблица Excel сохранена вложением XML.';

  @override
  String get importsFileTooLarge => 'Файл в буфере обмена превышает 200 МБ.';

  @override
  String get importsItemLimit =>
      'Прочитаны только первые 20 элементов. Остальные вставьте отдельно.';

  @override
  String get importsItemUnreadable =>
      'Один элемент буфера обмена не удалось прочитать. Остальное доступное содержимое сохранено.';

  @override
  String get importsLocalImageNotRead =>
      'Локальные изображения по ссылкам не читаются автоматически. Вставьте изображение или импортируйте исходный файл.';

  @override
  String get importsMergedTable =>
      'Объединённые ячейки преобразованы в читаемую таблицу. Исходное оформление сохранено во вложении HTML.';

  @override
  String get importsOfficeBusy =>
      'Буфер обмена занят другим приложением. Объекты Office не прочитаны.';

  @override
  String get importsOfficeEmbeddedKept =>
      'Встроенный объект Office сохранён исходным вложением. Редактируйте диаграммы, формулы и макет в исходном приложении.';

  @override
  String get importsOfficeExportFailed =>
      'Объект Office превышает лимит или не экспортируется. Сохраните в исходном приложении и импортируйте.';

  @override
  String get importsOfficeReadFailed =>
      'Не удалось прочитать Office. Остальное содержимое буфера обмена доступно.';

  @override
  String get importsOfficeUnavailable =>
      'Буфер обмена Office временно недоступен.';

  @override
  String get importsOfficeUnreadable =>
      'Исходный объект Office не прочитан. Остальное доступное содержимое сохранено.';

  @override
  String get importsRichFallback =>
      'Часть форматирования не преобразована. Читаемый текст сохранён.';

  @override
  String get importsRichTooLarge =>
      'Форматированный текст превышает 2 МБ. Импортируйте документ вложением.';

  @override
  String get importsRtfTooLarge =>
      'RTF слишком велик. Импортируйте исходный документ.';

  @override
  String get importsSpreadsheetTooLarge =>
      'Таблица слишком велика. Импортируйте файл Excel.';

  @override
  String get importsTableConverted =>
      'Таблица преобразована в Markdown. Полные данные сохранены во вложении TSV.';

  @override
  String get importsTextTooLarge =>
      'Текст превышает 2 МБ. Импортируйте его файлом.';

  @override
  String get importsTotalTooLarge =>
      'Общий размер вставляемых файлов превышает 200 МБ. Импортируйте меньшими группами.';

  @override
  String get importsUnsupported =>
      'Буфер обмена здесь не поддерживается. Импортируйте файл.';

  @override
  String get mainActiveProjects => 'В работе';

  @override
  String get mainAdjustCustomTone => 'Настроить цвет';

  @override
  String get mainAmbientDetail => 'Плавный свет добавляет немного цвета.';

  @override
  String get mainAppTitle => 'Morrow — Пространство для идей';

  @override
  String get mainAppearance => 'Оформление';

  @override
  String get mainArrangeIdeas => 'Сортировать идеи';

  @override
  String mainAttachmentCount(int count) {
    return 'Вложения · $count';
  }

  @override
  String mainAttachmentHint(int count, String name) {
    return 'Вложений: $count · $name';
  }

  @override
  String get mainAttachmentLimit => 'До 20 вложений в одной записи.';

  @override
  String get mainAutosaveNotice => 'Оформление и идеи сохраняются локально';

  @override
  String get mainAwaitDiscovery => 'В ожидании открытия';

  @override
  String get mainBackToWorkbench => 'Вернуться в рабочую область';

  @override
  String get mainBackgroundCanvas => 'Фоновое полотно';

  @override
  String get mainBackgroundSound => 'Включить фоновый звук';

  @override
  String get mainBodyHint =>
      'Запишите мысли или вставьте содержимое…\n\nПоддерживаются # заголовки, списки, таблицы и блоки кода';

  @override
  String get mainBrightWhite => 'Белый';

  @override
  String get mainBuiltinTexture => 'Встроенная текстура';

  @override
  String get mainCanvasCompass => 'Цветовой круг фона';

  @override
  String get mainCaptureIdea => 'Записать идею';

  @override
  String get mainCaptureNow => 'Записать мысль';

  @override
  String mainCardAttachments(int count, String name) {
    return 'Файлов: $count · $name';
  }

  @override
  String get mainCategoryExperiment => 'Эксперимент';

  @override
  String get mainCategoryIdea => 'Идея';

  @override
  String get mainCategoryProject => 'Проект';

  @override
  String get mainCategoryPrompt => 'Где сохранить';

  @override
  String get mainChangeFailed =>
      'Изменение не сохранено. Черновик сохранён; можно повторить.';

  @override
  String get mainCheckAgain => 'Проверить снова';

  @override
  String get mainClearSearch => 'Очистить поиск';

  @override
  String get mainClipboardEmpty =>
      'В буфере обмена нет доступных текста или файлов. Скопируйте файл в файловом менеджере или используйте импорт.';

  @override
  String get mainClipboardReadFailed =>
      'Не удалось прочитать содержимое. Импортируйте файл или проверьте разрешения на файл и буфер обмена.';

  @override
  String get mainClipboardSupport =>
      'Поддерживаются Markdown, форматированный текст и таблицы Office, снимки экрана и файлы. Сложные объекты сохраняются исходными вложениями. До 20 вложений по 200 МБ.';

  @override
  String get mainCollapseSidebar => 'Свернуть боковую панель';

  @override
  String get mainCompletedProjects => 'Завершено';

  @override
  String get mainComponentCompass => 'Цветовой круг компонента';

  @override
  String get mainComponentEmpty => 'Пустое состояние';

  @override
  String get mainComponentFooter => 'Подсказки и текст песни';

  @override
  String get mainComponentHero => 'Карточка обзора';

  @override
  String get mainComponentNavigation => 'Боковая навигация';

  @override
  String get mainComponentQuickCapture => 'Быстрая запись';

  @override
  String get mainComponentSearch => 'Строка поиска';

  @override
  String get mainComponentSettings => 'Компоненты и карточки · Настройки';

  @override
  String get mainContentProtection => 'Защита содержимого';

  @override
  String get mainContentRead => 'Содержимое прочитано';

  @override
  String mainContentReadFiles(int count) {
    return 'Содержимое прочитано; вложений сохранено: $count';
  }

  @override
  String get mainCornerRadius => 'Радиус углов';

  @override
  String get mainCredentialSettings => 'Учётные данные';

  @override
  String get mainCrystal => 'Прозрачное';

  @override
  String get mainCrystalDetail => 'Лёгкость и прозрачность, раскрывающие цвет.';

  @override
  String get mainCuriosity => 'Интересное начинается\nс капельки любопытства.';

  @override
  String get mainCustomCompass => 'Цветовой круг · Настройка';

  @override
  String get mainCustomLightness => 'Яркость цвета';

  @override
  String get mainCustomTheme => 'Своя';

  @override
  String get mainDaily => 'Мелочи дня';

  @override
  String get mainDailyExplore => 'Уделите десять минут открытиям';

  @override
  String get mainDailyIdea => 'Запишите идею';

  @override
  String get mainDailyWater => 'Налейте себе воды';

  @override
  String get mainDarkTheme => 'Тёмная';

  @override
  String get mainDeepBlack => 'Чёрный';

  @override
  String get mainDefaultCanvas => 'Обычный';

  @override
  String get mainDefaultGlobalColor => 'Цвет темы по умолчанию · Все элементы';

  @override
  String get mainDelete => 'Удалить';

  @override
  String mainDeleted(String title) {
    return '«$title» удалено';
  }

  @override
  String get mainDiagnosticDetails => 'Диагностические сведения';

  @override
  String get mainDone => 'Готово';

  @override
  String get mainEdit => 'Изменить';

  @override
  String get mainEditIdeaTitle => 'Сделайте идею яснее';

  @override
  String get mainEditorClosedUnknown =>
      'Редактор закрыт, но сохранение не подтверждено. Откройте рабочую область и проверьте результат, прежде чем создавать копию.';

  @override
  String get mainEditorSubtitle =>
      'Текст, таблицы, изображения — храните их здесь, пока идея обретает форму.';

  @override
  String get mainEditorUnavailable =>
      'Редактор недоступен. Проверьте службу содержимого и повторите.';

  @override
  String get mainEndpointSettings => 'Исходящие конечные точки';

  @override
  String get mainExpandSettings => 'Развернуть настройки';

  @override
  String get mainExpandSidebar => 'Развернуть боковую панель';

  @override
  String get mainExtensionPlugins => 'Расширения';

  @override
  String get mainFavoriteAttachments => 'Избранные вложения';

  @override
  String get mainFavoriteRecords => 'Избранные записи';

  @override
  String mainFavoriteTooltip(String title) {
    return 'В избранное: $title';
  }

  @override
  String get mainFavoritesIntro =>
      'Любимые тексты, изображения и файлы в одном месте.';

  @override
  String mainFieldLimit(int limit) {
    return 'Не более $limit символов. Сократите текст или импортируйте его файлом.';
  }

  @override
  String get mainFilterAll => 'Все';

  @override
  String get mainFilterAttachments => 'С файлами';

  @override
  String get mainFilterFavorites => 'Только избранное';

  @override
  String get mainFilterFile => 'Файлы';

  @override
  String get mainFilterImage => 'Изображения';

  @override
  String get mainFilterMedia => 'Аудио / видео';

  @override
  String get mainFilterPending => 'К выполнению';

  @override
  String get mainFilterText => 'Текст';

  @override
  String get mainFollowTheme => 'По теме';

  @override
  String get mainFrostDetail => 'Смягчите фон и освободите место для мыслей.';

  @override
  String get mainFrostEffect => 'Размытие';

  @override
  String get mainFrostOpacity => 'Непрозрачность стекла';

  @override
  String get mainFrostUnavailable =>
      'Размытие рабочего стола недоступно. Цвет и непрозрачность можно изменить.';

  @override
  String get mainFrosted => 'Матовое';

  @override
  String get mainGlassTexture => 'Стиль стекла';

  @override
  String mainGlobalColor(String color) {
    return '$color · Все элементы';
  }

  @override
  String get mainGreeting => 'Пусть ваши идеи растут.';

  @override
  String get mainGreetingDetail =>
      'Сохраняйте детали повседневности и искры вдохновения.';

  @override
  String get mainHeroBody =>
      'Мысль, небольшая задача, «а что если».\nВсё начинается здесь.';

  @override
  String get mainHeroCaption => 'УГОЛОК ВОЗМОЖНОСТЕЙ';

  @override
  String get mainHeroTitle => 'Можно начать с малого.';

  @override
  String get mainHideAppearance => 'Скрыть настройки оформления';

  @override
  String get mainHideCustomTone => 'Скрыть настройку цвета';

  @override
  String get mainHidePreview => 'Скрыть предпросмотр';

  @override
  String get mainHttpSettings => 'HTTP-задачи';

  @override
  String get mainHypothesis => 'Гипотеза';

  @override
  String get mainHypothesisPrompt => 'Гипотеза для проверки';

  @override
  String get mainHypothesisSection => 'Гипотеза / Что попробовать';

  @override
  String get mainIdeaDetails =>
      'Сохраните детали. Пусть следующий шаг станет понятнее.';

  @override
  String get mainIdeaNameHint => 'Дайте название';

  @override
  String get mainIdeaNameRequired => 'Сначала запишите идею';

  @override
  String get mainIdeaSaved => 'Идея сохранена.';

  @override
  String get mainImportFailed =>
      'Не удалось импортировать медиа. Проверьте файл и свободное место.';

  @override
  String get mainImportFile => 'Импортировать файл';

  @override
  String get mainInboxIntro =>
      'Сначала записывайте, потом разбирайте. Превращайте удачные идеи в небольшие проекты.';

  @override
  String get mainIoNoDeclarations =>
      'Установленные плагины не запрашивают доступ к файлам или сети.';

  @override
  String get mainIoSettings => 'Сеть и файлы';

  @override
  String get mainIoSettingsGuide =>
      'Управляйте доступом плагинов к файлам и сети отдельно от оформления. Одобрение возможности не даёт доступ ко всем файлам и адресам; операции зависят от текущего бэкенда.';

  @override
  String get mainIoSettingsSummary =>
      'Разрешения, учётные данные, конечные точки и API';

  @override
  String get mainJustNow => 'Только что';

  @override
  String get mainLabIntro =>
      'Начните с гипотезы. Сохраняйте попытки, наблюдения и неожиданные открытия.';

  @override
  String get mainLanguage => 'Язык';

  @override
  String get mainLanguageChinese => '简体中文';

  @override
  String get mainLanguageEnglish => 'English';

  @override
  String get mainLanguageSystem => 'Как в системе';

  @override
  String get mainLavender => 'Лаванда';

  @override
  String get mainLightOpacity => '20% · Лёгкое';

  @override
  String get mainLiquidAllCanvases =>
      'Отдельно доступен для всех четырёх типов фона';

  @override
  String get mainLiquidDetail =>
      'Плавные блики и мягкое преломление, словно капля воды в воздухе.';

  @override
  String get mainLiquidEffect => 'Эффект жидкого стекла';

  @override
  String get mainLiquidGlass => 'Жидкое стекло';

  @override
  String get mainLivePreview => 'Живой предпросмотр';

  @override
  String get mainLocalMedia => 'Локальные медиа';

  @override
  String get mainMakeYours => 'ПО-ВАШЕМУ';

  @override
  String get mainMarkOrganized => 'Отметить как разобранное';

  @override
  String get mainMarkdownBody => 'Текст · Markdown';

  @override
  String get mainMediaLimits => 'Изображения / GIF ≤ 25 МБ; видео ≤ 150 МБ';

  @override
  String get mainMonochrome => 'Монохромное';

  @override
  String mainMoreSteps(int count) {
    return 'Ещё шагов: $count; откройте для просмотра';
  }

  @override
  String mainMovedProject(String title) {
    return '«$title» перенесено в проекты';
  }

  @override
  String get mainMusic => 'Музыкальный плеер';

  @override
  String get mainMySpace => 'Моё пространство';

  @override
  String get mainNavigation => 'Навигация';

  @override
  String get mainNewIdea => 'Новая идея';

  @override
  String get mainNewIdeaTitle => 'Поймайте новую идею';

  @override
  String get mainNoHypothesis => 'Гипотезы пока нет';

  @override
  String get mainNoMatches => 'Подходящих идей нет';

  @override
  String get mainNoResultYet =>
      'Результат подождёт. Процесс тоже стоит записать.';

  @override
  String get mainNotNow => 'Не сейчас';

  @override
  String get mainObservationSection => 'Наблюдения / Что выяснилось';

  @override
  String get mainObservations => 'Наблюдения и выводы';

  @override
  String get mainObservationsPrompt => 'Наблюдения, процесс и выводы';

  @override
  String get mainOneHourAgo => 'Час назад';

  @override
  String get mainOnlineMedia => 'Онлайн-медиа';

  @override
  String get mainOpaqueFallback =>
      'Прозрачные панели поверх цвета текущей темы.';

  @override
  String get mainOpenNextStep =>
      'Откройте проект, чтобы изменить следующие шаги';

  @override
  String get mainOrganizedCount => 'Разобрано';

  @override
  String get mainOriginalColors => 'Исходные цвета';

  @override
  String get mainPageFavorites => 'Избранное';

  @override
  String get mainPageInbox => 'Входящие';

  @override
  String get mainPageLaboratory => 'Лаборатория';

  @override
  String get mainPageOverview => 'Обзор';

  @override
  String get mainPageProjects => 'Проекты';

  @override
  String mainPageSummary(String page) {
    return '$page · Обзор';
  }

  @override
  String get mainPasteChanged =>
      'Во время вставки ввод изменился. Откройте редактор заново.';

  @override
  String get mainPasteContent => 'Вставить содержимое';

  @override
  String get mainPause => 'Пауза';

  @override
  String get mainPersonalWorkspace => 'Личная рабочая область';

  @override
  String get mainPlay => 'Воспроизвести';

  @override
  String get mainPluginSettings => 'Плагины и службы';

  @override
  String get mainPluginSettingsSummary =>
      'Встроенные инструменты, расширения, сеть, файлы и защита данных';

  @override
  String get mainPreviewEmpty => 'Здесь появится предпросмотр';

  @override
  String mainProgress(int done, int total) {
    return 'Маленькие шаги · $done/$total';
  }

  @override
  String get mainProjectIntro =>
      'Продвигайтесь по списку задач. Каждый маленький шаг приближает к цели.';

  @override
  String get mainQueryAgain => 'Новый запрос';

  @override
  String get mainQueryCapacity => 'История запросов заполнена';

  @override
  String get mainQueryCapacityDetail =>
      'Данные сохранены. Эта версия пока не позволяет очистить историю запросов.';

  @override
  String get mainQueryLoading => 'Поиск идей…';

  @override
  String get mainQueryRetry => 'Повторить запрос';

  @override
  String get mainQueryTerminated => 'Запрос завершён';

  @override
  String get mainQueryUnknown => 'Результаты пока не подтверждены';

  @override
  String get mainQuickHint => 'Что пришло в голову?';

  @override
  String get mainReadOnlySettings =>
      'Содержимое доступно только для чтения. Проверьте плагин рабочей области, чтобы вернуть редактирование.';

  @override
  String get mainRecentThoughts => 'Недавние мысли';

  @override
  String get mainRecordedCount => 'С наблюдениями';

  @override
  String get mainRestoreDefault => 'По умолчанию';

  @override
  String get mainRetry => 'Повторить';

  @override
  String get mainRetrySave => 'Повторить сохранение';

  @override
  String get mainSage => 'Шалфей';

  @override
  String get mainSampleBody0 =>
      'Сохраняйте внезапные мысли здесь.\nНе спешите завершать — просто начните.';

  @override
  String get mainSampleBody1 =>
      'Небольшая страница для любимых слов,\nмузыки и повседневных деталей.';

  @override
  String get mainSampleBody2 =>
      'Попробуйте генеративное искусство. Пусть код\nсоздаёт неожиданные формы.';

  @override
  String get mainSampleBody3 =>
      'Тихий спутник, который поможет помнить\nмелочи, ускользающие из памяти.';

  @override
  String get mainSampleTitle0 => 'Дом для идей';

  @override
  String get mainSampleTitle1 => 'Тихий цифровой сад';

  @override
  String get mainSampleTitle2 => 'Сделать что-то ради удовольствия';

  @override
  String get mainSampleTitle3 => 'Мой маленький помощник';

  @override
  String get mainSampleTodo0 => 'Разобрать первую коллекцию';

  @override
  String get mainSampleTodo1 => 'Оформить вход в сад';

  @override
  String get mainSampleTodo2 => 'Посадить новую идею';

  @override
  String get mainSampleTodo3 => 'Набросать небольшой прототип';

  @override
  String get mainSampleTodo4 => 'Продумать напоминания';

  @override
  String get mainSaveConnectionUnknown =>
      'Соединение прервано; результат сохранения неизвестен. Откройте библиотеку заново и проверьте перед повтором.';

  @override
  String get mainSaveFailed =>
      'Не удалось сохранить. Изменения сохранены в текущем сеансе.';

  @override
  String get mainSaveIdea => 'Сохранить идею';

  @override
  String get mainSaveNotSubmitted =>
      'Не отправлено. Черновик и вложения сохранены; можно исправить и сохранить снова.';

  @override
  String get mainSaveReadOnly =>
      'Изменения не сохранены. Включите плагин рабочей области в разделе «Плагины и службы» и повторите.';

  @override
  String get mainSaveUnknown =>
      'Сохранение не подтверждено. Черновик и вложения сохранены. Повторите эту отправку; при закрытии рабочая область обновится для проверки.';

  @override
  String get mainSaving => 'Сохранение…';

  @override
  String get mainSearchHint => 'Поиск идей…';

  @override
  String get mainServiceRunSettings => 'Работа служб';

  @override
  String get mainServiceSettings => 'Службы API';

  @override
  String get mainSettings => 'Настройки';

  @override
  String get mainShowAppearance => 'Показать настройки оформления';

  @override
  String get mainSidebarMotto => 'Немного порядка. Простор для открытий.';

  @override
  String get mainSlowProgress => 'Даже маленькие шаги ведут вперёд.';

  @override
  String get mainSolidCanvas => 'Однотонный';

  @override
  String get mainSolidDetail => 'Спокойный однотонный фон.';

  @override
  String get mainSolidOpacity => '100% · Плотное';

  @override
  String get mainSortFavorites => 'Сначала избранное';

  @override
  String get mainSortRecent => 'Недавно добавленные';

  @override
  String get mainSortTitle => 'По названию';

  @override
  String get mainSquareCorners => '0 — прямые углы';

  @override
  String get mainStageActive => 'В работе';

  @override
  String get mainStageCompleted => 'Завершено';

  @override
  String get mainStageOrganized => 'Разобрано';

  @override
  String get mainStagePlanned => 'Запланировано';

  @override
  String get mainStageRecorded => 'Записано';

  @override
  String mainStageTooltip(String title) {
    return 'Изменить этап: $title';
  }

  @override
  String get mainStageUnsorted => 'Нужно разобрать';

  @override
  String get mainStageUnverified => 'Нужно проверить';

  @override
  String get mainStageVerifying => 'Проверяется';

  @override
  String get mainStayCurious => 'БУДЬТЕ ЛЮБОПЫТНЫ. БУДЬТЕ СОБОЙ.';

  @override
  String mainSteps(int done, int total) {
    return 'Шаги: $done/$total';
  }

  @override
  String get mainStorageUnavailable =>
      'Локальное хранилище недоступно. Изменения сохранятся только до конца сеанса.';

  @override
  String get mainStorageUnreadable =>
      'Не удалось прочитать сохранённые данные. Исходные данные сохранены и не будут перезаписаны.';

  @override
  String get mainTenMinutesAgo => '10 минут назад';

  @override
  String get mainTextureCanvas => 'Текстура';

  @override
  String get mainTextureDetail =>
      'Тонкая бумажная текстура создаёт ощущение материала.';

  @override
  String get mainThemeCompass => 'Цветовой круг темы';

  @override
  String get mainThemeGrayscale => 'Обесцвечивание темы';

  @override
  String get mainThemeTone => 'Цвета темы';

  @override
  String get mainThreeHoursAgo => '3 часа назад';

  @override
  String get mainTintOpacity => 'Непрозрачность оттенка';

  @override
  String get mainToProject => 'Перенести в проекты';

  @override
  String get mainTodosPrompt =>
      'Следующие шаги (по одному в строке, необязательно)';

  @override
  String get mainTransparencyUnavailable =>
      'Не удалось включить прозрачность системы. Можно использовать обычный фон.';

  @override
  String get mainTransparentCanvas => 'Прозрачный';

  @override
  String get mainTransparentDetail =>
      'Показывает пространство за окном; в веб-версии — фон страницы.';

  @override
  String get mainUndo => 'Отменить';

  @override
  String mainUnfavoriteTooltip(String title) {
    return 'Убрать из избранного: $title';
  }

  @override
  String get mainUnsortedCount => 'Нужно разобрать';

  @override
  String get mainUnverifiedCount => 'Нужно проверить';

  @override
  String get mainView => 'Просмотр';

  @override
  String get mainViewAll => 'Показать всё';

  @override
  String get mainWarmSand => 'Тёплый песок';

  @override
  String get mainWhiteTheme => 'Светлая';

  @override
  String get mainWindowRadius => 'Углы окна';

  @override
  String get mainWindowRadiusDetail =>
      'Отдельная настройка рамки; при разворачивании углы прямые';

  @override
  String get mainWindowsFrostOnly =>
      'Размытие рабочего стола доступно только в Windows';

  @override
  String get mainWorkbench => 'Рабочая область';

  @override
  String get mainWorkbenchPlugin => 'Плагин рабочей области';

  @override
  String get mainWriteHypothesis =>
      'Откройте запись и опишите, что хотите проверить.';

  @override
  String get mainYesterday => 'Вчера';

  @override
  String get pluginsApprovalUnknown =>
      'Не удалось подтвердить включение или изменение разрешений';

  @override
  String get pluginsApproveEnable => 'Разрешить и включить';

  @override
  String get pluginsApproveWorkbench =>
      'Разрешить чтение и изменение и включить';

  @override
  String get pluginsAttachment => 'Читать вложения';

  @override
  String get pluginsBackingUp => 'Копирование…';

  @override
  String get pluginsBackupLibrary => 'Скопировать библиотеку';

  @override
  String get pluginsBackupLibraryType => 'Резервная копия библиотеки';

  @override
  String get pluginsBackupProtection => 'Скопировать файл защиты';

  @override
  String get pluginsBackupUnknown =>
      'Резервное копирование ещё не подтверждено. Сохраните созданные файлы и проверьте папку назначения.';

  @override
  String pluginsBinaryPreview(String hex) {
    return 'Двоичное содержимое: $hex';
  }

  @override
  String get pluginsBuiltin => 'Встроенное пространство';

  @override
  String pluginsBuiltinCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count символа',
      many: '$count символов',
      few: '$count символа',
      one: '$count символ',
    );
    return '$_temp0 · Только в этом сеансе; не сохранено как карточка';
  }

  @override
  String get pluginsBuiltinEmpty =>
      'Введите текст для предпросмотра в верхнем регистре';

  @override
  String get pluginsBuiltinHeading => 'Текстовые инструменты';

  @override
  String get pluginsBuiltinInput => 'Введите текст';

  @override
  String get pluginsCancel => 'Отмена';

  @override
  String get pluginsChoosePackage => 'Выбрать файл плагина';

  @override
  String get pluginsChooseSmallFile => 'Выбрать небольшой файл';

  @override
  String get pluginsCloseTextTool => 'Скрыть текстовый инструмент';

  @override
  String get pluginsCloseUnknown =>
      'Не удалось подтвердить закрытие представления';

  @override
  String get pluginsCloseView => 'Закрыть представление';

  @override
  String get pluginsConnectionLost =>
      'Соединение прервано. Откройте представление плагина заново.';

  @override
  String get pluginsContentPermissions => 'Разрешения содержимого';

  @override
  String get pluginsCreate => 'Создавать содержимое';

  @override
  String get pluginsCredentialCancel => 'Закрыть форму';

  @override
  String get pluginsCredentialCreateTitle => 'Новые учётные данные';

  @override
  String pluginsCredentialDays(int days) {
    String _temp0 = intl.Intl.pluralLogic(
      days,
      locale: localeName,
      other: '$days дня',
      many: '$days дней',
      few: '$days дня',
      one: '$days день',
    );
    return '$_temp0';
  }

  @override
  String get pluginsCredentialDetails =>
      'Безопасно храните учётные данные для разрешённых API. Сохранение не разрешает сервер и не включает плагин. Просмотреть сохранённые секреты нельзя.';

  @override
  String get pluginsCredentialDisable => 'Отключить';

  @override
  String get pluginsCredentialDisabled => 'Отключены';

  @override
  String get pluginsCredentialDisabledDone => 'Учётные данные отключены.';

  @override
  String get pluginsCredentialEmpty => 'Нет сохранённых учётных данных';

  @override
  String get pluginsCredentialExpired => 'Срок истёк';

  @override
  String pluginsCredentialExpires(String date) {
    return 'Срок действия: $date';
  }

  @override
  String get pluginsCredentialHeader => 'Имя заголовка';

  @override
  String get pluginsCredentialInvalid =>
      'Проверьте имя заголовка и введите новый секрет. Поле секрета очищено.';

  @override
  String get pluginsCredentialLifetime => 'Срок действия';

  @override
  String get pluginsCredentialLoadFailed =>
      'Не удалось согласованно прочитать учётные данные. Обновите состояние для повторной попытки.';

  @override
  String get pluginsCredentialNew => 'Добавить учётные данные';

  @override
  String get pluginsCredentialReading => 'Чтение учётных данных…';

  @override
  String pluginsCredentialReference(String reference) {
    return 'Учётные данные $reference';
  }

  @override
  String get pluginsCredentialRefresh => 'Обновить состояние';

  @override
  String get pluginsCredentialReplace => 'Заменить секрет';

  @override
  String pluginsCredentialReplaceTitle(String reference) {
    return 'Заменить учётные данные $reference';
  }

  @override
  String get pluginsCredentialSave => 'Сохранить учётные данные';

  @override
  String get pluginsCredentialSaved =>
      'Учётные данные сохранены. Подключения API требуют отдельного разрешения.';

  @override
  String get pluginsCredentialSecret => 'Новое значение секрета';

  @override
  String get pluginsCredentialStored => 'Сохранены';

  @override
  String get pluginsCredentialTitle => 'Учётные данные API';

  @override
  String get pluginsCredentialUnknown =>
      'Результат не подтверждён. Поле секрета очищено. Обновите состояние перед следующим изменением.';

  @override
  String pluginsDeclared(String permissions) {
    return 'Заявленные разрешения: $permissions';
  }

  @override
  String get pluginsDependenciesNotice =>
      'Зависимости настраиваются в хосте. Эта страница не даёт им разрешений.';

  @override
  String get pluginsDisable => 'Отключить';

  @override
  String get pluginsDisableWorkbench => 'Отключить плагин пространства';

  @override
  String get pluginsDisabled => 'Выключен';

  @override
  String get pluginsDisabledDetails =>
      'Выключен. Разрешите чтение и изменение содержимого, чтобы использовать редактор и инструменты.';

  @override
  String get pluginsEdit => 'Изменять содержимое';

  @override
  String get pluginsEmptyLibrary => 'Сторонние плагины ещё не импортированы.';

  @override
  String get pluginsEmptyResult => '(пустой результат)';

  @override
  String get pluginsEnabled => 'Включён';

  @override
  String get pluginsEnabledDetails =>
      'Включён. Плагин может читать и изменять содержимое. Отключение сохраняет ваши данные.';

  @override
  String get pluginsEndpointAdvanced =>
      'Лимиты политики (в байтах, если не указано иное)';

  @override
  String get pluginsEndpointCertificate => 'Выбрать корневой сертификат DER';

  @override
  String get pluginsEndpointCertificateDetails =>
      'Необязательный корневой сертификат HTTPS: один двоичный DER (.der или .cer), до 32 КиБ. PEM и наборы сертификатов не принимаются. Удалите сертификат перед переходом на HTTP.';

  @override
  String get pluginsEndpointCertificateInvalid =>
      'Выберите один корректный двоичный сертификат DER (.der или .cer) до 32 КиБ.';

  @override
  String pluginsEndpointCertificateSelected(int bytes) {
    return 'Выбран корневой сертификат DER ($bytes байт)';
  }

  @override
  String get pluginsEndpointConcurrency => 'Параллельные запросы (1–128)';

  @override
  String get pluginsEndpointCreateTitle => 'Новое разрешение конечной точки';

  @override
  String get pluginsEndpointCredential => 'Ссылка на учётные данные';

  @override
  String get pluginsEndpointCredentialLifetime =>
      'Учётные данные должны действовать весь срок разрешения конечной точки. Их срок не продлевается.';

  @override
  String get pluginsEndpointCredentialUnavailable =>
      'Нужны заявленное и одобренное разрешение пакета на использование учётных данных и действующая сохранённая ссылка.';

  @override
  String get pluginsEndpointCredentialsFailed =>
      'Не удалось прочитать ссылки на учётные данные. Обновите состояние перед выбором.';

  @override
  String get pluginsEndpointDetails =>
      'Сохраните политику сервера для конкретного пакета и его хеша. Сохранение не подключает к сети, не включает плагин и не делает сетевые задачи сразу доступными.';

  @override
  String get pluginsEndpointDigest => 'Хеш пакета';

  @override
  String get pluginsEndpointDisabledDone =>
      'Разрешение конечной точки отключено.';

  @override
  String get pluginsEndpointEmpty =>
      'Нет сохранённых разрешений конечных точек';

  @override
  String get pluginsEndpointFrameBytes => 'Лимит кадра (1–131072 байт)';

  @override
  String get pluginsEndpointHeaderBytes => 'Максимум байт заголовков (1–16384)';

  @override
  String get pluginsEndpointInvalid =>
      'Проверьте пакет, origin, методы, срок 1–30 дней, разрешение учётных данных, сертификат и лимиты политики.';

  @override
  String get pluginsEndpointLifetime => 'Срок действия (1–30 дней)';

  @override
  String get pluginsEndpointLoadFailed =>
      'Не удалось согласованно прочитать разрешения конечных точек. Обновите состояние для повторной попытки.';

  @override
  String get pluginsEndpointLocalHttp => 'Локальный HTTP';

  @override
  String get pluginsEndpointLocalHttps => 'Локальный HTTPS';

  @override
  String get pluginsEndpointMethods => 'Разрешённые методы запросов';

  @override
  String get pluginsEndpointNew => 'Добавить конечную точку';

  @override
  String get pluginsEndpointNoCredential => 'Без учётных данных';

  @override
  String get pluginsEndpointOrigin =>
      'Только origin, например https://api.example.com';

  @override
  String get pluginsEndpointPackage => 'Пакет';

  @override
  String get pluginsEndpointPackageUnavailable =>
      'Пакет недоступен или не имеет разрешения HTTP. Существующие разрешения можно отключить.';

  @override
  String get pluginsEndpointProfile => 'Профиль подключения';

  @override
  String get pluginsEndpointPublicHttps => 'Публичный HTTPS';

  @override
  String get pluginsEndpointRemoveCertificate => 'Удалить корневой сертификат';

  @override
  String get pluginsEndpointReplace => 'Заменить разрешение';

  @override
  String get pluginsEndpointReplaceTitle =>
      'Заменить разрешение с текущим хешем пакета';

  @override
  String get pluginsEndpointRequestBytes => 'Максимум байт запроса (1–65536)';

  @override
  String get pluginsEndpointResponseBytes => 'Максимум байт ответа (1–65536)';

  @override
  String get pluginsEndpointSave => 'Сохранить разрешение конечной точки';

  @override
  String get pluginsEndpointSaved =>
      'Разрешение сохранено. Подключение к сети не выполнялось.';

  @override
  String get pluginsEndpointTimeout => 'Тайм-аут (1–30000 мс)';

  @override
  String get pluginsEndpointTitle => 'Разрешения конечных точек API';

  @override
  String get pluginsEndpointUnknown =>
      'Результат не подтверждён. Обновите состояние перед изменениями. Запрос не будет автоматически отправлен повторно.';

  @override
  String get pluginsEndpointWorking => 'Обновление состояния конечной точки…';

  @override
  String get pluginsExistingVersion =>
      'Эта версия уже установлена. Состояние включения не изменилось.';

  @override
  String pluginsFileLimit(int limit) {
    return 'Файл слишком велик. Выберите файл размером не более $limit байт.';
  }

  @override
  String get pluginsHttpTaskAbandon => 'Завершить наблюдение за попыткой';

  @override
  String get pluginsHttpTaskAbandonDetails =>
      'Наблюдение можно завершить, только когда свежее состояние подтверждает отсутствие активной задачи и доступность исходной библиотеки. Это не доказывает отсутствие удалённых изменений. Идентификатор и неопределённость сохраняются в истории; новый запрос требует явной отправки.';

  @override
  String get pluginsHttpTaskAbsent => 'Результат не доставлен';

  @override
  String get pluginsHttpTaskAccepted => 'Принято';

  @override
  String get pluginsHttpTaskAcknowledge => 'Подтвердить завершённую задачу';

  @override
  String get pluginsHttpTaskArchivedUnknown =>
      'Наблюдение завершено пользователем. Предыдущие удалённые изменения не подтверждены; попытка не повторялась.';

  @override
  String get pluginsHttpTaskBase64 => 'Base64';

  @override
  String get pluginsHttpTaskBody => 'Тело запроса';

  @override
  String get pluginsHttpTaskBodyFormat => 'Кодировка тела запроса';

  @override
  String get pluginsHttpTaskBusy => 'Занято';

  @override
  String get pluginsHttpTaskCancel => 'Запросить отмену';

  @override
  String get pluginsHttpTaskCancelled =>
      'Отмена обнаружена; удалённые изменения могли произойти';

  @override
  String get pluginsHttpTaskCatalogUnavailable =>
      'Каталог плагинов недоступен. Обновите его перед новой отправкой; управление существующей задачей доступно.';

  @override
  String get pluginsHttpTaskClosed => 'Закрыто';

  @override
  String get pluginsHttpTaskCompleted => 'Завершено';

  @override
  String get pluginsHttpTaskConflict => 'Конфликт';

  @override
  String get pluginsHttpTaskConsumed => 'Результат потреблён';

  @override
  String get pluginsHttpTaskControlUnknown =>
      'Результат управляющего действия не подтверждён. Обновите состояние задачи перед следующим действием.';

  @override
  String pluginsHttpTaskCounters(String bytes, String calls) {
    return 'Вызовы IO: $calls; учтённые байты: $bytes';
  }

  @override
  String get pluginsHttpTaskDeadline => 'Превышен срок выполнения';

  @override
  String get pluginsHttpTaskDenied => 'Отказано';

  @override
  String get pluginsHttpTaskDetails =>
      'Выполните один явно заданный запрос через разрешённую конечную точку и включённый плагин с экспериментальным HTTP-обработчиком. Состояние задачи доступно, даже когда библиотека занята.';

  @override
  String get pluginsHttpTaskDisconnect => 'Ошибка очистки соединения';

  @override
  String get pluginsHttpTaskEndpoint => 'Разрешённая конечная точка';

  @override
  String get pluginsHttpTaskEndpointsFailed =>
      'Не удалось согласованно прочитать конечные точки, либо библиотека занята. Управление задачами доступно. Обновите конечные точки после возврата библиотеки.';

  @override
  String get pluginsHttpTaskEvidenceUnavailable =>
      'Подтверждение результата недоступно';

  @override
  String pluginsHttpTaskExecution(int code, String fault) {
    return 'Выполнение гостя: $fault; код выхода: $code';
  }

  @override
  String pluginsHttpTaskExit(
    String disconnect,
    String execution,
    String maintenance,
  ) {
    return 'Выход рабочего процесса — выполнение: $execution; отключение: $disconnect; обслуживание: $maintenance';
  }

  @override
  String get pluginsHttpTaskExplicit =>
      'Отправка выполняет один реальный запрос. Каждый щелчок создаёт новый идентификатор. Неподтверждённые отправки и чтения не повторяются автоматически. Отмена не доказывает откат удалённой операции.';

  @override
  String get pluginsHttpTaskFailed => 'Ошибка';

  @override
  String get pluginsHttpTaskHeaders =>
      'Обычные заголовки запроса: Имя: значение, по одному на строку';

  @override
  String get pluginsHttpTaskHeadersHint =>
      'Повторяющиеся заголовки сохраняются отдельно. Заголовки учётных данных и соединения задаёт только среда выполнения.';

  @override
  String get pluginsHttpTaskHistory => 'Предыдущие наблюдения задач (до 5)';

  @override
  String pluginsHttpTaskHttpResult(int code, String status) {
    return 'Результат HTTP: $status; удалённый статус: $code';
  }

  @override
  String get pluginsHttpTaskInactive => 'Соединение неактивно';

  @override
  String get pluginsHttpTaskInvalid =>
      'Проверьте конечную точку, метод, относительный путь, обычные заголовки, кодировку тела и тайм-аут на соответствие разрешённым лимитам.';

  @override
  String get pluginsHttpTaskInvalidOptions => 'Недопустимые параметры';

  @override
  String pluginsHttpTaskKey(String identity) {
    return 'Идентификатор задачи: $identity';
  }

  @override
  String get pluginsHttpTaskLimit => 'Достигнута квота или лимит';

  @override
  String get pluginsHttpTaskLoadingEndpoints =>
      'Чтение разрешённых конечных точек…';

  @override
  String get pluginsHttpTaskLocal => 'Библиотека доступна; активных задач нет';

  @override
  String get pluginsHttpTaskModule => 'Недопустимый гостевой модуль';

  @override
  String get pluginsHttpTaskNew => 'Подготовить новый запрос';

  @override
  String get pluginsHttpTaskNoEndpoints =>
      'Нет конечных точек для включённого и разрешённого плагина пересылки HTTP.';

  @override
  String get pluginsHttpTaskNotFound => 'Не найдено';

  @override
  String get pluginsHttpTaskOk => 'ОК';

  @override
  String get pluginsHttpTaskOutcomeUnknown =>
      'Удалённый результат неизвестен; не считайте изменения отменёнными и не повторяйте отправку';

  @override
  String get pluginsHttpTaskPackageChanged => 'Привязка пакета изменена';

  @override
  String get pluginsHttpTaskPending => 'Ожидание результата';

  @override
  String get pluginsHttpTaskPoll => 'Проверить задачу';

  @override
  String get pluginsHttpTaskProtocol => 'Ошибка протокола задачи';

  @override
  String get pluginsHttpTaskRead => 'Прочитать результат один раз';

  @override
  String get pluginsHttpTaskReadBound => 'Превышен предел чтения';

  @override
  String get pluginsHttpTaskReadPending =>
      'Результат не возвращён. Проверьте состояние перед явным повторным чтением.';

  @override
  String get pluginsHttpTaskReadUnknown =>
      'Чтение результата не подтверждено и могло уже потребить результат. Повторного чтения не будет. Состояние и завершение всё ещё можно проверить.';

  @override
  String get pluginsHttpTaskReady =>
      'Результат готов: прочитайте явно. Это не означает завершение рабочего процесса.';

  @override
  String get pluginsHttpTaskReclaimed =>
      'Рабочий процесс завершён; исходная библиотека возвращена';

  @override
  String get pluginsHttpTaskRecoveryRequired =>
      'Рабочий процесс завершён; очистка или обслуживание требуют исправления';

  @override
  String get pluginsHttpTaskRefresh => 'Обновить состояние задачи';

  @override
  String get pluginsHttpTaskRefreshEndpoints =>
      'Обновить разрешённые конечные точки';

  @override
  String get pluginsHttpTaskRemoteError =>
      'Удалённый сервер вернул 4xx/5xx. HTTP-обмен завершён; это отдельно от ошибок выполнения гостя.';

  @override
  String get pluginsHttpTaskRepair => 'Исправить очистку';

  @override
  String get pluginsHttpTaskResponseBase64 => 'Тело ответа: точный Base64';

  @override
  String get pluginsHttpTaskResponseHeaders =>
      'Заголовки ответа (повторы сохранены; двоичные значения в Base64)';

  @override
  String get pluginsHttpTaskResponseText =>
      'Тело ответа: текстовый предпросмотр';

  @override
  String get pluginsHttpTaskResultUnavailable =>
      'Доставка результата недоступна';

  @override
  String get pluginsHttpTaskRevoked => 'Разрешение отозвано';

  @override
  String get pluginsHttpTaskRunning =>
      'Выполняется; библиотека передана рабочему процессу';

  @override
  String get pluginsHttpTaskSpawn => 'Не удалось запустить рабочий процесс';

  @override
  String get pluginsHttpTaskStart => 'Отправить новый запрос';

  @override
  String get pluginsHttpTaskStartUnknown =>
      'Результат отправки неизвестен. Идентификатор сохранён. Запросите состояние той же задачи; запрос не будет отправлен повторно.';

  @override
  String get pluginsHttpTaskStatusFailed =>
      'Не удалось подтвердить состояние задачи. Обновите состояние; запрос не повторялся.';

  @override
  String get pluginsHttpTaskStopping =>
      'Остановка; ожидание фактического выхода рабочего процесса';

  @override
  String pluginsHttpTaskSubmission(String identity) {
    return 'Идентификатор отправки: $identity';
  }

  @override
  String get pluginsHttpTaskTarget =>
      'Относительный путь, например /v1/items?limit=10';

  @override
  String get pluginsHttpTaskText => 'Текст UTF-8';

  @override
  String get pluginsHttpTaskTimeout =>
      'Тайм-аут в мс (1–30000, в пределах разрешения)';

  @override
  String get pluginsHttpTaskTitle => 'Задачи HTTP';

  @override
  String get pluginsHttpTaskTrap => 'Ловушка при выполнении гостевого кода';

  @override
  String get pluginsHttpTaskUnavailable =>
      'Исходная библиотека недоступна; требуется восстановление';

  @override
  String get pluginsHttpTaskUnsupported => 'Операция не поддерживается';

  @override
  String get pluginsHttpTaskWorking => 'Ожидание ответа управления задачей…';

  @override
  String get pluginsImport => 'Импортировать';

  @override
  String get pluginsImportDetails =>
      'После импорта вы решаете, включать ли плагин. Отключение и удаление сохраняют содержимое.';

  @override
  String pluginsImportPreview(String name) {
    return 'Предпросмотр импорта: $name';
  }

  @override
  String get pluginsImportUnknown => 'Не удалось подтвердить импорт';

  @override
  String get pluginsImportedDisabled =>
      'Импортирован и выключен. Выберите разрешения.';

  @override
  String get pluginsInputFailed => 'Не удалось прочитать входной файл';

  @override
  String get pluginsInputTooLong =>
      'Достигнут предел ввода. Сократите текст и повторите попытку.';

  @override
  String get pluginsInspectFailed =>
      'Не удалось загрузить предпросмотр плагина';

  @override
  String get pluginsInspectedOnly =>
      'Файл только проверен. После импорта включите плагин отдельно.';

  @override
  String get pluginsInsufficientApproval =>
      'Плагин включён, но ему нужен доступ к содержимому. Пространство доступно только для чтения. Отключите плагин, чтобы снова проверить разрешения.';

  @override
  String pluginsIoApproved(String permissions) {
    return 'Разрешено: $permissions';
  }

  @override
  String get pluginsIoCredentialUse =>
      'Использовать разрешённые учётные данные';

  @override
  String pluginsIoDeclared(String permissions) {
    return 'Запрошенные разрешения сети и файлов: $permissions';
  }

  @override
  String get pluginsIoFileCreate => 'Создавать файлы';

  @override
  String get pluginsIoFileDelete => 'Удалять файлы';

  @override
  String get pluginsIoFileList => 'Просматривать разрешённые папки';

  @override
  String get pluginsIoFileRead => 'Читать разрешённые файлы';

  @override
  String get pluginsIoFileReplace => 'Заменять файлы';

  @override
  String get pluginsIoHttpListen => 'Принимать сетевые соединения';

  @override
  String get pluginsIoHttpPublish => 'Предоставлять API-сервис';

  @override
  String get pluginsIoHttpRequest => 'Вызывать сетевые API';

  @override
  String get pluginsIoNoneApproved => 'Разрешения сети и файлов не выданы';

  @override
  String get pluginsIoRevoke => 'Отозвать все разрешения сети и файлов';

  @override
  String get pluginsIoSave => 'Сохранить разрешения сети и файлов';

  @override
  String get pluginsIoScopeNotice =>
      'Здесь сохраняются только категории разрешений. Адреса серверов, доступ к файлам и учётные данные требуют отдельного одобрения; недоступные функции не включаются. После изменений откройте форму плагина заново.';

  @override
  String get pluginsIoTitle => 'Разрешения сети и файлов';

  @override
  String get pluginsIoWebSocketConnect => 'Подключаться к WebSocket-сервисам';

  @override
  String get pluginsListUnknown => 'Не удалось подтвердить список плагинов';

  @override
  String get pluginsManageAbove =>
      'Управляйте этим плагином с помощью элементов выше.';

  @override
  String get pluginsManagementUnavailable =>
      'Управление плагинами недоступно. Существующее содержимое можно читать.';

  @override
  String get pluginsNoPermissions => 'Разрешения для содержимого не заявлены.';

  @override
  String get pluginsOpenTextTool => 'Открыть текстовый инструмент';

  @override
  String get pluginsOpenView => 'Открыть представление';

  @override
  String get pluginsOpeningView => 'Открытие представления плагина…';

  @override
  String get pluginsOperation => 'Запрашивать результаты операций';

  @override
  String pluginsOtherCapability(String name) {
    return 'Другое заявленное разрешение: $name';
  }

  @override
  String get pluginsPackageFile => 'Плагин Morrow';

  @override
  String get pluginsPreviewOnly =>
      'Только предпросмотр. Результаты не сохраняются автоматически в существующее содержимое.';

  @override
  String get pluginsPreviewTruncated => '…показаны только первые 4096 символов';

  @override
  String get pluginsProtection => 'Защита содержимого';

  @override
  String get pluginsProtectionDetails =>
      'Сохраните исходный файл защиты для восстановления с этой системной учётной записью. Карточек и вложений в нём нет.';

  @override
  String get pluginsProtectionFileType => 'Файл защиты библиотеки';

  @override
  String get pluginsProtectionSaved =>
      'Файл защиты скопирован. Выберите его для восстановления при сбое запуска.';

  @override
  String get pluginsRead => 'Читать содержимое';

  @override
  String get pluginsReadingState => 'Загрузка состояния плагина…';

  @override
  String pluginsRefreshFailed(String reason) {
    return '$reason. Не удалось обновить список. Нажмите «Обновить список» для повторного чтения.';
  }

  @override
  String get pluginsRefreshList => 'Обновить список';

  @override
  String get pluginsRefreshState => 'Обновить состояние';

  @override
  String get pluginsRename => 'Переименовать';

  @override
  String pluginsResultBytes(int count, String preview) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count байта',
      many: '$count байт',
      few: '$count байта',
      one: '$count байт',
    );
    return '$_temp0\n$preview';
  }

  @override
  String get pluginsSavePermissions => 'Сохранить разрешения';

  @override
  String pluginsSelectedFile(String name) {
    return 'Выбран файл: $name';
  }

  @override
  String get pluginsServiceAcknowledgeUncertain =>
      'Я проверил обновлённые записи';

  @override
  String get pluginsServiceAddScope => 'Добавить область содержимого';

  @override
  String get pluginsServiceAttachmentId => 'Точный идентификатор вложения';

  @override
  String get pluginsServiceAuthenticationUnavailable =>
      'Выбранная аутентификация отсутствует, отключена, просрочена или принадлежит другому субъекту. Черновые области сохранены; выберите замену или явно удалите её.';

  @override
  String get pluginsServiceAuthorities => 'Записи аутентификации и публикации';

  @override
  String get pluginsServiceCardId => 'Точный идентификатор карточки';

  @override
  String get pluginsServiceCatalogChanged =>
      'Каталог пакетов изменился или недоступен. Черновик сохранён. Явно обновите выбор перед сохранением.';

  @override
  String get pluginsServiceClearToken => 'Очистить токен';

  @override
  String get pluginsServiceCloseEditor => 'Закрыть редактор';

  @override
  String get pluginsServiceConfigDigest => 'Хеш конфигурации';

  @override
  String get pluginsServiceConfiguration => 'Сохранённая конфигурация';

  @override
  String get pluginsServiceConfigurations => 'Сохранённые конфигурации';

  @override
  String get pluginsServiceCopyClear => 'Скопировать и очистить токен';

  @override
  String get pluginsServiceCreated => 'Создано (UTC)';

  @override
  String get pluginsServiceDays => 'Запрошенный срок (1–30 дней)';

  @override
  String get pluginsServiceDigestFixed =>
      'Редактирование сохраняет исходный хеш пакета. Выберите совпадающий пакет; это не включает его.';

  @override
  String get pluginsServiceDisable => 'Отключить';

  @override
  String get pluginsServiceDisabled => 'Отключено';

  @override
  String get pluginsServiceEditConfig => 'Изменить конфигурацию';

  @override
  String get pluginsServiceEditPublication => 'Изменить публикацию';

  @override
  String get pluginsServiceExpired => 'Срок истёк или ещё не наступил';

  @override
  String get pluginsServiceExpires => 'Фактический срок (UTC)';

  @override
  String get pluginsServiceHandler => 'Заявленный обработчик сервиса';

  @override
  String get pluginsServiceIdentity => 'Идентификатор сервиса';

  @override
  String get pluginsServiceInvalid =>
      'Проверьте поля, выбранные разрешения и текущий пакет перед сохранением.';

  @override
  String get pluginsServiceIssue => 'Выпустить токен';

  @override
  String get pluginsServiceIssuedToken =>
      'Однократно показываемый Bearer-токен';

  @override
  String get pluginsServiceListenAddress =>
      'Числовой адрес и порт прослушивания';

  @override
  String get pluginsServiceLoadFailed =>
      'Не удалось обновить записи. Повторите обновление перед изменениями.';

  @override
  String get pluginsServiceManagementOnly =>
      'Здесь управляются сохранённые настройки и разрешения. Сохранение не запускает слушатель, пакет или сервис.';

  @override
  String get pluginsServiceMethod => 'Метод HTTP';

  @override
  String get pluginsServiceNewAuthentication => 'Новая аутентификация';

  @override
  String get pluginsServiceNewConfig => 'Новая конфигурация';

  @override
  String get pluginsServiceNo => 'Нет';

  @override
  String get pluginsServiceNoAuthentication =>
      'Сначала создайте действующую запись аутентификации.';

  @override
  String get pluginsServiceNoAuthorities =>
      'Нет записей аутентификации или публикации.';

  @override
  String get pluginsServiceNoConfigurations => 'Нет конфигураций сервиса.';

  @override
  String get pluginsServicePackage => 'Заявленный и разрешённый пакет';

  @override
  String get pluginsServicePackageDigest => 'Хеш пакета';

  @override
  String get pluginsServicePackageUnavailable =>
      'Подходящий пакет или его разрешения прослушивания/публикации недоступны. Исторические записи можно читать и отключать.';

  @override
  String get pluginsServicePath => 'Точный путь запроса';

  @override
  String get pluginsServicePolicyChanged =>
      'Выбранная исходная запись изменилась или недоступна. Обновите выбор либо откройте редактор из текущей записи. Черновик сохранён.';

  @override
  String get pluginsServicePrincipalId => 'Идентификатор субъекта';

  @override
  String get pluginsServicePrincipals =>
      'Разрешённые субъекты и области содержимого';

  @override
  String get pluginsServicePublicationEditor => 'Разрешение публикации';

  @override
  String get pluginsServicePublicationHelp =>
      'Разрешение привязано к точной конфигурации, ревизии и ссылке. Срок ограничен всеми выбранными записями аутентификации и может быть короче запрошенного. Сохранение не запускает прослушивание.';

  @override
  String get pluginsServicePublicationMismatch =>
      'Публикация больше не соответствует текущей конфигурации. Проверьте и явно сохраните новое разрешение.';

  @override
  String get pluginsServiceQueryPath =>
      'Отдельный путь запроса результата (необязательно)';

  @override
  String get pluginsServiceReference => 'Ссылка на разрешение';

  @override
  String get pluginsServiceRefresh => 'Обновить записи';

  @override
  String get pluginsServiceRefreshSelection => 'Обновить выбор';

  @override
  String get pluginsServiceRemovePrincipal => 'Удалить субъект';

  @override
  String get pluginsServiceRemoveScope => 'Удалить область';

  @override
  String get pluginsServiceRetention =>
      'Хранение истории запросов (мс, до 30 дней)';

  @override
  String get pluginsServiceRevision => 'Ревизия';

  @override
  String get pluginsServiceRotate => 'Заменить токен';

  @override
  String get pluginsServiceRotateAuthentication => 'Заменить аутентификацию';

  @override
  String get pluginsServiceRunAbandon => 'Сохранить запись и завершить попытку';

  @override
  String get pluginsServiceRunAdvanced => 'Лимиты запросов и рабочего процесса';

  @override
  String get pluginsServiceRunAttempt => 'Неподтверждённая попытка запуска';

  @override
  String get pluginsServiceRunBoundsHint =>
      'Лимиты также должны соответствовать декларации плагина и сохранённым разрешениям. Зарезервированная работа расходует общий бюджет даже при отмене. По истечении срока запуск останавливается; автопродления нет.';

  @override
  String get pluginsServiceRunBytes => 'Лимит байт запуска (до 67 108 864)';

  @override
  String get pluginsServiceRunCalls => 'Вызовов на задачу (до 1024)';

  @override
  String get pluginsServiceRunCancelled => 'Отменено';

  @override
  String get pluginsServiceRunClosed => 'Закрыто';

  @override
  String get pluginsServiceRunConcurrent => 'Параллельные задачи (до 128)';

  @override
  String get pluginsServiceRunControlUnknown =>
      'Результат управления неизвестен. Обновите состояние исходного сервиса перед другой операцией.';

  @override
  String get pluginsServiceRunDenied => 'Отказано';

  @override
  String get pluginsServiceRunExited => 'Сервис завершён';

  @override
  String get pluginsServiceRunHeaderBytes =>
      'Максимальный размер заголовков (байт, до 65 536)';

  @override
  String get pluginsServiceRunHint =>
      'Выберите разрешённую публикацию и конечные лимиты, затем явно запустите сервис. После остановки дождитесь возврата исходного владельца пространства перед подтверждением результата.';

  @override
  String pluginsServiceRunHostFailure(String detail) {
    return 'Диагностика хоста: $detail';
  }

  @override
  String get pluginsServiceRunHttpPanel =>
      'Этой задачей владеет API-сервис. Остановите его или подтвердите выход в панели сервиса выше. Черновик HTTP-запроса сохранён.';

  @override
  String get pluginsServiceRunIdentityChanged =>
      'Содержимым владеет другая задача. Панель не будет управлять ею с прежним идентификатором сервиса.';

  @override
  String get pluginsServiceRunInvalid =>
      'Проверьте выбранный сервис и числовые лимиты. Новый запуск не отправлен.';

  @override
  String get pluginsServiceRunInvalidOutcome => 'Недопустимая конфигурация';

  @override
  String get pluginsServiceRunJobBytes => 'Байт на задачу (до 16 777 216)';

  @override
  String get pluginsServiceRunJobs =>
      'Всего резервирований задач (до 1 000 000)';

  @override
  String get pluginsServiceRunLastObservation =>
      'Показано последнее наблюдение; текущее состояние не проверено.';

  @override
  String get pluginsServiceRunLifetime => 'Длительность (мс, до 3 600 000)';

  @override
  String get pluginsServiceRunLimit => 'Лимит достигнут';

  @override
  String get pluginsServiceRunLocal => 'Содержимое доступно локально';

  @override
  String pluginsServiceRunNetwork(
    String bind,
    String listener,
    String supervision,
  ) {
    return 'Привязка: $bind; слушатель: $listener; надзор: $supervision';
  }

  @override
  String get pluginsServiceRunNextSettings =>
      'Настройки следующего явного запуска';

  @override
  String get pluginsServiceRunNoSelection =>
      'Нет доступных разрешённых публикаций. Проверьте плагин, конфигурацию и аутентификацию.';

  @override
  String get pluginsServiceRunOutboundAttempt =>
      'Конечные точки этой попытки запуска';

  @override
  String get pluginsServiceRunOutboundClear => 'Очистить выбор конечных точек';

  @override
  String get pluginsServiceRunOutboundFailed =>
      'Не удалось проверить список конечных точек. Обновите перед использованием выбранных.';

  @override
  String get pluginsServiceRunOutboundHint =>
      'Исходящие API (необязательно, до 8). Показаны только разрешённые для пакета конечные точки. Если ничего не выбрано, исходящие вызовы запрещены.';

  @override
  String get pluginsServiceRunOutboundStale =>
      'Выбранная конечная точка изменилась или недоступна. Явно выберите текущую версию или очистите выбор.';

  @override
  String get pluginsServiceRunOwned => 'Содержимым управляет работающий сервис';

  @override
  String get pluginsServiceRunPending => 'Ожидание';

  @override
  String get pluginsServiceRunReclaimed =>
      'Владение содержимым возвращено; требуется подтверждение';

  @override
  String get pluginsServiceRunReclaiming =>
      'Ожидание возврата владения содержимым';

  @override
  String get pluginsServiceRunRecovery => 'Очистка требует исправления';

  @override
  String get pluginsServiceRunRequestBytes =>
      'Максимальный размер запроса (байт)';

  @override
  String get pluginsServiceRunResponseBytes =>
      'Максимальный размер ответа (байт)';

  @override
  String get pluginsServiceRunRunning => 'Сервис работает';

  @override
  String get pluginsServiceRunSelection => 'Разрешённая публикация сервиса';

  @override
  String get pluginsServiceRunStale =>
      'Выбранный пакет, конфигурация или разрешение изменились. Обновите записи и выберите снова перед запуском.';

  @override
  String get pluginsServiceRunStart => 'Запустить ограниченный сервис';

  @override
  String get pluginsServiceRunStartRejected =>
      'Ответ запуска сообщил об ошибке. Текущая задача проверена; изучите причину и состояние очистки перед продолжением.';

  @override
  String get pluginsServiceRunStartUnknown =>
      'Результат запуска неизвестен. Идентификатор попытки сохранён; обновите, чтобы найти её. Автоматического повторного запуска не будет.';

  @override
  String get pluginsServiceRunStarting => 'Запуск сервиса';

  @override
  String get pluginsServiceRunStatusFailed =>
      'Не удалось проверить состояние сервиса. Обновите перед следующими действиями.';

  @override
  String get pluginsServiceRunStop => 'Остановить сервис';

  @override
  String get pluginsServiceRunStopping =>
      'Остановка; ожидание выхода слушателя и рабочего процесса';

  @override
  String get pluginsServiceRunSucceeded => 'Успешно';

  @override
  String get pluginsServiceRunTask => 'Идентификатор текущей задачи';

  @override
  String get pluginsServiceRunTimeout => 'Тайм-аут задачи (мс, до 30 000)';

  @override
  String get pluginsServiceRunTimeoutOutcome => 'Тайм-аут';

  @override
  String get pluginsServiceRunTitle => 'Запуск API-сервиса';

  @override
  String get pluginsServiceRunTotalBytes =>
      'Лимит байт рабочего процесса (до 67 108 864)';

  @override
  String get pluginsServiceRunTransport => 'Ошибка транспорта';

  @override
  String get pluginsServiceRunUnavailable => 'Хранилище содержимого недоступно';

  @override
  String get pluginsServiceSaveConfig => 'Сохранить конфигурацию';

  @override
  String get pluginsServiceSavePublication => 'Сохранить разрешение публикации';

  @override
  String get pluginsServiceSaved =>
      'Сохранено. Проверьте возвращённую ревизию и фактический срок ниже.';

  @override
  String get pluginsServiceScopeAttachment => 'Читать вложение';

  @override
  String get pluginsServiceScopeCreate => 'Создавать содержимое';

  @override
  String get pluginsServiceScopeEdit => 'Изменять содержимое';

  @override
  String get pluginsServiceScopeKind => 'Разрешённая операция с содержимым';

  @override
  String get pluginsServiceScopeQuery => 'Запросить операцию';

  @override
  String get pluginsServiceScopeRead => 'Читать содержимое';

  @override
  String get pluginsServiceScopeRename => 'Переименовать карточку';

  @override
  String get pluginsServiceScopeSummary => 'Читать сводку';

  @override
  String get pluginsServiceScopesHelp =>
      'Явно выберите аутентификацию. Добавьте разрешённые операции и точные идентификаторы объектов ниже. Для удаления области или субъекта используйте отдельную кнопку; при редактировании существующие области сохраняются.';

  @override
  String get pluginsServiceTitle => 'Настройка сервиса';

  @override
  String get pluginsServiceTls => 'Требовать TLS';

  @override
  String get pluginsServiceTlsAttempt =>
      'Хеш PEM-сертификата, привязанный к попытке запуска';

  @override
  String get pluginsServiceTlsCertificate => 'Выбрать цепочку сертификатов';

  @override
  String get pluginsServiceTlsChecked =>
      'Сертификат и соответствие ключа проверены. SHA-256 PEM показан ниже. Клиенты всё равно должны проверять имя хоста, срок и цепочку доверия.';

  @override
  String get pluginsServiceTlsChecking => 'Обработка выбранного сертификата…';

  @override
  String get pluginsServiceTlsFailed =>
      'Проверка сертификата не удалась. Проверьте PEM-файлы, соответствие ключа и локальные пути.';

  @override
  String get pluginsServiceTlsHelp =>
      'Адреса вне loopback требуют TLS. Здесь сохраняется только требование; слушатель и TLS-идентичность не создаются.';

  @override
  String get pluginsServiceTlsHint =>
      'Выберите цепочку сертификатов PEM и закрытый ключ, затем проверьте их. При запуске файлы проверяются снова; активный сертификат не заменяется автоматически.';

  @override
  String get pluginsServiceTlsInspect => 'Проверить сертификат';

  @override
  String get pluginsServiceTlsOutsideValidity =>
      'Цепочка сертификатов ещё не действует или просрочена. Проверьте или замените сертификат и повторите проверку перед запуском.';

  @override
  String get pluginsServiceTlsPrivateKey => 'Выбрать закрытый ключ';

  @override
  String get pluginsServiceTlsRecheck =>
      'Проверьте сертификат ещё раз перед запуском. Изменение часов не восстанавливает предыдущий выбор.';

  @override
  String get pluginsServiceTlsUnavailable =>
      'Этот бэкенд не поддерживает выбор локального TLS-сертификата.';

  @override
  String pluginsServiceTlsValidity(String end, String start) {
    return 'Общий срок цепочки сертификатов (UTC): с $start по $end. После истечения сервис остановится.';
  }

  @override
  String get pluginsServiceTokenDiscarded =>
      'Одноразовый токен очищен при закрытии панели. При необходимости явно выпустите новый.';

  @override
  String get pluginsServiceTokenHelp =>
      'Токен показывается только сейчас. При необходимости скопируйте его явно. Очистка или закрытие панели удалит его из сеанса; получить его из списка нельзя. Замена отменяет предыдущий токен.';

  @override
  String get pluginsServiceUncertainHelp =>
      'Сначала обновите и проверьте исходные записи. Подтверждение уведомления лишь разрешает следующее явное действие; оно не доказывает сбой предыдущего изменения и не повторяет его.';

  @override
  String get pluginsServiceUnsupported =>
      'Неподдерживаемое историческое значение';

  @override
  String get pluginsServiceWorking => 'Выполнение…';

  @override
  String get pluginsServiceWriteUnknown =>
      'Результат последнего изменения неизвестен. Оно не отправлялось повторно.';

  @override
  String get pluginsServiceYes => 'Да';

  @override
  String get pluginsSettingsUnknown =>
      'Настройка ещё не подтверждена. Обновите состояние перед повторным выбором.';

  @override
  String get pluginsSnapshotDetails =>
      'Копия библиотеки включает карточки, вложения и записи аудита. Внешние ресурсы остаются ссылками. Для восстановления нужна исходная системная учётная запись.';

  @override
  String get pluginsSnapshotSaved =>
      'Библиотека скопирована, включая вложения и исходный файл защиты.';

  @override
  String get pluginsStateUnavailable =>
      'Не удалось загрузить состояние плагина. Повторите попытку.';

  @override
  String get pluginsSummary => 'Читать сводки';

  @override
  String get pluginsTextInput => 'Исходный текст';

  @override
  String get pluginsThirdParty => 'Сторонние плагины';

  @override
  String get pluginsTlsIdentitiesDisable => 'Отключить идентичность';

  @override
  String get pluginsTlsIdentitiesEmpty =>
      'В библиотеке нет сохранённых идентичностей.';

  @override
  String get pluginsTlsIdentitiesFileMode =>
      'Следующий запуск: проверенные локальные файлы.';

  @override
  String get pluginsTlsIdentitiesHint =>
      'Выберите идентичность явно. Замена или отключение останавливает использующие её сервисы; новый запуск всегда выполняется явно.';

  @override
  String get pluginsTlsIdentitiesImport =>
      'Подготовить сертификаты для импорта или замены';

  @override
  String get pluginsTlsIdentitiesReplace => 'Заменить проверенными файлами';

  @override
  String get pluginsTlsIdentitiesSave => 'Сохранить как новую идентичность';

  @override
  String get pluginsTlsIdentitiesSaved =>
      'Сохранено. Проверьте идентичность и ревизию ниже, затем выберите для нового запуска.';

  @override
  String get pluginsTlsIdentitiesSavedMode =>
      'Следующий запуск: сохранённая идентичность. Хост проверит срок сертификата при запуске.';

  @override
  String get pluginsTlsIdentitiesSelect => 'Использовать при следующем запуске';

  @override
  String get pluginsTlsIdentitiesStale =>
      'Выбранная идентичность изменилась, отключена или не обновлена. Выберите актуальную снова.';

  @override
  String get pluginsTlsIdentitiesTitle => 'Сохранённые TLS-идентичности';

  @override
  String get pluginsTlsIdentitiesUnknownHint =>
      'Обновите и проверьте записи перед подтверждением. Отсутствие квитанции не означает сбой; не создавайте повторно без проверки.';

  @override
  String get pluginsTlsIdentitiesUseFile =>
      'Использовать проверенные файлы при следующем запуске';

  @override
  String get pluginsTransform => 'Преобразовать';

  @override
  String get pluginsTransformUnknown => 'Не удалось подтвердить преобразование';

  @override
  String get pluginsUiExecution =>
      'Выполнение плагина не завершилось. Откройте представление заново и повторите попытку.';

  @override
  String get pluginsUiRejected =>
      'Действие плагина не принято. Проверьте ввод и текущие разрешения.';

  @override
  String get pluginsUiUnavailable =>
      'Плагин недоступен. Проверьте состояние и откройте представление заново.';

  @override
  String get pluginsUnavailableView => 'Представление плагина недоступно';

  @override
  String pluginsUnconfirmed(String reason) {
    return '$reason. Операция не подтверждена. Проверьте обновлённое состояние перед повторным выбором.';
  }

  @override
  String get pluginsUninstallKeepContent => 'Удалить (сохранить содержимое)';

  @override
  String get pluginsUninstallUnknown => 'Не удалось подтвердить удаление';

  @override
  String get pluginsUninstalled => 'Удалён. Существующее содержимое сохранено.';

  @override
  String get pluginsUpdatingView => 'Обновление предпросмотра…';

  @override
  String get pluginsUseText => 'Использовать текст';

  @override
  String get pluginsUseTransform => 'Использовать преобразование';

  @override
  String get pluginsViewFailed => 'Не удалось открыть представление плагина';

  @override
  String get pluginsWorkbench => 'Плагин рабочего пространства';

  @override
  String get pluginsWorkbenchReadOnly =>
      'Плагин разрешён, но пространство доступно только для чтения. Устраните проблему библиотеки или плагина и обновите состояние.';

  @override
  String get recoveryAllFiles => 'Все файлы';

  @override
  String get recoveryBackupExists =>
      'Файл резервной копии уже существует. Выберите новое имя.';

  @override
  String get recoveryBackupFile => 'Резервная копия библиотеки';

  @override
  String get recoveryBackupUnknown =>
      'Результат копирования требует проверки. Сохраните текущий файл и проверьте место сохранения.';

  @override
  String get recoveryBindingMissing =>
      'У библиотеки нет привязанного файла защиты. Выбранный файл невозможно сопоставить.';

  @override
  String get recoveryBusy =>
      'Библиотека занята другим процессом. Закройте другое окно и повторите.';

  @override
  String get recoveryChooseKey => 'Выбрать файл восстановления';

  @override
  String get recoveryCloseFirst =>
      'Рабочая область ещё открыта. Закройте её перед сменой библиотеки.';

  @override
  String get recoveryFailed =>
      'Восстановление не завершено. Сохраните исходные файлы и повторите.';

  @override
  String get recoveryIdentityBusy =>
      'Другая копия библиотеки уже открыта. Закройте её перед открытием этой.';

  @override
  String get recoveryIdentityMismatch =>
      'Идентификатор библиотеки не совпадает с регистрацией. Сохраните исходные данные и восстановите правильную копию.';

  @override
  String get recoveryKeyFile => 'Файл защиты библиотеки';

  @override
  String get recoveryKeyGuide =>
      'Если файл защиты отсутствует или повреждён, выберите его копию. Она должна принадлежать этой библиотеке; нужна исходная системная учётная запись.';

  @override
  String get recoveryKeyMismatch =>
      'Ключ не подходит или не расшифровывается. Используйте исходный файл и системную учётную запись.';

  @override
  String get recoveryKeyUnknown =>
      'Результат восстановления требует проверки. Попробуйте открыть заново; прежний файл защиты, если был, сохранён в копии.';

  @override
  String get recoveryLibraryInvalid =>
      'Не удалось проверить или открыть библиотеку. Сохраните исходную библиотеку и ключ защиты, затем повторите.';

  @override
  String get recoveryMaintenance =>
      'Библиотеке требуется обслуживание. Сохраните исходные файлы и изучите диагностику.';

  @override
  String get recoveryMissingKey =>
      'Ключ защиты библиотеки отсутствует. Восстановите исходный .audit-key и повторите.';

  @override
  String get recoveryMissingLibrary =>
      'Ключ есть, но библиотека отсутствует или пуста. Восстановите исходную библиотеку.';

  @override
  String get recoveryOpenFailed =>
      'Рабочая область не открылась. Проверьте файлы плагинов и папку данных и повторите.';

  @override
  String get recoveryPluginUnavailable =>
      'Плагин рабочей области недоступен. Имеющиеся данные можно просматривать и экспортировать.';

  @override
  String get recoveryRegistryInvalid =>
      'Регистрация активной библиотеки повреждена или не поддерживается. Открытие остановлено для защиты данных.';

  @override
  String get recoveryRegistryUnreadable =>
      'Не удалось прочитать активную библиотеку или её регистрацию. Проверьте исходное расположение; новая библиотека автоматически не создаётся.';

  @override
  String get recoveryRetry => 'Повторить';

  @override
  String get recoverySnapshot => 'Восстановить резервную копию библиотеки';

  @override
  String get recoverySnapshotGuide =>
      'Резервную копию библиотеки можно восстановить в новую папку и переключиться на неё. Исходная папка сохранится. Будет восстановлено состояние на момент копирования; нужна исходная системная учётная запись.';

  @override
  String get recoverySnapshotInvalid =>
      'Неверный формат копии или ошибка целостности. Сохраните исходный файл копии.';

  @override
  String get recoverySnapshotUnknown =>
      'Результат восстановления требует проверки. Проверьте папку назначения; исходная библиотека не заменена.';

  @override
  String recoverySwitchUnconfirmed(String path) {
    return 'Копия восстановлена в $path, но переключение не подтверждено. Сохраните папку и откройте рабочую область для проверки.';
  }

  @override
  String get recoverySwitchUnknown =>
      'Переключение библиотеки не подтверждено. Откройте рабочую область заново для проверки.';

  @override
  String get recoveryTargetExists =>
      'Папка восстановления уже существует. Выберите новую, ещё не существующую папку.';

  @override
  String get recoveryTitle => 'Открыть рабочую область заново';

  @override
  String get visualApplyColor => 'Применить цвет';

  @override
  String get visualApplyComponent => 'Применить к этому компоненту';

  @override
  String get visualApplyTexture => 'Применить медиа';

  @override
  String visualAttachmentDetails(String action, String extension, String size) {
    return '$extension · $size · $action';
  }

  @override
  String get visualAttachmentFailure =>
      'Ошибка работы с файлом. Проверьте файл и свободное место.';

  @override
  String get visualAttachmentPreview => 'Предпросмотр локального вложения';

  @override
  String get visualAttachmentReadFailure =>
      'Не удалось прочитать вложение. Импортируйте заново.';

  @override
  String get visualAudio => 'Аудио';

  @override
  String get visualAudioStateFailure =>
      'Не удалось подтвердить состояние аудио. Повторите.';

  @override
  String get visualAutoLyrics =>
      'Автоматически искать отсутствующий текст песни онлайн';

  @override
  String get visualCancel => 'Отмена';

  @override
  String get visualChangeCover => 'Изменить обложку';

  @override
  String get visualChooseAudio =>
      'Выберите аудиофайл или одноимённый файл LRC.';

  @override
  String get visualChooseLyrics => 'Выберите файл текста песни LRC или TXT.';

  @override
  String get visualClickPreview => 'Нажмите для просмотра';

  @override
  String get visualClose => 'Закрыть';

  @override
  String get visualCloseDialog => 'Закрыть диалог';

  @override
  String get visualCloseWindow => 'Закрыть окно';

  @override
  String get visualCollapsePlaylist => 'Свернуть плейлист';

  @override
  String get visualColorGuide =>
      'Перетащите круг, чтобы выбрать тон и насыщенность, затем настройте яркость. Можно также ввести значение цвета.';

  @override
  String get visualColorTitle => 'Добавьте цвет в своё пространство';

  @override
  String visualComponentCompass(String title) {
    return '$title · Цветовой круг';
  }

  @override
  String get visualComponents => 'Компоненты и карточки';

  @override
  String get visualComponentsGuide =>
      'По умолчанию элементы следуют теме. Настройка одной карточки не изменит остальные.';

  @override
  String get visualCornerTips1 =>
      'Не каждая идея должна быть полезной.\nНекоторые просто делают день интереснее.';

  @override
  String get visualCornerTips2 =>
      'Запишите и дайте вырасти.\nИдея не обязана сразу быть законченной.';

  @override
  String get visualCornerTips3 =>
      'Оставьте немного места для себя.\nЛюбопытству нужен простор.';

  @override
  String get visualCornerTips4 =>
      'Попробуйте сегодня что-то новое.\nМаленький обход может удивить.';

  @override
  String get visualCornerTips5 =>
      'Мечты тоже куда-то ведут.\nДайте мыслям побродить.';

  @override
  String get visualCornerTips6 =>
      'Находите время для любимого.\nНе нужно доказывать его ценность.';

  @override
  String get visualCornerTips7 =>
      'Прогресс бывает небольшим.\nГотовность начать уже важна.';

  @override
  String get visualCornerTips8 =>
      'Иногда смотрите в окно.\nЖизнь тоже вдохновляет.';

  @override
  String get visualCover => 'Обложка';

  @override
  String get visualCustomCompass => 'Цветовой круг · Настройка';

  @override
  String get visualCustomMaterialGuide =>
      'Выключите, чтобы следовать теме, сохранив свои настройки этого элемента.';

  @override
  String get visualDefaultOpen => 'Открыть приложением по умолчанию';

  @override
  String get visualDownloadOpen => 'Скачать для открытия';

  @override
  String get visualEmbeddedLyrics => 'Встроено в аудио';

  @override
  String get visualExpandPlaylist => 'Развернуть плейлист';

  @override
  String get visualFile => 'Файл';

  @override
  String get visualFileOpenFailure =>
      'Не удалось открыть файл. Установите совместимое приложение или сохраните вложение и откройте его там.';

  @override
  String get visualFileRetry => 'Ошибка работы с файлом. Повторите.';

  @override
  String get visualFindLyrics => 'Найти текст песни';

  @override
  String get visualFindLyricsGuide =>
      'Ищите в LRCLIB по песне и исполнителю, затем выберите нужную версию.';

  @override
  String get visualFollowTheme => 'По теме';

  @override
  String get visualFooterLyrics => 'Показывать текст песни внизу';

  @override
  String get visualFooterTips => 'Показывать подсказки внизу';

  @override
  String get visualFooterTips1 =>
      'Ничего срочного. Дайте любопытству немного времени.';

  @override
  String get visualFooterTips10 =>
      'Не нужно занимать каждую минуту. Оставьте немного свободы.';

  @override
  String get visualFooterTips2 => 'Запишите мысль. Разобрать можно позже.';

  @override
  String get visualFooterTips3 =>
      'Превратите большую идею в один маленький шаг на сегодня.';

  @override
  String get visualFooterTips4 => 'Потянитесь и дайте глазам отдохнуть.';

  @override
  String get visualFooterTips5 => 'Идея пока может оставаться без ответа.';

  @override
  String get visualFooterTips6 =>
      'Некоторые открытия приходят, когда не спешишь.';

  @override
  String get visualFooterTips7 => 'Сохранённая деталь помогает идее вырасти.';

  @override
  String get visualFooterTips8 =>
      'Сегодняшняя заметка может стать завтрашним началом.';

  @override
  String get visualFooterTips9 =>
      'Дайте мыслям побродить, затем вернитесь к любимому.';

  @override
  String get visualFrosting => 'Размытие';

  @override
  String get visualGif => 'Анимированный GIF';

  @override
  String get visualHexColor => 'Цвет HEX';

  @override
  String get visualHexInvalid => 'Введите шестизначный шестнадцатеричный цвет.';

  @override
  String get visualImage => 'Изображение';

  @override
  String get visualImageDecodeFailure =>
      'Не удалось декодировать изображение. Сохраните и откройте в другом приложении.';

  @override
  String visualImageLoadFailure(String name) {
    return 'Не удалось загрузить изображение: $name';
  }

  @override
  String visualImageNotImported(String name) {
    return '$name (изображение не импортировано)';
  }

  @override
  String visualImageUnavailable(String name) {
    return 'Изображение недоступно: $name';
  }

  @override
  String get visualImportFailure =>
      'Ошибка импорта. Проверьте файл, кодировку и свободное место.';

  @override
  String get visualImportLyrics => 'Импортировать текст песни';

  @override
  String get visualImportMusic => 'Импортировать музыку';

  @override
  String get visualImportMusicHint =>
      'Нажмите +, чтобы добавить песни с устройства';

  @override
  String get visualIndependentMaterial => 'Свой материал';

  @override
  String get visualInheritColor => 'Использовать цвет темы';

  @override
  String get visualLinkFailure =>
      'Не удалось открыть ссылку. Скопируйте адрес и повторите.';

  @override
  String visualLoadImage(String name) {
    return 'Загрузить изображение · $name';
  }

  @override
  String get visualLoading => 'Загрузка…';

  @override
  String get visualLyricsEmpty => 'Файл текста песни пуст.';

  @override
  String get visualLyricsFile => 'Файл текста песни';

  @override
  String get visualLyricsImportHint =>
      'Импортируйте файл текста песни или найдите онлайн.';

  @override
  String get visualLyricsLoading => 'Загрузка текста песни…';

  @override
  String visualLyricsMatch(String album, String kind, int seconds) {
    String _temp0 = intl.Intl.pluralLogic(
      seconds,
      locale: localeName,
      other: '$seconds секунды',
      many: '$seconds секунд',
      few: '$seconds секунды',
      one: '$seconds секунда',
    );
    return '$album\n$kind · $_temp0';
  }

  @override
  String get visualLyricsMissing =>
      'Текст песни не найден. Импортируйте файл или повторите поиск.';

  @override
  String get visualLyricsNotFound =>
      'Текст песни не найден. Попробуйте другое название или исполнителя.';

  @override
  String get visualLyricsOnPlay => 'Загружать текст песни при воспроизведении';

  @override
  String get visualLyricsParseFailure =>
      'Не удалось разобрать текст песни. Импортируйте заново.';

  @override
  String get visualLyricsReadFailure =>
      'Не удалось загрузить текст песни. Импортируйте вручную или повторите.';

  @override
  String get visualLyricsServiceFailure =>
      'Не удалось подключиться к службе текстов песен. Повторите позже или импортируйте файл.';

  @override
  String get visualLyricsSize => 'Размер файла текста песни — до 1 МБ.';

  @override
  String get visualLyricsSources => 'Файл → Встроенный текст → LRCLIB';

  @override
  String get visualLyricsVersions =>
      'Найдено несколько версий. Выберите нужную в поиске.';

  @override
  String get visualMaterialPreview => 'Предпросмотр материала';

  @override
  String get visualMaximize => 'Развернуть';

  @override
  String get visualMediaAddress => 'Адрес медиа';

  @override
  String get visualMediaAddressInvalid =>
      'Введите корректный HTTP- или HTTPS-адрес без данных входа.';

  @override
  String get visualMediaPreviewFailure =>
      'Предпросмотр недоступен. Сохраните медиа и откройте в другом приложении.';

  @override
  String get visualMediaType => 'Тип медиа';

  @override
  String get visualMinimize => 'Свернуть';

  @override
  String get visualMusic => 'Музыка';

  @override
  String get visualMusicEmptyTitle => 'Оставьте место для музыки';

  @override
  String get visualMusicPlayer => 'Музыкальный плеер';

  @override
  String get visualNextTrack => 'Следующая песня';

  @override
  String get visualNoLyricsRead => 'Текст песни не загружен';

  @override
  String visualNoLyricsTitle(String title) {
    return '♪ $title · Нет текста песни';
  }

  @override
  String get visualNoTimeline => 'Нет временных меток';

  @override
  String get visualOpacity => 'Непрозрачность';

  @override
  String get visualOptionalArtist => 'Исполнитель (необязательно)';

  @override
  String get visualPauseMusic => 'Приостановить музыку';

  @override
  String get visualPaused => 'На паузе';

  @override
  String get visualPlainLyrics => 'Обычный текст песни';

  @override
  String get visualPlayMusic => 'Включить музыку';

  @override
  String get visualPlaybackFailure =>
      'Песня не воспроизводится. Проверьте файл или попробуйте другой аудиоформат.';

  @override
  String visualPlaybackPosition(int count, int index, String state) {
    return '$index / $count · $state';
  }

  @override
  String get visualPlaybackRequestFailure =>
      'Не удалось начать воспроизведение. Повторите.';

  @override
  String get visualPlaying => 'Воспроизводится';

  @override
  String get visualPlaylistEmpty => 'Плейлист пуст';

  @override
  String get visualPlaylistLyricsHint =>
      'Импортируйте LRC через меню плейлиста';

  @override
  String get visualPlaylistSaved =>
      'Плейлист и текст песен сохраняются автоматически';

  @override
  String get visualPlaylistUpdateFailure =>
      'Не удалось обновить плейлист. Повторите.';

  @override
  String get visualPreviewColor => 'Предпросмотр цвета';

  @override
  String get visualPreviousTrack => 'Предыдущая песня';

  @override
  String get visualRemoveAttachment => 'Удалить вложение';

  @override
  String get visualRemoveTrack => 'Удалить из плейлиста';

  @override
  String get visualResetMaterial => 'Вернуть тему';

  @override
  String get visualRestoreWindow => 'Восстановить';

  @override
  String get visualSaveAttachment => 'Сохранить вложение как';

  @override
  String get visualSearch => 'Поиск';

  @override
  String get visualSearchLyrics => 'Поиск текста песни';

  @override
  String get visualSongCover => 'Обложка альбома';

  @override
  String get visualSongTitle => 'Название песни';

  @override
  String get visualSyncedLyrics => 'Синхронизированный текст';

  @override
  String get visualTextureFailure =>
      'Не удалось загрузить медиа. Проверьте файл, адрес или формат. Для веб-медиа также нужен разрешённый доступ между источниками.';

  @override
  String get visualTextureLinkGuide =>
      'Вставьте прямую HTTP- или HTTPS-ссылку на изображение, GIF или видео. Для страниц сначала найдите адрес исходного медиа.';

  @override
  String get visualTextureLinkTitle => 'Добавьте вдохновения';

  @override
  String get visualTexturePlaybackGuide =>
      'Видео повторяется без звука; звук включается в настройках. Онлайн-медиа должны разрешать доступ, включая междоменную загрузку в веб-версии.';

  @override
  String get visualTipsMaterialGuide =>
      'Выключено — прозрачные подсказки; включено — материал ниже. Свои значения сохраняются.';

  @override
  String get visualTransparentTips => 'Прозрачное наложение (по умолчанию)';

  @override
  String get visualUseCustomMaterial => 'Использовать свой материал';

  @override
  String get visualVideo => 'Видео';

  @override
  String get visualViewLyrics => 'Просмотр текста песни';
}
