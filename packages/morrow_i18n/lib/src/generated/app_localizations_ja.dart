// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Japanese (`ja`).
class AppLocalizationsJa extends AppLocalizations {
  AppLocalizationsJa([String locale = 'ja']) : super(locale);

  @override
  String get commonAppName => 'Morrow';

  @override
  String get commonCancel => 'キャンセル';

  @override
  String commonCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count 件',
      zero: '項目なし',
    );
    return '$_temp0';
  }

  @override
  String commonGreeting(String name) {
    return 'こんにちは、$nameさん';
  }

  @override
  String get importsAttachmentLimit => '一度に取り込める添付は 20 個までです。残りは分けて貼り付けてください。';

  @override
  String get importsClipboardChanged => '読み取り中にクリップボードが変わりました。再度貼り付けてください。';

  @override
  String get importsEmbeddedImageUnreadable => '埋め込み画像を読み取れませんでした。';

  @override
  String get importsEmbeddedImagesSeparate => '一部の埋め込み画像は別ファイルとして読み込む必要があります。';

  @override
  String get importsExcelValues =>
      '値と数式を Markdown に変換しました。書式と結合セルは XML 添付に保持しています。';

  @override
  String get importsExcelXmlKept => '元の Excel 表は XML 添付として保持しました。';

  @override
  String get importsFileTooLarge => 'クリップボードのファイルが 200 MB を超えています。';

  @override
  String get importsItemLimit => '最初の 20 項目のみ読み取りました。残りは分けて貼り付けてください。';

  @override
  String get importsItemUnreadable =>
      'クリップボードの 1 項目を読み取れませんでした。他の読み取り可能な内容は保持しました。';

  @override
  String get importsLocalImageNotRead =>
      'ローカルのリンク画像は自動で読み取りません。画像を貼り付けるか、元のファイルを読み込んでください。';

  @override
  String get importsMergedTable => '結合セルを読みやすい表に変換しました。元の書式は HTML 添付に保持しています。';

  @override
  String get importsOfficeBusy =>
      '別のアプリがクリップボードを使用中です。Office オブジェクトは読み取っていません。';

  @override
  String get importsOfficeEmbeddedKept =>
      '埋め込み Office オブジェクトは元の添付として保持します。グラフ、数式、レイアウトは元のアプリで編集してください。';

  @override
  String get importsOfficeExportFailed =>
      'Office オブジェクトが上限を超えているか、書き出せませんでした。元のアプリで保存してから読み込んでください。';

  @override
  String get importsOfficeReadFailed =>
      'Office の内容を読み取れませんでした。クリップボードの他の内容は利用できます。';

  @override
  String get importsOfficeUnavailable => 'Office のクリップボードは一時的に利用できません。';

  @override
  String get importsOfficeUnreadable =>
      '元の Office オブジェクトを読み取れませんでした。他の利用可能な内容は保持しました。';

  @override
  String get importsRichFallback => '一部の書式を変換できませんでした。読めるテキストは保持しました。';

  @override
  String get importsRichTooLarge => '書式付きテキストが 2 MB を超えています。文書を添付として読み込んでください。';

  @override
  String get importsRtfTooLarge => 'RTF の内容が大きすぎます。元の文書を読み込んでください。';

  @override
  String get importsSpreadsheetTooLarge => '表が大きすぎます。Excel ファイルを読み込んでください。';

  @override
  String get importsTableConverted =>
      '表を Markdown に変換しました。完全なデータは TSV 添付に保持しています。';

  @override
  String get importsTextTooLarge => 'テキストが 2 MB を超えています。ファイルとして読み込んでください。';

  @override
  String get importsTotalTooLarge =>
      '貼り付けるファイルの合計が 200 MB を超えています。小分けにして読み込んでください。';

  @override
  String get importsUnsupported => 'ここではクリップボードを利用できません。ファイルを読み込んでください。';

  @override
  String get mainActiveProjects => '進行中';

  @override
  String get mainAdjustCustomTone => '色調を調整';

  @override
  String get mainAmbientDetail => '流れる光が、ほのかな彩りを添えます。';

  @override
  String get mainAppTitle => 'Morrow — アイデアの居場所';

  @override
  String get mainAppearance => '外観';

  @override
  String get mainArrangeIdeas => 'アイデアを並べ替え';

  @override
  String mainAttachmentCount(int count) {
    return '添付 · $count';
  }

  @override
  String mainAttachmentHint(int count, String name) {
    return '添付 $count 件 · $name';
  }

  @override
  String get mainAttachmentLimit => '1 件の記録につき添付は 20 個までです。';

  @override
  String get mainAutosaveNotice => '外観とアイデアはローカルに保存されます';

  @override
  String get mainAwaitDiscovery => '新しい発見を待っています';

  @override
  String get mainBackToWorkbench => 'ワークスペースに戻る';

  @override
  String get mainBackgroundCanvas => '背景キャンバス';

  @override
  String get mainBackgroundSound => '背景の音声を再生';

  @override
  String get mainBodyHint => '思いつきを書いたり、内容を貼り付けたり…\n\n# 見出し、リスト、表、コードブロックに対応';

  @override
  String get mainBrightWhite => '白';

  @override
  String get mainBuiltinTexture => '内蔵テクスチャを使用';

  @override
  String get mainCanvasCompass => 'キャンバスのカラーホイール';

  @override
  String get mainCaptureIdea => 'アイデアを記録';

  @override
  String get mainCaptureNow => '思いつきを記録';

  @override
  String mainCardAttachments(int count, String name) {
    return 'ファイル $count 件 · $name';
  }

  @override
  String get mainCategoryExperiment => '実験';

  @override
  String get mainCategoryIdea => 'アイデア';

  @override
  String get mainCategoryProject => 'プロジェクト';

  @override
  String get mainCategoryPrompt => '保存先';

  @override
  String get mainChangeFailed => '変更を保存できませんでした。下書きは保持されています。再試行できます。';

  @override
  String get mainCheckAgain => 'もう一度確認';

  @override
  String get mainClearSearch => '検索をクリア';

  @override
  String get mainClipboardEmpty =>
      'クリップボードに読み取り可能なテキストやファイルがありません。ファイルをコピーするか、ファイルの読み込みを利用してください。';

  @override
  String get mainClipboardReadFailed =>
      '内容を読み取れませんでした。ファイルを取り込むか、ファイルとクリップボードの権限を確認してください。';

  @override
  String get mainClipboardSupport =>
      'Markdown、Office の書式付きテキストと表、スクリーンショット、ファイルに対応。複雑なオブジェクトは元の添付として保持します。最大 20 個、各 200 MB。';

  @override
  String get mainCollapseSidebar => 'サイドバーを折りたたむ';

  @override
  String get mainCompletedProjects => '完了';

  @override
  String get mainComponentCompass => 'コンポーネントのカラーホイール';

  @override
  String get mainComponentEmpty => '空の状態';

  @override
  String get mainComponentFooter => 'フッターのヒントと歌詞';

  @override
  String get mainComponentHero => '概要カード';

  @override
  String get mainComponentNavigation => 'サイドバーのナビゲーション';

  @override
  String get mainComponentQuickCapture => 'クイックメモ';

  @override
  String get mainComponentSearch => '検索バー';

  @override
  String get mainComponentSettings => 'コンポーネントとカード · 設定';

  @override
  String get mainContentProtection => 'コンテンツの保護';

  @override
  String get mainContentRead => '内容を読み取りました';

  @override
  String mainContentReadFiles(int count) {
    return '内容を読み取り、添付 $count 件を保持しました';
  }

  @override
  String get mainCornerRadius => '角の丸み';

  @override
  String get mainCredentialSettings => '認証情報';

  @override
  String get mainCrystal => 'クリア';

  @override
  String get mainCrystalDetail => '軽やかに透き通り、色を引き立てます。';

  @override
  String get mainCuriosity => '面白いことは、\n小さな好奇心から。';

  @override
  String get mainCustomCompass => 'カラーホイール · カスタム';

  @override
  String get mainCustomLightness => '明るさを調整';

  @override
  String get mainCustomTheme => 'カスタム';

  @override
  String get mainDaily => '日々の小さなこと';

  @override
  String get mainDailyExplore => '10 分だけ探求しよう';

  @override
  String get mainDailyIdea => 'アイデアを書き留めよう';

  @override
  String get mainDailyWater => '水を一杯飲もう';

  @override
  String get mainDarkTheme => 'ダーク';

  @override
  String get mainDeepBlack => '黒';

  @override
  String get mainDefaultCanvas => '標準';

  @override
  String get mainDefaultGlobalColor => '標準テーマカラー · すべての操作部品';

  @override
  String get mainDelete => '削除';

  @override
  String mainDeleted(String title) {
    return '「$title」を削除しました';
  }

  @override
  String get mainDiagnosticDetails => '診断の詳細';

  @override
  String get mainDone => '完了';

  @override
  String get mainEdit => '編集';

  @override
  String get mainEditIdeaTitle => 'アイデアをより明確に';

  @override
  String get mainEditorClosedUnknown =>
      'エディターを閉じましたが、保存結果は未確認です。別のコピーを作る前にワークスペースを開き直して確認してください。';

  @override
  String get mainEditorSubtitle => 'テキスト、表、画像。アイデアが形になるまで、ここに。';

  @override
  String get mainEditorUnavailable => 'エディターを利用できません。コンテンツサービスを確認して再試行してください。';

  @override
  String get mainEndpointSettings => '送信先エンドポイント';

  @override
  String get mainExpandSettings => '設定を展開';

  @override
  String get mainExpandSidebar => 'サイドバーを展開';

  @override
  String get mainExtensionPlugins => '拡張機能';

  @override
  String get mainFavoriteAttachments => 'お気に入りの添付';

  @override
  String get mainFavoriteRecords => 'お気に入りの記録';

  @override
  String mainFavoriteTooltip(String title) {
    return '「$title」をお気に入りに追加';
  }

  @override
  String get mainFavoritesIntro => 'お気に入りのテキスト、画像、ファイルをひとまとめに。';

  @override
  String mainFieldLimit(int limit) {
    return '最大 $limit 文字です。短くするか、ファイルとして読み込んでください。';
  }

  @override
  String get mainFilterAll => 'すべて';

  @override
  String get mainFilterAttachments => 'ファイルあり';

  @override
  String get mainFilterFavorites => 'お気に入りのみ';

  @override
  String get mainFilterFile => 'ファイル';

  @override
  String get mainFilterImage => '画像';

  @override
  String get mainFilterMedia => '音声 / 動画';

  @override
  String get mainFilterPending => '未完了';

  @override
  String get mainFilterText => 'テキスト';

  @override
  String get mainFollowTheme => 'テーマに合わせる';

  @override
  String get mainFrostDetail => '背景をやわらげ、思考に余白を。';

  @override
  String get mainFrostEffect => 'ぼかし';

  @override
  String get mainFrostOpacity => 'ガラスの不透明度';

  @override
  String get mainFrostUnavailable => 'デスクトップのぼかしを利用できません。色と不透明度は調整できます。';

  @override
  String get mainFrosted => 'すりガラス';

  @override
  String get mainGlassTexture => 'ガラスの質感';

  @override
  String mainGlobalColor(String color) {
    return '$color · すべての操作部品';
  }

  @override
  String get mainGreeting => 'アイデアを育てよう。';

  @override
  String get mainGreetingDetail => '日々の気づきも、ひらめきの瞬間も。';

  @override
  String get mainHeroBody => '思いつき、小さな用事、「もしも」。\nここから始めよう。';

  @override
  String get mainHeroCaption => '可能性の片隅';

  @override
  String get mainHeroTitle => '小さな思いつきからで大丈夫。';

  @override
  String get mainHideAppearance => '外観設定を隠す';

  @override
  String get mainHideCustomTone => 'カスタム色調を隠す';

  @override
  String get mainHidePreview => 'プレビューを隠す';

  @override
  String get mainHttpSettings => 'HTTP タスク';

  @override
  String get mainHypothesis => '仮説';

  @override
  String get mainHypothesisPrompt => '検証する仮説';

  @override
  String get mainHypothesisSection => '仮説 / 試したいこと';

  @override
  String get mainIdeaDetails => '細部を記録して、次の一歩を明確に。';

  @override
  String get mainIdeaNameHint => '名前を付けましょう';

  @override
  String get mainIdeaNameRequired => 'まずアイデアを入力してください';

  @override
  String get mainIdeaSaved => 'アイデアを保存しました。';

  @override
  String get mainImportFailed => 'メディアを読み込めませんでした。ファイルと空き容量を確認してください。';

  @override
  String get mainImportFile => 'ファイルを読み込む';

  @override
  String get mainInboxIntro => 'まず記録、整理は後で。有望なアイデアを小さなプロジェクトに。';

  @override
  String get mainIoNoDeclarations => 'ファイルやネットワークへのアクセスを宣言するプラグインはありません。';

  @override
  String get mainIoSettings => 'ネットワークとファイル';

  @override
  String get mainIoSettingsGuide =>
      '外観とは別にプラグインのファイル・ネットワーク権限を管理します。機能を承認しても、すべてのファイルや接続先へのアクセスは許可されません。利用可能な操作は現在のバックエンドによります。';

  @override
  String get mainIoSettingsSummary => '権限、認証情報、エンドポイント、API サービス';

  @override
  String get mainJustNow => 'たった今';

  @override
  String get mainLabIntro => '仮説から始めて、試行、観察、意外な発見を残しましょう。';

  @override
  String get mainLanguage => '言語';

  @override
  String get mainLanguageChinese => '简体中文';

  @override
  String get mainLanguageEnglish => 'English';

  @override
  String get mainLanguageSystem => 'システムに合わせる';

  @override
  String get mainLavender => 'ラベンダー';

  @override
  String get mainLightOpacity => '20% · 淡い';

  @override
  String get mainLiquidAllCanvases => '4 種類すべての背景で個別に使用できます';

  @override
  String get mainLiquidDetail => '浮かぶ水滴のような、流れる光とやわらかな屈折。';

  @override
  String get mainLiquidEffect => 'リキッドガラス効果';

  @override
  String get mainLiquidGlass => 'リキッドガラス';

  @override
  String get mainLivePreview => 'ライブプレビュー';

  @override
  String get mainLocalMedia => 'ローカルメディア';

  @override
  String get mainMakeYours => '自分らしく';

  @override
  String get mainMarkOrganized => '整理済みにする';

  @override
  String get mainMarkdownBody => '本文 · Markdown';

  @override
  String get mainMediaLimits => '画像 / GIF は 25 MB 以下、動画は 150 MB 以下';

  @override
  String get mainMonochrome => 'モノクロ';

  @override
  String mainMoreSteps(int count) {
    return 'ほか $count ステップ。開いて表示';
  }

  @override
  String mainMovedProject(String title) {
    return '「$title」をプロジェクトに移動しました';
  }

  @override
  String get mainMusic => '音楽プレーヤー';

  @override
  String get mainMySpace => '自分のスペース';

  @override
  String get mainNavigation => 'ナビゲーション';

  @override
  String get mainNewIdea => '新しいアイデア';

  @override
  String get mainNewIdeaTitle => '新しいひらめきをつかまえる';

  @override
  String get mainNoHypothesis => '仮説はまだありません';

  @override
  String get mainNoMatches => '一致するアイデアはありません';

  @override
  String get mainNoResultYet => '結果は後でも大丈夫。過程も記録する価値があります。';

  @override
  String get mainNotNow => '今はしない';

  @override
  String get mainObservationSection => '観察 / 分かったこと';

  @override
  String get mainObservations => '観察と結論';

  @override
  String get mainObservationsPrompt => '観察、過程、結論';

  @override
  String get mainOneHourAgo => '1 時間前';

  @override
  String get mainOnlineMedia => 'オンラインメディア';

  @override
  String get mainOpaqueFallback => '現在のテーマカラーに透明なパネルを重ねます。';

  @override
  String get mainOpenNextStep => 'プロジェクトを開いて次のステップを編集';

  @override
  String get mainOrganizedCount => '整理済み';

  @override
  String get mainOriginalColors => '元の色';

  @override
  String get mainPageFavorites => 'お気に入り';

  @override
  String get mainPageInbox => '受信箱';

  @override
  String get mainPageLaboratory => '実験室';

  @override
  String get mainPageOverview => '概要';

  @override
  String get mainPageProjects => 'プロジェクト';

  @override
  String mainPageSummary(String page) {
    return '$page · 概要';
  }

  @override
  String get mainPasteChanged => '貼り付け中に入力内容が変わりました。エディターを開き直してください。';

  @override
  String get mainPasteContent => '内容を貼り付け';

  @override
  String get mainPause => '一時停止';

  @override
  String get mainPersonalWorkspace => '個人用ワークスペース';

  @override
  String get mainPlay => '再生';

  @override
  String get mainPluginSettings => 'プラグインとサービス';

  @override
  String get mainPluginSettingsSummary => '内蔵ツール、拡張機能、ネットワーク、ファイル、コンテンツ保護';

  @override
  String get mainPreviewEmpty => 'ここにプレビューが表示されます';

  @override
  String mainProgress(int done, int total) {
    return '小さな一歩 · $done/$total';
  }

  @override
  String get mainProjectIntro => 'チェックリストで少しずつ前進。一歩ずつ、目標に近づきます。';

  @override
  String get mainQueryAgain => '新しく検索';

  @override
  String get mainQueryCapacity => '検索履歴がいっぱいです';

  @override
  String get mainQueryCapacityDetail => '内容は保持されています。このバージョンでは検索履歴を消去できません。';

  @override
  String get mainQueryLoading => 'アイデアを検索中…';

  @override
  String get mainQueryRetry => '検索を再試行';

  @override
  String get mainQueryTerminated => 'この検索は終了しました';

  @override
  String get mainQueryUnknown => '結果はまだ確定していません';

  @override
  String get mainQuickHint => '今、何を思いつきましたか？';

  @override
  String get mainReadOnlySettings =>
      '内容は読み取り専用です。ワークスペースプラグインを確認して編集を復旧してください。';

  @override
  String get mainRecentThoughts => '最近のアイデア';

  @override
  String get mainRecordedCount => '観察記録あり';

  @override
  String get mainRestoreDefault => '既定に戻す';

  @override
  String get mainRetry => '再試行';

  @override
  String get mainRetrySave => '保存を再試行';

  @override
  String get mainSage => 'セージ';

  @override
  String get mainSampleBody0 => 'ふと思いついたことをここに。\n完成を急がず、まずは始めよう。';

  @override
  String get mainSampleBody1 => '好きな言葉や音楽、\n日常の小さなことを集めるページ。';

  @override
  String get mainSampleBody2 => 'ジェネラティブアートに挑戦。\nコードから意外な形を生み出そう。';

  @override
  String get mainSampleBody3 => '忘れがちな小さなことを\nそっと覚えてくれる相棒。';

  @override
  String get mainSampleTitle0 => 'アイデアの居場所';

  @override
  String get mainSampleTitle1 => '静かなデジタルガーデン';

  @override
  String get mainSampleTitle2 => '楽しむために何か作ろう';

  @override
  String get mainSampleTitle3 => '小さなデスクトップの相棒';

  @override
  String get mainSampleTodo0 => '最初のコレクションを整理';

  @override
  String get mainSampleTodo1 => 'ガーデンの入り口をデザイン';

  @override
  String get mainSampleTodo2 => '新しいアイデアの種をまく';

  @override
  String get mainSampleTodo3 => '小さな試作品を描く';

  @override
  String get mainSampleTodo4 => 'リマインダーの操作を考える';

  @override
  String get mainSaveConnectionUnknown =>
      '接続が切れ、保存結果は不明です。再試行する前にライブラリを開き直して確認してください。';

  @override
  String get mainSaveFailed => '保存できませんでした。変更はこのセッション内に保持されています。';

  @override
  String get mainSaveIdea => 'アイデアを保存';

  @override
  String get mainSaveNotSubmitted => '未送信です。下書きと添付ファイルは保持されています。編集して再保存できます。';

  @override
  String get mainSaveReadOnly =>
      '変更は保存されていません。「プラグインとサービス」でワークスペースプラグインを有効にして再試行してください。';

  @override
  String get mainSaveUnknown =>
      '保存結果は未確認です。下書きと添付は保持されています。この送信を再試行してください。閉じるとワークスペースを更新して確認します。';

  @override
  String get mainSaving => '保存中…';

  @override
  String get mainSearchHint => 'アイデアを検索…';

  @override
  String get mainServiceRunSettings => 'サービスの実行';

  @override
  String get mainServiceSettings => 'API サービス';

  @override
  String get mainSettings => '設定';

  @override
  String get mainShowAppearance => '外観設定を表示';

  @override
  String get mainSidebarMotto => '少しの整理。ひらめく余白。';

  @override
  String get mainSlowProgress => '小さな一歩も、前進です。';

  @override
  String get mainSolidCanvas => '単色';

  @override
  String get mainSolidDetail => '落ち着いた単色の背景。';

  @override
  String get mainSolidOpacity => '100% · 不透明';

  @override
  String get mainSortFavorites => 'お気に入り優先';

  @override
  String get mainSortRecent => '最近追加した順';

  @override
  String get mainSortTitle => 'タイトル順';

  @override
  String get mainSquareCorners => '0 で四角い角になります';

  @override
  String get mainStageActive => '進行中';

  @override
  String get mainStageCompleted => '完了';

  @override
  String get mainStageOrganized => '整理済み';

  @override
  String get mainStagePlanned => '計画済み';

  @override
  String get mainStageRecorded => '記録済み';

  @override
  String mainStageTooltip(String title) {
    return '「$title」の段階を変更';
  }

  @override
  String get mainStageUnsorted => '未整理';

  @override
  String get mainStageUnverified => '未検証';

  @override
  String get mainStageVerifying => '検証中';

  @override
  String get mainStayCurious => '好奇心を忘れず、自分らしく。';

  @override
  String mainSteps(int done, int total) {
    return '$done/$total ステップ';
  }

  @override
  String get mainStorageUnavailable => 'ローカル保存領域を利用できません。変更はこのセッション中のみ保持されます。';

  @override
  String get mainStorageUnreadable => '保存済みの内容を読み取れませんでした。元のデータは保持され、上書きされません。';

  @override
  String get mainTenMinutesAgo => '10 分前';

  @override
  String get mainTextureCanvas => 'テクスチャ';

  @override
  String get mainTextureDetail => '繊細な紙のような質感を添えます。';

  @override
  String get mainThemeCompass => 'テーマのカラーホイール';

  @override
  String get mainThemeGrayscale => 'テーマのグレースケール';

  @override
  String get mainThemeTone => 'テーマカラー';

  @override
  String get mainThreeHoursAgo => '3 時間前';

  @override
  String get mainTintOpacity => '色の不透明度';

  @override
  String get mainToProject => 'プロジェクトに移動';

  @override
  String get mainTodosPrompt => '次のステップ（1 行に 1 件、省略可）';

  @override
  String get mainTransparencyUnavailable => 'システムの透過を有効にできませんでした。標準の背景を利用できます。';

  @override
  String get mainTransparentCanvas => '透明';

  @override
  String get mainTransparentDetail => 'ウィンドウの背後を表示します。Web 版ではホストの背景を表示します。';

  @override
  String get mainUndo => '元に戻す';

  @override
  String mainUnfavoriteTooltip(String title) {
    return '「$title」をお気に入りから削除';
  }

  @override
  String get mainUnsortedCount => '未整理';

  @override
  String get mainUnverifiedCount => '未検証';

  @override
  String get mainView => '表示';

  @override
  String get mainViewAll => 'すべて表示';

  @override
  String get mainWarmSand => 'ウォームサンド';

  @override
  String get mainWhiteTheme => 'ライト';

  @override
  String get mainWindowRadius => 'ウィンドウの角';

  @override
  String get mainWindowRadiusDetail => 'ウィンドウの縁を個別に調整。最大化すると角は四角くなります';

  @override
  String get mainWindowsFrostOnly => 'デスクトップのぼかしは Windows のみ対応';

  @override
  String get mainWorkbench => 'ワークスペース';

  @override
  String get mainWorkbenchPlugin => 'ワークスペースプラグイン';

  @override
  String get mainWriteHypothesis => '記録を開いて、検証したいことを書きましょう。';

  @override
  String get mainYesterday => '昨日';

  @override
  String get pluginsApprovalUnknown => '有効化または権限の変更を確認できませんでした';

  @override
  String get pluginsApproveEnable => '承認して有効化';

  @override
  String get pluginsApproveWorkbench => '閲覧・編集を許可して有効化';

  @override
  String get pluginsAttachment => '添付ファイルを閲覧';

  @override
  String get pluginsBackingUp => 'バックアップ中…';

  @override
  String get pluginsBackupLibrary => 'ライブラリをバックアップ';

  @override
  String get pluginsBackupLibraryType => 'ライブラリのバックアップ';

  @override
  String get pluginsBackupProtection => '保護ファイルをバックアップ';

  @override
  String get pluginsBackupUnknown =>
      'バックアップはまだ確認されていません。作成されたファイルを保持し、保存先を確認してください。';

  @override
  String pluginsBinaryPreview(String hex) {
    return 'バイナリ内容：$hex';
  }

  @override
  String get pluginsBuiltin => '組み込みワークスペース';

  @override
  String pluginsBuiltinCount(int count) {
    return '$count 文字 · このセッションのみ。カードとしては保存されません';
  }

  @override
  String get pluginsBuiltinEmpty => 'テキストを入力して大文字変換をプレビュー';

  @override
  String get pluginsBuiltinHeading => 'テキストツール';

  @override
  String get pluginsBuiltinInput => 'テキストを入力';

  @override
  String get pluginsCancel => 'キャンセル';

  @override
  String get pluginsChoosePackage => 'プラグインファイルを選択';

  @override
  String get pluginsChooseSmallFile => '小さなファイルを選択';

  @override
  String get pluginsCloseTextTool => 'テキストツールを隠す';

  @override
  String get pluginsCloseUnknown => 'ビューを閉じたことを確認できませんでした';

  @override
  String get pluginsCloseView => 'ビューを閉じる';

  @override
  String get pluginsConnectionLost => '接続が中断されました。プラグインのビューを開き直してください。';

  @override
  String get pluginsContentPermissions => 'コンテンツの権限';

  @override
  String get pluginsCreate => 'コンテンツを作成';

  @override
  String get pluginsCredentialCancel => 'フォームを閉じる';

  @override
  String get pluginsCredentialCreateTitle => '新しい認証情報';

  @override
  String pluginsCredentialDays(int days) {
    return '$days 日';
  }

  @override
  String get pluginsCredentialDetails =>
      '承認された API 接続用の認証情報を安全に保存します。保存してもサーバーの承認やプラグインの有効化は行いません。保存済みのシークレットは表示できません。';

  @override
  String get pluginsCredentialDisable => '無効化';

  @override
  String get pluginsCredentialDisabled => '無効';

  @override
  String get pluginsCredentialDisabledDone => '認証情報を無効にしました。';

  @override
  String get pluginsCredentialEmpty => '保存済みの認証情報はありません';

  @override
  String get pluginsCredentialExpired => '期限切れ';

  @override
  String pluginsCredentialExpires(String date) {
    return '有効期限：$date';
  }

  @override
  String get pluginsCredentialHeader => 'ヘッダー名';

  @override
  String get pluginsCredentialInvalid =>
      'ヘッダー名を確認し、新しいシークレットを入力してください。シークレット欄は消去されました。';

  @override
  String get pluginsCredentialLifetime => '有効期間';

  @override
  String get pluginsCredentialLoadFailed =>
      '認証情報を整合した状態で読み取れませんでした。状態を更新して再試行してください。';

  @override
  String get pluginsCredentialNew => '認証情報を追加';

  @override
  String get pluginsCredentialReading => '認証情報を読み込み中…';

  @override
  String pluginsCredentialReference(String reference) {
    return '認証情報 $reference';
  }

  @override
  String get pluginsCredentialRefresh => '状態を更新';

  @override
  String get pluginsCredentialReplace => 'シークレットを置換';

  @override
  String pluginsCredentialReplaceTitle(String reference) {
    return '認証情報 $reference を置換';
  }

  @override
  String get pluginsCredentialSave => '認証情報を保存';

  @override
  String get pluginsCredentialSaved => '認証情報を保存しました。API 接続には別途承認が必要です。';

  @override
  String get pluginsCredentialSecret => '新しいシークレット値';

  @override
  String get pluginsCredentialStored => '保存済み';

  @override
  String get pluginsCredentialTitle => 'API 認証情報';

  @override
  String get pluginsCredentialUnknown =>
      '結果を確認できませんでした。シークレット欄は消去されました。状態を更新してから次の変更を行ってください。';

  @override
  String pluginsDeclared(String permissions) {
    return '宣言された権限：$permissions';
  }

  @override
  String get pluginsDependenciesNotice =>
      '依存関係はホストで設定する必要があります。このページでは承認されません。';

  @override
  String get pluginsDisable => '無効化';

  @override
  String get pluginsDisableWorkbench => 'ワークスペースプラグインを無効化';

  @override
  String get pluginsDisabled => '無効';

  @override
  String get pluginsDisabledDetails =>
      '無効です。エディターとツールを使うには、コンテンツの閲覧と編集を許可してください。';

  @override
  String get pluginsEdit => 'コンテンツを編集';

  @override
  String get pluginsEmptyLibrary => 'サードパーティ製プラグインはまだありません。';

  @override
  String get pluginsEmptyResult => '（空の結果）';

  @override
  String get pluginsEnabled => '有効';

  @override
  String get pluginsEnabledDetails =>
      '有効です。このプラグインはコンテンツを閲覧・編集できます。無効にしてもデータは保持されます。';

  @override
  String get pluginsEndpointAdvanced => 'ポリシーの上限（特記なき場合はバイト）';

  @override
  String get pluginsEndpointCertificate => 'DER 信頼ルートを選択';

  @override
  String get pluginsEndpointCertificateDetails =>
      '任意の HTTPS 信頼ルート：32 KiB 以下のバイナリ DER 証明書（.der または .cer）1 個。PEM や証明書バンドルは使用できません。HTTP に切り替える前に信頼ルートを削除してください。';

  @override
  String get pluginsEndpointCertificateInvalid =>
      '32 KiB 以下の有効なバイナリ DER 証明書（.der または .cer）を 1 個選択してください。';

  @override
  String pluginsEndpointCertificateSelected(int bytes) {
    return 'DER 信頼ルートを選択済み（$bytes バイト）';
  }

  @override
  String get pluginsEndpointConcurrency => '同時リクエスト数（1～128）';

  @override
  String get pluginsEndpointCreateTitle => '新しいエンドポイント承認';

  @override
  String get pluginsEndpointCredential => '認証情報の参照';

  @override
  String get pluginsEndpointCredentialLifetime =>
      '認証情報はエンドポイントの有効期間全体を通じて有効である必要があります。有効期限は延長されません。';

  @override
  String get pluginsEndpointCredentialUnavailable =>
      'パッケージに宣言・承認された認証情報使用権限と、有効な保存済み参照が必要です。';

  @override
  String get pluginsEndpointCredentialsFailed =>
      '認証情報の参照を読み取れませんでした。状態を更新してから選択してください。';

  @override
  String get pluginsEndpointDetails =>
      '特定のパッケージとダイジェストに対するサーバーポリシーを保存します。保存してもネットワーク接続やプラグインの有効化は行わず、ネットワークタスクが直ちに利用可能になることもありません。';

  @override
  String get pluginsEndpointDigest => 'パッケージダイジェスト';

  @override
  String get pluginsEndpointDisabledDone => 'エンドポイント承認を無効にしました。';

  @override
  String get pluginsEndpointEmpty => '保存済みのエンドポイント承認はありません';

  @override
  String get pluginsEndpointFrameBytes => 'フレーム上限（1～131072 バイト）';

  @override
  String get pluginsEndpointHeaderBytes => 'ヘッダー最大サイズ（1～16384 バイト）';

  @override
  String get pluginsEndpointInvalid =>
      'パッケージ、オリジン、メソッド、1～30 日の有効期間、認証情報の権限、証明書、ポリシーの上限を確認してください。';

  @override
  String get pluginsEndpointLifetime => '有効期間（1～30 日）';

  @override
  String get pluginsEndpointLoadFailed =>
      'エンドポイント承認を整合した状態で読み取れませんでした。状態を更新して再試行してください。';

  @override
  String get pluginsEndpointLocalHttp => 'ローカル HTTP';

  @override
  String get pluginsEndpointLocalHttps => 'ローカル HTTPS';

  @override
  String get pluginsEndpointMethods => '許可するリクエストメソッド';

  @override
  String get pluginsEndpointNew => 'エンドポイントを追加';

  @override
  String get pluginsEndpointNoCredential => '認証情報なし';

  @override
  String get pluginsEndpointOrigin => 'オリジンのみ（例：https://api.example.com）';

  @override
  String get pluginsEndpointPackage => 'パッケージ';

  @override
  String get pluginsEndpointPackageUnavailable =>
      'パッケージが利用不可か、HTTP 権限が未承認です。既存の承認は無効にできます。';

  @override
  String get pluginsEndpointProfile => '接続プロファイル';

  @override
  String get pluginsEndpointPublicHttps => 'パブリック HTTPS';

  @override
  String get pluginsEndpointRemoveCertificate => '信頼ルートを削除';

  @override
  String get pluginsEndpointReplace => '承認を置換';

  @override
  String get pluginsEndpointReplaceTitle => '現在のパッケージダイジェストで承認を置換';

  @override
  String get pluginsEndpointRequestBytes => 'リクエスト最大サイズ（1～65536 バイト）';

  @override
  String get pluginsEndpointResponseBytes => 'レスポンス最大サイズ（1～65536 バイト）';

  @override
  String get pluginsEndpointSave => 'エンドポイント承認を保存';

  @override
  String get pluginsEndpointSaved => 'エンドポイント承認を保存しました。ネットワーク接続は行っていません。';

  @override
  String get pluginsEndpointTimeout => 'タイムアウト（1～30000 ミリ秒）';

  @override
  String get pluginsEndpointTitle => 'API エンドポイントの承認';

  @override
  String get pluginsEndpointUnknown =>
      '結果を確認できませんでした。次の変更前に状態を更新してください。リクエストは自動再送されません。';

  @override
  String get pluginsEndpointWorking => 'エンドポイントの状態を更新中…';

  @override
  String get pluginsExistingVersion => 'このバージョンはインストール済みです。有効・無効の状態は変わりません。';

  @override
  String pluginsFileLimit(int limit) {
    return 'ファイルが大きすぎます。$limit バイト以下のファイルを選択してください。';
  }

  @override
  String get pluginsHttpTaskAbandon => 'この試行の観測を終了';

  @override
  String get pluginsHttpTaskAbandonDetails =>
      '最新の状態でアクティブなタスクがなく、元のライブラリが利用可能だと確認された場合のみ観測を終了できます。リモートへの影響がなかった証明にはなりません。識別子と不確実性は履歴に残り、新規リクエストには明示的な送信が必要です。';

  @override
  String get pluginsHttpTaskAbsent => '結果の配信なし';

  @override
  String get pluginsHttpTaskAccepted => '受付済み';

  @override
  String get pluginsHttpTaskAcknowledge => '終了済みタスクを確認';

  @override
  String get pluginsHttpTaskArchivedUnknown =>
      'ユーザーが観測を終了しました。以前のリモートへの影響は未確認のままです。この試行は再実行していません。';

  @override
  String get pluginsHttpTaskBase64 => 'Base64';

  @override
  String get pluginsHttpTaskBody => 'リクエスト本文';

  @override
  String get pluginsHttpTaskBodyFormat => 'リクエスト本文のエンコード';

  @override
  String get pluginsHttpTaskBusy => '使用中';

  @override
  String get pluginsHttpTaskCancel => 'キャンセルを要求';

  @override
  String get pluginsHttpTaskCancelled => 'キャンセルを検知。リモート側への影響はすでに発生している可能性があります';

  @override
  String get pluginsHttpTaskCatalogUnavailable =>
      'プラグインカタログの状態が利用できません。新規送信前にプラグインライブラリを更新してください。既存タスクの操作は可能です。';

  @override
  String get pluginsHttpTaskClosed => '終了済み';

  @override
  String get pluginsHttpTaskCompleted => '完了';

  @override
  String get pluginsHttpTaskConflict => '競合';

  @override
  String get pluginsHttpTaskConsumed => '結果は消費済み';

  @override
  String get pluginsHttpTaskControlUnknown =>
      '制御の結果を確認できませんでした。タスクの状態を更新してから次の操作を判断してください。';

  @override
  String pluginsHttpTaskCounters(String bytes, String calls) {
    return 'IO 呼び出し：$calls・計上バイト数：$bytes';
  }

  @override
  String get pluginsHttpTaskDeadline => '期限超過';

  @override
  String get pluginsHttpTaskDenied => '拒否';

  @override
  String get pluginsHttpTaskDetails =>
      '承認済みエンドポイントと実験的 HTTP 転送ハンドラーを持つ有効なプラグインで、明示的なリクエストを 1 回実行します。ライブラリが使用中でもタスクの状態は確認できます。';

  @override
  String get pluginsHttpTaskDisconnect => '接続のクリーンアップ失敗';

  @override
  String get pluginsHttpTaskEndpoint => '承認済みエンドポイント';

  @override
  String get pluginsHttpTaskEndpointsFailed =>
      'エンドポイントを整合した状態で読み取れないか、ライブラリが使用中です。タスクの操作は可能です。ライブラリが戻ってからエンドポイントを更新してください。';

  @override
  String get pluginsHttpTaskEvidenceUnavailable => '結果の証跡が利用不可';

  @override
  String pluginsHttpTaskExecution(int code, String fault) {
    return 'ゲスト実行：$fault・終了コード：$code';
  }

  @override
  String pluginsHttpTaskExit(
    String disconnect,
    String execution,
    String maintenance,
  ) {
    return 'ワーカー終了 — 実行：$execution・切断：$disconnect・保守：$maintenance';
  }

  @override
  String get pluginsHttpTaskExplicit =>
      '送信すると実際のリクエストを 1 回実行します。クリックごとに新しい識別子が作成されます。結果不明の送信や読み取りを自動再実行しません。キャンセルしてもリモート操作の取り消しは保証されません。';

  @override
  String get pluginsHttpTaskFailed => '失敗';

  @override
  String get pluginsHttpTaskHeaders => '通常のリクエストヘッダー（1 行に 名前: 値）';

  @override
  String get pluginsHttpTaskHeadersHint =>
      '重複ヘッダーは個別に保持します。認証情報と接続のヘッダーはランタイムのみが設定します。';

  @override
  String get pluginsHttpTaskHistory => '過去のタスク観測（最大 5 件）';

  @override
  String pluginsHttpTaskHttpResult(int code, String status) {
    return 'HTTP 結果：$status・リモートステータス：$code';
  }

  @override
  String get pluginsHttpTaskInactive => '接続は非アクティブ';

  @override
  String get pluginsHttpTaskInvalid =>
      'エンドポイント、メソッド、相対ターゲット、通常のヘッダー、本文のエンコード、タイムアウトが承認範囲内か確認してください。';

  @override
  String get pluginsHttpTaskInvalidOptions => '無効なオプション';

  @override
  String pluginsHttpTaskKey(String identity) {
    return 'タスク識別子：$identity';
  }

  @override
  String get pluginsHttpTaskLimit => 'クォータまたは上限に到達';

  @override
  String get pluginsHttpTaskLoadingEndpoints => '承認済みエンドポイントを読み込み中…';

  @override
  String get pluginsHttpTaskLocal => 'ライブラリ利用可能・実行中のタスクなし';

  @override
  String get pluginsHttpTaskModule => '無効なゲストモジュール';

  @override
  String get pluginsHttpTaskNew => '新しいリクエストを準備';

  @override
  String get pluginsHttpTaskNoEndpoints =>
      '有効かつ承認済みの HTTP 転送プラグインに一致するエンドポイントがありません。';

  @override
  String get pluginsHttpTaskNotFound => '見つかりません';

  @override
  String get pluginsHttpTaskOk => '正常';

  @override
  String get pluginsHttpTaskOutcomeUnknown =>
      'リモート側の結果は不明です。ロールバック済みと見なしたり再送したりしないでください';

  @override
  String get pluginsHttpTaskPackageChanged => 'パッケージの紐付けが変更済み';

  @override
  String get pluginsHttpTaskPending => '結果待ち';

  @override
  String get pluginsHttpTaskPoll => 'タスクを確認';

  @override
  String get pluginsHttpTaskProtocol => 'タスクのプロトコルエラー';

  @override
  String get pluginsHttpTaskRead => '結果を 1 回読み取る';

  @override
  String get pluginsHttpTaskReadBound => '読み取り上限超過';

  @override
  String get pluginsHttpTaskReadPending =>
      '結果は返されませんでした。状態を確認してから明示的に再読み取りしてください。';

  @override
  String get pluginsHttpTaskReadUnknown =>
      '結果の読み取りを確認できず、すでに消費された可能性があります。再読み取りは行いません。状態とワーカー終了は確認できます。';

  @override
  String get pluginsHttpTaskReady =>
      '結果の準備完了。明示的に読み取ってください。ワーカーの終了を意味するものではありません。';

  @override
  String get pluginsHttpTaskReclaimed => 'ワーカー終了・元のライブラリを返却済み';

  @override
  String get pluginsHttpTaskRecoveryRequired => 'ワーカー終了・クリーンアップまたは保守の修復が必要';

  @override
  String get pluginsHttpTaskRefresh => 'タスクの状態を更新';

  @override
  String get pluginsHttpTaskRefreshEndpoints => '承認済みエンドポイントを更新';

  @override
  String get pluginsHttpTaskRemoteError =>
      'リモートサーバーから 4xx/5xx が返されました。HTTP 通信は完了しており、ゲスト実行エラーとは別です。';

  @override
  String get pluginsHttpTaskRepair => 'クリーンアップを修復';

  @override
  String get pluginsHttpTaskResponseBase64 => 'レスポンス本文：正確な Base64';

  @override
  String get pluginsHttpTaskResponseHeaders => 'レスポンスヘッダー（重複保持・バイナリ値は Base64）';

  @override
  String get pluginsHttpTaskResponseText => 'レスポンス本文：プレーンテキストのプレビュー';

  @override
  String get pluginsHttpTaskResultUnavailable => '結果の配信が利用不可';

  @override
  String get pluginsHttpTaskRevoked => '承認取り消し済み';

  @override
  String get pluginsHttpTaskRunning => '実行中・ワーカーがライブラリを所有';

  @override
  String get pluginsHttpTaskSpawn => 'ワーカーを起動できませんでした';

  @override
  String get pluginsHttpTaskStart => '新しいリクエストを送信';

  @override
  String get pluginsHttpTaskStartUnknown =>
      '送信結果は不明です。識別子は保持されています。状態を照会して同じタスクを確認してください。リクエストは再送しません。';

  @override
  String get pluginsHttpTaskStatusFailed =>
      'タスクの状態を確認できませんでした。状態を更新してください。リクエストは再実行していません。';

  @override
  String get pluginsHttpTaskStopping => '停止中・ワーカーの実際の終了を待機';

  @override
  String pluginsHttpTaskSubmission(String identity) {
    return '送信識別子：$identity';
  }

  @override
  String get pluginsHttpTaskTarget => '相対ターゲット（例：/v1/items?limit=10）';

  @override
  String get pluginsHttpTaskText => 'UTF-8 テキスト';

  @override
  String get pluginsHttpTaskTimeout => 'タイムアウト（1～30000 ミリ秒、承認範囲内）';

  @override
  String get pluginsHttpTaskTitle => 'HTTP タスク';

  @override
  String get pluginsHttpTaskTrap => 'ゲスト実行でトラップが発生';

  @override
  String get pluginsHttpTaskUnavailable => '元のライブラリが利用不可・復旧対応が必要';

  @override
  String get pluginsHttpTaskUnsupported => '未対応の操作';

  @override
  String get pluginsHttpTaskWorking => 'タスク制御の応答を待機中…';

  @override
  String get pluginsImport => 'インポート';

  @override
  String get pluginsImportDetails =>
      'インポート後に有効にするか選択できます。無効化やアンインストール後もコンテンツは保持されます。';

  @override
  String pluginsImportPreview(String name) {
    return 'インポートのプレビュー：$name';
  }

  @override
  String get pluginsImportUnknown => 'インポートを確認できませんでした';

  @override
  String get pluginsImportedDisabled => 'インポート済みで無効です。許可する権限を選択してください。';

  @override
  String get pluginsInputFailed => '入力ファイルを読み取れませんでした';

  @override
  String get pluginsInputTooLong => '入力上限に達しました。テキストを短くして再試行してください。';

  @override
  String get pluginsInspectFailed => 'プラグインのプレビューを読み込めませんでした';

  @override
  String get pluginsInspectedOnly => 'ファイルの検査のみ完了しました。インポート後、別途有効にしてください。';

  @override
  String get pluginsInsufficientApproval =>
      'プラグインは有効ですが、コンテンツへの権限が必要です。ワークスペースは読み取り専用です。無効にして権限を再確認してください。';

  @override
  String pluginsIoApproved(String permissions) {
    return '承認済み：$permissions';
  }

  @override
  String get pluginsIoCredentialUse => '許可された認証情報を使用';

  @override
  String pluginsIoDeclared(String permissions) {
    return '要求されたネットワーク・ファイル権限：$permissions';
  }

  @override
  String get pluginsIoFileCreate => 'ファイルを作成';

  @override
  String get pluginsIoFileDelete => 'ファイルを削除';

  @override
  String get pluginsIoFileList => '許可されたフォルダーを参照';

  @override
  String get pluginsIoFileRead => '許可されたファイルを読む';

  @override
  String get pluginsIoFileReplace => 'ファイルを置換';

  @override
  String get pluginsIoHttpListen => 'ネットワーク接続を待ち受ける';

  @override
  String get pluginsIoHttpPublish => 'API サービスを提供';

  @override
  String get pluginsIoHttpRequest => 'ネットワーク API を呼び出す';

  @override
  String get pluginsIoNoneApproved => 'ネットワーク・ファイル権限は未承認です';

  @override
  String get pluginsIoRevoke => 'ネットワーク・ファイル権限をすべて取り消す';

  @override
  String get pluginsIoSave => 'ネットワーク・ファイル権限を保存';

  @override
  String get pluginsIoScopeNotice =>
      'ここでは権限の種類のみ保存します。サーバーアドレス、ファイルアクセス、認証情報は別途承認が必要です。利用不可の機能は有効になりません。変更後はフォームを開き直してください。';

  @override
  String get pluginsIoTitle => 'ネットワークとファイルの権限';

  @override
  String get pluginsIoWebSocketConnect => 'WebSocket サービスに接続';

  @override
  String get pluginsListUnknown => 'プラグイン一覧を確認できませんでした';

  @override
  String get pluginsManageAbove => '上のワークスペースプラグインの操作項目で管理してください。';

  @override
  String get pluginsManagementUnavailable =>
      'プラグイン管理は利用できません。既存のコンテンツは引き続き閲覧できます。';

  @override
  String get pluginsNoPermissions => 'コンテンツの権限は宣言されていません。';

  @override
  String get pluginsOpenTextTool => 'テキストツールを開く';

  @override
  String get pluginsOpenView => 'ビューを開く';

  @override
  String get pluginsOpeningView => 'プラグインのビューを開いています…';

  @override
  String get pluginsOperation => '操作結果を照会';

  @override
  String pluginsOtherCapability(String name) {
    return 'その他の宣言済み権限：$name';
  }

  @override
  String get pluginsPackageFile => 'Morrow プラグイン';

  @override
  String get pluginsPreviewOnly => 'プレビューのみです。結果は既存のコンテンツに自動保存されません。';

  @override
  String get pluginsPreviewTruncated => '…先頭の 4,096 文字のみ表示';

  @override
  String get pluginsProtection => 'コンテンツの保護';

  @override
  String get pluginsProtectionDetails =>
      'このシステムアカウントで復旧できるよう、元の保護ファイルをバックアップしてください。カードや添付ファイルは含まれません。';

  @override
  String get pluginsProtectionFileType => 'ライブラリ保護ファイル';

  @override
  String get pluginsProtectionSaved =>
      '保護ファイルをバックアップしました。起動に失敗した場合、復旧時に選択してください。';

  @override
  String get pluginsRead => 'コンテンツを閲覧';

  @override
  String get pluginsReadingState => 'プラグインの状態を読み込み中…';

  @override
  String pluginsRefreshFailed(String reason) {
    return '$reason。一覧を更新できませんでした。「一覧を更新」で読み込みを再試行してください。';
  }

  @override
  String get pluginsRefreshList => '一覧を更新';

  @override
  String get pluginsRefreshState => '状態を更新';

  @override
  String get pluginsRename => '名前を変更';

  @override
  String pluginsResultBytes(int count, String preview) {
    return '$count バイト\n$preview';
  }

  @override
  String get pluginsSavePermissions => '権限を保存';

  @override
  String pluginsSelectedFile(String name) {
    return '選択したファイル：$name';
  }

  @override
  String get pluginsServiceAcknowledgeUncertain => '更新後の記録を確認しました';

  @override
  String get pluginsServiceAddScope => 'コンテンツ範囲を追加';

  @override
  String get pluginsServiceAttachmentId => '正確な添付ファイル識別子';

  @override
  String get pluginsServiceAuthenticationUnavailable =>
      '選択した認証が存在しない、無効、期限切れ、または別の主体に属しています。範囲の下書きは保持されます。有効な認証に置き換えるか明示的に削除してください。';

  @override
  String get pluginsServiceAuthorities => '認証と公開の記録';

  @override
  String get pluginsServiceCardId => '正確なカード識別子';

  @override
  String get pluginsServiceCatalogChanged =>
      'パッケージカタログが変更されたか利用できません。下書きは保持しています。保存前に選択を明示的に更新してください。';

  @override
  String get pluginsServiceClearToken => 'トークンを消去';

  @override
  String get pluginsServiceCloseEditor => 'エディターを閉じる';

  @override
  String get pluginsServiceConfigDigest => '設定ダイジェスト';

  @override
  String get pluginsServiceConfiguration => '保存済み設定';

  @override
  String get pluginsServiceConfigurations => '保存済み設定';

  @override
  String get pluginsServiceCopyClear => 'トークンをコピーして消去';

  @override
  String get pluginsServiceCreated => '作成日時（UTC）';

  @override
  String get pluginsServiceDays => '要求する有効期間（1～30 日）';

  @override
  String get pluginsServiceDigestFixed =>
      '編集時も元のパッケージダイジェストを保持します。一致するパッケージを選択してください。選択しても有効化されません。';

  @override
  String get pluginsServiceDisable => '無効化';

  @override
  String get pluginsServiceDisabled => '無効';

  @override
  String get pluginsServiceEditConfig => '設定を編集';

  @override
  String get pluginsServiceEditPublication => '公開を編集';

  @override
  String get pluginsServiceExpired => '期限切れ、または有効期間前';

  @override
  String get pluginsServiceExpires => '実際の有効期限（UTC）';

  @override
  String get pluginsServiceHandler => '宣言済みサービスハンドラー';

  @override
  String get pluginsServiceIdentity => 'サービス識別子';

  @override
  String get pluginsServiceInvalid => '保存前に入力欄、選択した承認、現在のパッケージを確認してください。';

  @override
  String get pluginsServiceIssue => 'トークンを発行';

  @override
  String get pluginsServiceIssuedToken => '一度だけ表示される Bearer トークン';

  @override
  String get pluginsServiceListenAddress => '数値の待ち受けアドレスとポート';

  @override
  String get pluginsServiceLoadFailed => '記録を更新できませんでした。変更前に再度更新してください。';

  @override
  String get pluginsServiceManagementOnly =>
      '保存済みの設定と承認を管理します。保存してもリスナーの起動、パッケージの実行、サービスの有効化は行いません。';

  @override
  String get pluginsServiceMethod => 'HTTP メソッド';

  @override
  String get pluginsServiceNewAuthentication => '新しい認証';

  @override
  String get pluginsServiceNewConfig => '新しい設定';

  @override
  String get pluginsServiceNo => 'いいえ';

  @override
  String get pluginsServiceNoAuthentication => '先に現在有効な認証記録を作成してください。';

  @override
  String get pluginsServiceNoAuthorities => '認証や公開の記録はありません。';

  @override
  String get pluginsServiceNoConfigurations => 'サービス設定はありません。';

  @override
  String get pluginsServicePackage => '宣言・承認済みパッケージ';

  @override
  String get pluginsServicePackageDigest => 'パッケージダイジェスト';

  @override
  String get pluginsServicePackageUnavailable =>
      '一致するパッケージまたは待ち受け・公開の承認が利用できません。過去の記録は閲覧・無効化できます。';

  @override
  String get pluginsServicePath => '正確なリクエストパス';

  @override
  String get pluginsServicePolicyChanged =>
      '元の記録が変更されたか利用できなくなりました。選択を更新するか現在の記録からエディターを開き直してください。下書きは保持しています。';

  @override
  String get pluginsServicePrincipalId => '主体の識別子';

  @override
  String get pluginsServicePrincipals => '許可された主体とコンテンツ範囲';

  @override
  String get pluginsServicePublicationEditor => '公開の承認';

  @override
  String get pluginsServicePublicationHelp =>
      '承認はこの設定、リビジョン、参照に厳密に紐付きます。実際の有効期限は選択したすべての認証記録によって制限され、要求より短くなる場合があります。保存しても待ち受けは開始しません。';

  @override
  String get pluginsServicePublicationMismatch =>
      'この公開は現在の設定に一致しません。確認して置き換え用の承認を明示的に保存してください。';

  @override
  String get pluginsServiceQueryPath => '結果照会用の別パス（任意）';

  @override
  String get pluginsServiceReference => '承認の参照';

  @override
  String get pluginsServiceRefresh => '記録を更新';

  @override
  String get pluginsServiceRefreshSelection => 'この選択を更新';

  @override
  String get pluginsServiceRemovePrincipal => '主体を削除';

  @override
  String get pluginsServiceRemoveScope => '範囲を削除';

  @override
  String get pluginsServiceRetention => 'リクエスト履歴の保持期間（ミリ秒、最大 30 日）';

  @override
  String get pluginsServiceRevision => 'リビジョン';

  @override
  String get pluginsServiceRotate => 'トークンをローテーション';

  @override
  String get pluginsServiceRotateAuthentication => '認証をローテーション';

  @override
  String get pluginsServiceRunAbandon => '記録を保持してこの試行を終了';

  @override
  String get pluginsServiceRunAdvanced => 'リクエストとワーカーの上限';

  @override
  String get pluginsServiceRunAttempt => '未解決の開始試行';

  @override
  String get pluginsServiceRunBoundsHint =>
      '上限はプラグインの宣言と保存済み承認にも適合する必要があります。予約済み作業はキャンセル後も累積枠を消費します。期限到達で停止し、自動更新は行いません。';

  @override
  String get pluginsServiceRunBytes => '実行バイト上限（最大 67,108,864）';

  @override
  String get pluginsServiceRunCalls => 'ジョブごとの呼び出し数（最大 1,024）';

  @override
  String get pluginsServiceRunCancelled => 'キャンセル済み';

  @override
  String get pluginsServiceRunClosed => '終了済み';

  @override
  String get pluginsServiceRunConcurrent => '同時ジョブ数（最大 128）';

  @override
  String get pluginsServiceRunControlUnknown =>
      '制御結果は不明です。別の操作前に元のサービス状態を更新してください。';

  @override
  String get pluginsServiceRunDenied => '拒否';

  @override
  String get pluginsServiceRunExited => 'サービス終了';

  @override
  String get pluginsServiceRunHeaderBytes => 'ヘッダー最大サイズ（バイト、最大 65,536）';

  @override
  String get pluginsServiceRunHint =>
      '承認済みの公開と有限の実行上限を選び、明示的にサービスを開始してください。停止後は元のワークスペース所有者が戻ってから結果を確認してください。';

  @override
  String pluginsServiceRunHostFailure(String detail) {
    return 'ホスト診断：$detail';
  }

  @override
  String get pluginsServiceRunHttpPanel =>
      'API サービスがこのタスクを所有しています。上のサービス実行パネルで停止または終了の確認を行ってください。HTTP リクエストの下書きは保持しています。';

  @override
  String get pluginsServiceRunIdentityChanged =>
      '別のタスクがコンテンツを所有しています。このパネルは以前のサービス識別子でそのタスクを操作しません。';

  @override
  String get pluginsServiceRunInvalid =>
      '選択したサービスと数値上限を確認してください。新規実行は送信していません。';

  @override
  String get pluginsServiceRunInvalidOutcome => '無効な設定';

  @override
  String get pluginsServiceRunJobBytes => 'ジョブごとのバイト数（最大 16,777,216）';

  @override
  String get pluginsServiceRunJobs => 'タスク予約総数（最大 1,000,000）';

  @override
  String get pluginsServiceRunLastObservation => '最後の観測を表示中。現在の状態は未確認です。';

  @override
  String get pluginsServiceRunLifetime => '実行時間（ミリ秒、最大 3,600,000）';

  @override
  String get pluginsServiceRunLimit => '上限に到達';

  @override
  String get pluginsServiceRunLocal => 'コンテンツをローカルで利用可能';

  @override
  String pluginsServiceRunNetwork(
    String bind,
    String listener,
    String supervision,
  ) {
    return 'バインド：$bind・リスナー：$listener・監視：$supervision';
  }

  @override
  String get pluginsServiceRunNextSettings => '次の明示的な実行の設定';

  @override
  String get pluginsServiceRunNoSelection =>
      '利用可能な承認済み公開がありません。プラグイン、設定、認証の状態を確認してください。';

  @override
  String get pluginsServiceRunOutboundAttempt => 'この開始試行に紐付くエンドポイント';

  @override
  String get pluginsServiceRunOutboundClear => 'エンドポイントの選択を解除';

  @override
  String get pluginsServiceRunOutboundFailed =>
      'エンドポイント一覧を確認できませんでした。選択済みのものを使用する前に更新してください。';

  @override
  String get pluginsServiceRunOutboundHint =>
      '外部 API（任意、最大 8 件）。このパッケージ用に承認されたエンドポイントのみ表示します。未選択なら外部呼び出しを禁止します。';

  @override
  String get pluginsServiceRunOutboundStale =>
      '選択したエンドポイントが変更されたか利用できません。現在のバージョンを明示的に選ぶか、選択を解除してください。';

  @override
  String get pluginsServiceRunOwned => '実行中のサービスがコンテンツを管理';

  @override
  String get pluginsServiceRunPending => '待機中';

  @override
  String get pluginsServiceRunReclaimed => 'コンテンツの所有権を回収済み・確認が必要';

  @override
  String get pluginsServiceRunReclaiming => 'コンテンツの所有権返却を待機';

  @override
  String get pluginsServiceRunRecovery => 'クリーンアップの修復が必要';

  @override
  String get pluginsServiceRunRequestBytes => 'リクエスト最大サイズ（バイト）';

  @override
  String get pluginsServiceRunResponseBytes => 'レスポンス最大サイズ（バイト）';

  @override
  String get pluginsServiceRunRunning => 'サービス実行中';

  @override
  String get pluginsServiceRunSelection => '承認済みサービス公開';

  @override
  String get pluginsServiceRunStale =>
      '選択したパッケージ、設定、承認が変更されました。記録を更新し、開始前に再選択してください。';

  @override
  String get pluginsServiceRunStart => '上限付きサービスを開始';

  @override
  String get pluginsServiceRunStartRejected =>
      '開始応答がエラーを報告しました。現在のタスクは確認済みです。原因とクリーンアップ状態を確認してから続行してください。';

  @override
  String get pluginsServiceRunStartUnknown =>
      '開始結果は不明です。試行識別子を保持しています。更新して確認してください。自動で再開始することはありません。';

  @override
  String get pluginsServiceRunStarting => 'サービスを開始中';

  @override
  String get pluginsServiceRunStatusFailed =>
      '現在のサービス状態を確認できませんでした。次の操作前に更新してください。';

  @override
  String get pluginsServiceRunStop => 'サービスを停止';

  @override
  String get pluginsServiceRunStopping => '停止中・リスナーとワーカーの終了を待機';

  @override
  String get pluginsServiceRunSucceeded => '成功';

  @override
  String get pluginsServiceRunTask => '現在のタスク識別子';

  @override
  String get pluginsServiceRunTimeout => 'ジョブのタイムアウト（ミリ秒、最大 30,000）';

  @override
  String get pluginsServiceRunTimeoutOutcome => 'タイムアウト';

  @override
  String get pluginsServiceRunTitle => 'API サービスを実行';

  @override
  String get pluginsServiceRunTotalBytes => 'ワーカーのバイト上限（最大 67,108,864）';

  @override
  String get pluginsServiceRunTransport => '転送エラー';

  @override
  String get pluginsServiceRunUnavailable => 'コンテンツストレージが利用不可';

  @override
  String get pluginsServiceSaveConfig => '設定を保存';

  @override
  String get pluginsServiceSavePublication => '公開承認を保存';

  @override
  String get pluginsServiceSaved => '保存しました。返されたリビジョンと実際の有効期限を以下で確認してください。';

  @override
  String get pluginsServiceScopeAttachment => '添付ファイルを閲覧';

  @override
  String get pluginsServiceScopeCreate => 'コンテンツを作成';

  @override
  String get pluginsServiceScopeEdit => 'コンテンツを編集';

  @override
  String get pluginsServiceScopeKind => '許可するコンテンツ操作';

  @override
  String get pluginsServiceScopeQuery => '操作を照会';

  @override
  String get pluginsServiceScopeRead => 'コンテンツを閲覧';

  @override
  String get pluginsServiceScopeRename => 'カード名を変更';

  @override
  String get pluginsServiceScopeSummary => '概要を閲覧';

  @override
  String get pluginsServiceScopesHelp =>
      '認証を明示的に選択し、許可する操作と正確なオブジェクト識別子を追加してください。範囲や主体の削除には専用ボタンを使用します。編集中も既存の範囲を保持します。';

  @override
  String get pluginsServiceTitle => 'サービス設定';

  @override
  String get pluginsServiceTls => 'TLS を必須にする';

  @override
  String get pluginsServiceTlsAttempt => 'この開始試行に紐付く証明書 PEM ダイジェスト';

  @override
  String get pluginsServiceTlsCertificate => '証明書チェーンを選択';

  @override
  String get pluginsServiceTlsChecked =>
      '証明書と鍵の対応を確認しました。証明書 PEM の SHA-256 を以下に表示します。クライアント側では引き続きホスト名、有効期間、信頼チェーンの検証が必要です。';

  @override
  String get pluginsServiceTlsChecking => '選択した証明書を処理中…';

  @override
  String get pluginsServiceTlsFailed =>
      '証明書の検査に失敗しました。PEM ファイル、鍵の対応、ローカルパスを確認して再試行してください。';

  @override
  String get pluginsServiceTlsHelp =>
      'ループバック以外のアドレスには TLS が必要です。ここでは要件のみ保存し、リスナーや TLS ID は作成しません。';

  @override
  String get pluginsServiceTlsHint =>
      'PEM 証明書チェーンと秘密鍵を選択して検査してください。開始時にファイルを再検査します。有効な証明書は自動でローテーションされません。';

  @override
  String get pluginsServiceTlsInspect => '証明書を検査';

  @override
  String get pluginsServiceTlsOutsideValidity =>
      '証明書チェーンが有効期間前または期限切れです。証明書を確認または交換し、開始前に再検査してください。';

  @override
  String get pluginsServiceTlsPrivateKey => '秘密鍵を選択';

  @override
  String get pluginsServiceTlsRecheck =>
      '開始前に証明書を再検査してください。時刻が変わっても以前の選択は復元されません。';

  @override
  String get pluginsServiceTlsUnavailable =>
      'このバックエンドはローカル TLS 証明書の選択に対応していません。';

  @override
  String pluginsServiceTlsValidity(String end, String start) {
    return '証明書チェーン共通の有効期間（UTC）：$start ～ $end。期限後はサービスを停止します。';
  }

  @override
  String get pluginsServiceTokenDiscarded =>
      'パネルを閉じたため、一度だけ表示されるトークンを消去しました。必要なら明示的に新規発行してください。';

  @override
  String get pluginsServiceTokenHelp =>
      'このトークンは今だけ表示されます。必要なら明示的にコピーしてください。消去またはパネルを閉じるとセッションから削除され、一覧から取得できません。ローテーションすると以前のトークンが置き換わります。';

  @override
  String get pluginsServiceUncertainHelp =>
      'まず元の記録を更新して確認してください。この通知の確認は次の明示的な操作を可能にするだけです。以前の変更の失敗を証明せず、再実行もしません。';

  @override
  String get pluginsServiceUnsupported => '未対応の過去の値';

  @override
  String get pluginsServiceWorking => '処理中…';

  @override
  String get pluginsServiceWriteUnknown => '直前の変更結果は不明です。再送していません。';

  @override
  String get pluginsServiceYes => 'はい';

  @override
  String get pluginsSettingsUnknown => '設定はまだ確認されていません。状態を更新してから再度選択してください。';

  @override
  String get pluginsSnapshotDetails =>
      'バックアップにはカード、添付ファイル、監査記録が含まれます。外部リソースは参照のままです。復旧には元のシステムアカウントが必要です。';

  @override
  String get pluginsSnapshotSaved => '添付ファイルと元の保護ファイルを含め、ライブラリをバックアップしました。';

  @override
  String get pluginsStateUnavailable => 'プラグインの状態を読み込めませんでした。再試行してください。';

  @override
  String get pluginsSummary => '概要を閲覧';

  @override
  String get pluginsTextInput => '入力テキスト';

  @override
  String get pluginsThirdParty => 'サードパーティ製プラグイン';

  @override
  String get pluginsTlsIdentitiesDisable => 'ID を無効化';

  @override
  String get pluginsTlsIdentitiesEmpty => 'このライブラリに保存済み ID はありません。';

  @override
  String get pluginsTlsIdentitiesFileMode => '次回開始：検査済みローカルファイル。';

  @override
  String get pluginsTlsIdentitiesHint =>
      'ID を明示的に選択してください。置換や無効化により、その ID を使用するサービスは停止します。新たな開始には常に明示的な操作が必要です。';

  @override
  String get pluginsTlsIdentitiesImport => 'インポート・置換用の証明書ファイルを準備';

  @override
  String get pluginsTlsIdentitiesReplace => '検査済みファイルで置換';

  @override
  String get pluginsTlsIdentitiesSave => '新しい ID として保存';

  @override
  String get pluginsTlsIdentitiesSaved =>
      '保存しました。以下の ID とリビジョンを確認してから、新しい開始用に選択してください。';

  @override
  String get pluginsTlsIdentitiesSavedMode =>
      '次回開始：保存済み ID。開始時にホストが証明書の有効期間を確認します。';

  @override
  String get pluginsTlsIdentitiesSelect => '次回開始に使用';

  @override
  String get pluginsTlsIdentitiesStale =>
      '選択した ID が変更・無効化されたか、未更新です。現在の ID を選び直してください。';

  @override
  String get pluginsTlsIdentitiesTitle => '保存済み TLS ID';

  @override
  String get pluginsTlsIdentitiesUnknownHint =>
      '確認前に記録を更新して点検してください。受領証がないことは変更の失敗を意味しません。確認せず再作成しないでください。';

  @override
  String get pluginsTlsIdentitiesUseFile => '次回開始に検査済みファイルを使用';

  @override
  String get pluginsTransform => '変換';

  @override
  String get pluginsTransformUnknown => '変換を確認できませんでした';

  @override
  String get pluginsUiExecution => 'プラグインの実行が完了しませんでした。ビューを開き直して再試行してください。';

  @override
  String get pluginsUiRejected => 'プラグインの操作は受け付けられませんでした。入力と現在の権限を確認してください。';

  @override
  String get pluginsUiUnavailable => 'プラグインは利用できません。状態を確認してビューを開き直してください。';

  @override
  String get pluginsUnavailableView => 'プラグインのビューは利用できません';

  @override
  String pluginsUnconfirmed(String reason) {
    return '$reason。操作は未確認です。更新後の状態を確認してから再度選択してください。';
  }

  @override
  String get pluginsUninstallKeepContent => 'アンインストール（内容を保持）';

  @override
  String get pluginsUninstallUnknown => 'アンインストールを確認できませんでした';

  @override
  String get pluginsUninstalled => 'アンインストールしました。既存のコンテンツは保持されています。';

  @override
  String get pluginsUpdatingView => 'プレビューを更新中…';

  @override
  String get pluginsUseText => '代わりにテキストを使用';

  @override
  String get pluginsUseTransform => '変換を使用';

  @override
  String get pluginsViewFailed => 'プラグインのビューを開けませんでした';

  @override
  String get pluginsWorkbench => 'ワークスペースプラグイン';

  @override
  String get pluginsWorkbenchReadOnly =>
      'プラグインは許可されていますが、ワークスペースは読み取り専用です。ライブラリまたはプラグインの問題を解決して状態を更新してください。';

  @override
  String get recoveryAllFiles => 'すべてのファイル';

  @override
  String get recoveryBackupExists => 'バックアップ先にファイルがあります。新しい名前を選んでください。';

  @override
  String get recoveryBackupFile => 'ライブラリのバックアップ';

  @override
  String get recoveryBackupUnknown =>
      'バックアップ結果の確認が必要です。現在のファイルを保持し、保存先を確認してください。';

  @override
  String get recoveryBindingMissing =>
      'このライブラリには保護ファイルの関連付けがなく、選択したファイルと照合できません。';

  @override
  String get recoveryBusy => '別のプロセスが使用中です。他のウィンドウを閉じて再試行してください。';

  @override
  String get recoveryChooseKey => '復旧ファイルを選択';

  @override
  String get recoveryCloseFirst => 'ワークスペースは実行中です。ライブラリを切り替える前に閉じてください。';

  @override
  String get recoveryFailed => '復旧が完了しませんでした。元のファイルを保持して再試行してください。';

  @override
  String get recoveryIdentityBusy => 'このライブラリの別のコピーが使用中です。そちらを閉じてから開いてください。';

  @override
  String get recoveryIdentityMismatch =>
      '登録されたライブラリ識別情報が一致しません。元のデータを保持し、正しいバックアップを復元してください。';

  @override
  String get recoveryKeyFile => 'ライブラリ保護ファイル';

  @override
  String get recoveryKeyGuide =>
      '保護ファイルがない、または破損している場合はバックアップを選んでください。このライブラリに属するファイルと、元のシステムアカウントが必要です。';

  @override
  String get recoveryKeyMismatch =>
      'キーが一致しないか復号できません。元のファイルとシステムアカウントを使ってください。';

  @override
  String get recoveryKeyUnknown =>
      '復旧結果の確認が必要です。再度開いてください。以前の保護ファイルがあった場合、そのコピーは保持されています。';

  @override
  String get recoveryLibraryInvalid =>
      'ライブラリを検証または開けませんでした。元のライブラリと保護キーを保持して再試行してください。';

  @override
  String get recoveryMaintenance => 'ライブラリの確認が必要です。元のファイルを保持し、診断の詳細を確認してください。';

  @override
  String get recoveryMissingKey => '保護キーがありません。元の .audit-key を復元して再試行してください。';

  @override
  String get recoveryMissingLibrary =>
      'キーはありますが、ライブラリがないか空です。元のライブラリを復元してください。';

  @override
  String get recoveryOpenFailed =>
      'ワークスペースを開けませんでした。プラグインとデータフォルダーを確認して再試行してください。';

  @override
  String get recoveryPluginUnavailable =>
      'ワークスペースプラグインを利用できません。既存の内容は閲覧・書き出しできます。';

  @override
  String get recoveryRegistryInvalid =>
      '有効なライブラリ登録が破損しているか未対応です。データ保護のため開く処理を中止しました。';

  @override
  String get recoveryRegistryUnreadable =>
      '有効なライブラリまたは登録ファイルを読めません。元の場所を確認してください。代替ライブラリは自動作成しません。';

  @override
  String get recoveryRetry => '再試行';

  @override
  String get recoverySnapshot => 'ライブラリのバックアップを復元';

  @override
  String get recoverySnapshotGuide =>
      'バックアップを新しいフォルダーに復元して切り替えられます。元のフォルダーは保持します。内容はバックアップ時点に戻り、元のシステムアカウントが必要です。';

  @override
  String get recoverySnapshotInvalid =>
      'バックアップの形式または整合性確認が無効です。元のバックアップを保持してください。';

  @override
  String get recoverySnapshotUnknown =>
      '復元結果の確認が必要です。復元先を確認してください。元のライブラリは置き換えていません。';

  @override
  String recoverySwitchUnconfirmed(String path) {
    return '$path に復元しましたが、切り替えは未確認です。このフォルダーを保持し、ワークスペースを開き直して確認してください。';
  }

  @override
  String get recoverySwitchUnknown => 'ライブラリの切り替えは未確認です。ワークスペースを開き直して確認してください。';

  @override
  String get recoveryTargetExists => '復元先は既に存在します。まだ存在しない新しいフォルダーを選んでください。';

  @override
  String get recoveryTitle => 'ワークスペースを開き直す';

  @override
  String get visualApplyColor => '色を適用';

  @override
  String get visualApplyComponent => 'このコンポーネントに適用';

  @override
  String get visualApplyTexture => 'メディアを適用';

  @override
  String visualAttachmentDetails(String action, String extension, String size) {
    return '$extension · $size · $action';
  }

  @override
  String get visualAttachmentFailure => 'ファイル操作に失敗しました。ファイルと空き容量を確認してください。';

  @override
  String get visualAttachmentPreview => 'ローカル添付のプレビュー';

  @override
  String get visualAttachmentReadFailure => '添付を読み取れませんでした。再度読み込んでください。';

  @override
  String get visualAudio => '音声';

  @override
  String get visualAudioStateFailure => '音声の状態を確認できませんでした。再試行してください。';

  @override
  String get visualAutoLyrics => '歌詞がない場合は自動でオンライン検索';

  @override
  String get visualCancel => 'キャンセル';

  @override
  String get visualChangeCover => 'アートワークを変更';

  @override
  String get visualChooseAudio => '音声ファイル、または同名の LRC 歌詞ファイルを選んでください。';

  @override
  String get visualChooseLyrics => 'LRC または TXT の歌詞ファイルを選んでください。';

  @override
  String get visualClickPreview => '選択してプレビュー';

  @override
  String get visualClose => '閉じる';

  @override
  String get visualCloseDialog => 'ダイアログを閉じる';

  @override
  String get visualCloseWindow => 'ウィンドウを閉じる';

  @override
  String get visualCollapsePlaylist => 'プレイリストを折りたたむ';

  @override
  String get visualColorGuide => 'ホイールをドラッグして色相と彩度を選び、明るさを調整します。色の値も入力できます。';

  @override
  String get visualColorTitle => '自分の空間に彩りを';

  @override
  String visualComponentCompass(String title) {
    return '$title · カラーホイール';
  }

  @override
  String get visualComponents => 'コンポーネントとカード';

  @override
  String get visualComponentsGuide =>
      '既定では各項目はテーマに従います。1 枚のカードを変更しても他には影響しません。';

  @override
  String get visualCornerTips1 => 'すべてのアイデアが役立つ必要はありません。\n今日を少し面白くするだけでも。';

  @override
  String get visualCornerTips2 => '書き留めたら、育てていきましょう。\n最初から完成していなくても大丈夫。';

  @override
  String get visualCornerTips3 => '自分のための余白を少し。\n好奇心にも呼吸する場所が必要です。';

  @override
  String get visualCornerTips4 => '今日は何か新しいことを。\n小さな寄り道に驚きがあるかも。';

  @override
  String get visualCornerTips5 => '空想にも行き先はあります。\n思考が散歩する道を作りましょう。';

  @override
  String get visualCornerTips6 => '好きなことに時間を。\n価値を証明しなくてもいいのです。';

  @override
  String get visualCornerTips7 => '進歩は小さくても。\n始めようと思ったことにも意味があります。';

  @override
  String get visualCornerTips8 => '時には窓の外を。\n暮らしもひらめきの源です。';

  @override
  String get visualCover => 'アートワーク';

  @override
  String get visualCustomCompass => 'カラーホイール · カスタム';

  @override
  String get visualCustomMaterialGuide => 'オフにするとテーマに従います。この項目の個別設定は保持されます。';

  @override
  String get visualDefaultOpen => '既定のアプリで開く';

  @override
  String get visualDownloadOpen => 'ダウンロードして開く';

  @override
  String get visualEmbeddedLyrics => '音声に埋め込み済み';

  @override
  String get visualExpandPlaylist => 'プレイリストを展開';

  @override
  String get visualFile => 'ファイル';

  @override
  String get visualFileOpenFailure =>
      'ファイルを開けませんでした。対応アプリを入れるか、添付を保存して別のアプリで開いてください。';

  @override
  String get visualFileRetry => 'ファイル操作に失敗しました。再試行してください。';

  @override
  String get visualFindLyrics => '歌詞を探す';

  @override
  String get visualFindLyricsGuide => 'LRCLIB で曲名とアーティストを検索し、合うバージョンを選んでください。';

  @override
  String get visualFollowTheme => 'テーマに合わせる';

  @override
  String get visualFooterLyrics => 'フッターに歌詞を表示';

  @override
  String get visualFooterTips => 'フッターにヒントを表示';

  @override
  String get visualFooterTips1 => '急がなくて大丈夫。好奇心に少し時間を。';

  @override
  String get visualFooterTips10 => 'すべての時間を埋めなくても。余白を残しましょう。';

  @override
  String get visualFooterTips2 => '思いつきをメモ。整理は後でできます。';

  @override
  String get visualFooterTips3 => '大きなアイデアを、今日の小さな一歩に。';

  @override
  String get visualFooterTips4 => '少し伸びをして、目を休めましょう。';

  @override
  String get visualFooterTips5 => 'アイデアの答えは、まだなくても大丈夫。';

  @override
  String get visualFooterTips6 => 'ゆっくり進むと見つかることもあります。';

  @override
  String get visualFooterTips7 => '細部を残すことも、アイデアを育てる方法。';

  @override
  String get visualFooterTips8 => '今日のメモが、明日の始まりになるかも。';

  @override
  String get visualFooterTips9 => '思いを巡らせたら、好きなことに戻りましょう。';

  @override
  String get visualFrosting => 'すりガラス効果';

  @override
  String get visualGif => 'アニメーション GIF';

  @override
  String get visualHexColor => 'HEX カラー';

  @override
  String get visualHexInvalid => '6 桁の 16 進数カラーを入力してください。';

  @override
  String get visualImage => '画像';

  @override
  String get visualImageDecodeFailure => '画像をデコードできません。保存して別のアプリで開いてください。';

  @override
  String visualImageLoadFailure(String name) {
    return '画像を読み込めませんでした: $name';
  }

  @override
  String visualImageNotImported(String name) {
    return '$name（画像未読み込み）';
  }

  @override
  String visualImageUnavailable(String name) {
    return '画像を利用できません: $name';
  }

  @override
  String get visualImportFailure => '読み込みに失敗しました。ファイル、文字コード、空き容量を確認してください。';

  @override
  String get visualImportLyrics => '歌詞を読み込む';

  @override
  String get visualImportMusic => '音楽を読み込む';

  @override
  String get visualImportMusicHint => '＋でローカルの曲を追加';

  @override
  String get visualIndependentMaterial => 'カスタムの質感';

  @override
  String get visualInheritColor => 'テーマの色を使う';

  @override
  String get visualLinkFailure => 'リンクを開けませんでした。アドレスをコピーして再試行してください。';

  @override
  String visualLoadImage(String name) {
    return '画像を読み込む · $name';
  }

  @override
  String get visualLoading => '読み込み中…';

  @override
  String get visualLyricsEmpty => '歌詞ファイルが空です。';

  @override
  String get visualLyricsFile => '歌詞ファイル';

  @override
  String get visualLyricsImportHint => '歌詞ファイルを読み込むか、オンラインで検索してください。';

  @override
  String get visualLyricsLoading => '歌詞を読み込み中…';

  @override
  String visualLyricsMatch(String album, String kind, int seconds) {
    return '$album\n$kind · $seconds 秒';
  }

  @override
  String get visualLyricsMissing => '歌詞が見つかりません。ファイルを読み込むか再検索してください。';

  @override
  String get visualLyricsNotFound => '歌詞が見つかりません。曲名やアーティストを変えてみてください。';

  @override
  String get visualLyricsOnPlay => '再生中に歌詞を自動で読み込む';

  @override
  String get visualLyricsParseFailure => '歌詞を解析できませんでした。もう一度読み込んでください。';

  @override
  String get visualLyricsReadFailure => '歌詞を読み込めませんでした。手動で取り込むか再試行してください。';

  @override
  String get visualLyricsServiceFailure =>
      '歌詞サービスに接続できませんでした。後で再試行するか、ローカルの歌詞を読み込んでください。';

  @override
  String get visualLyricsSize => '歌詞ファイルは 1 MB 以下にしてください。';

  @override
  String get visualLyricsSources => 'ローカルファイル → 埋め込み → LRCLIB';

  @override
  String get visualLyricsVersions => '複数のバージョンが見つかりました。検索画面で選んでください。';

  @override
  String get visualMaterialPreview => '質感のプレビュー';

  @override
  String get visualMaximize => '最大化';

  @override
  String get visualMediaAddress => 'メディアのアドレス';

  @override
  String get visualMediaAddressInvalid =>
      'ログイン情報を含まない有効な HTTP / HTTPS アドレスを入力してください。';

  @override
  String get visualMediaPreviewFailure =>
      'このメディアはプレビューできません。保存して別のアプリで開いてください。';

  @override
  String get visualMediaType => 'メディアの種類';

  @override
  String get visualMinimize => '最小化';

  @override
  String get visualMusic => '音楽';

  @override
  String get visualMusicEmptyTitle => '音楽にも居場所を';

  @override
  String get visualMusicPlayer => '音楽プレーヤー';

  @override
  String get visualNextTrack => '次の曲';

  @override
  String get visualNoLyricsRead => '歌詞が読み込まれていません';

  @override
  String visualNoLyricsTitle(String title) {
    return '♪ $title · 歌詞なし';
  }

  @override
  String get visualNoTimeline => '時間情報なし';

  @override
  String get visualOpacity => '不透明度';

  @override
  String get visualOptionalArtist => 'アーティスト（省略可）';

  @override
  String get visualPauseMusic => '音楽を一時停止';

  @override
  String get visualPaused => '一時停止中';

  @override
  String get visualPlainLyrics => '通常の歌詞';

  @override
  String get visualPlayMusic => '音楽を再生';

  @override
  String get visualPlaybackFailure => '曲を再生できません。ファイルを確認するか、別の音声形式を試してください。';

  @override
  String visualPlaybackPosition(int count, int index, String state) {
    return '$index / $count · $state';
  }

  @override
  String get visualPlaybackRequestFailure => '再生を開始できませんでした。再試行してください。';

  @override
  String get visualPlaying => '再生中';

  @override
  String get visualPlaylistEmpty => 'プレイリストは空です';

  @override
  String get visualPlaylistLyricsHint => 'プレイリストのメニューから LRC 歌詞を追加';

  @override
  String get visualPlaylistSaved => 'プレイリストと歌詞は自動保存されます';

  @override
  String get visualPlaylistUpdateFailure => 'プレイリストを更新できませんでした。再試行してください。';

  @override
  String get visualPreviewColor => '色をプレビュー';

  @override
  String get visualPreviousTrack => '前の曲';

  @override
  String get visualRemoveAttachment => '添付ファイルを削除';

  @override
  String get visualRemoveTrack => 'プレイリストから削除';

  @override
  String get visualResetMaterial => 'テーマに戻す';

  @override
  String get visualRestoreWindow => '元のサイズに戻す';

  @override
  String get visualSaveAttachment => '添付ファイルを別名で保存';

  @override
  String get visualSearch => '検索';

  @override
  String get visualSearchLyrics => '歌詞を検索';

  @override
  String get visualSongCover => 'アルバムアート';

  @override
  String get visualSongTitle => '曲名';

  @override
  String get visualSyncedLyrics => '同期歌詞';

  @override
  String get visualTextureFailure =>
      'メディアを読み込めませんでした。ファイル、アドレス、形式を確認してください。Web メディアにはクロスオリジンアクセスの許可も必要です。';

  @override
  String get visualTextureLinkGuide =>
      '画像、GIF、動画の直接の HTTP / HTTPS リンクを貼り付けてください。共有ページの場合は元のメディア URL を探してください。';

  @override
  String get visualTextureLinkTitle => 'インスピレーションを取り込む';

  @override
  String get visualTexturePlaybackGuide =>
      '動画は既定で無音ループ再生します。音声は設定で有効にできます。オンラインメディアは Web でのクロスオリジン読み込みを含め、アクセスを許可する必要があります。';

  @override
  String get visualTipsMaterialGuide =>
      'オフではヒントは透明、オンでは下の質感を使用します。個別の値は保持されます。';

  @override
  String get visualTransparentTips => '透明オーバーレイ（既定）';

  @override
  String get visualUseCustomMaterial => '質感を個別に設定';

  @override
  String get visualVideo => '動画';

  @override
  String get visualViewLyrics => '歌詞を表示';
}
