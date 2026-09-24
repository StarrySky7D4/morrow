// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for Korean (`ko`).
class AppLocalizationsKo extends AppLocalizations {
  AppLocalizationsKo([String locale = 'ko']) : super(locale);

  @override
  String get commonAppName => 'Morrow';

  @override
  String get commonCancel => '취소';

  @override
  String commonCount(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count개 항목',
      zero: '항목 없음',
    );
    return '$_temp0';
  }

  @override
  String commonGreeting(String name) {
    return '안녕하세요, $name님';
  }

  @override
  String get importsAttachmentLimit =>
      '한 번에 최대 20개를 가져올 수 있습니다. 나머지는 나눠서 붙여넣으세요.';

  @override
  String get importsClipboardChanged => '읽는 동안 클립보드가 변경되었습니다. 다시 붙여넣으세요.';

  @override
  String get importsEmbeddedImageUnreadable => '내장 이미지를 읽지 못했습니다.';

  @override
  String get importsEmbeddedImagesSeparate => '일부 내장 이미지는 별도 파일로 가져와야 합니다.';

  @override
  String get importsExcelValues =>
      '값과 수식을 Markdown으로 변환했습니다. 서식과 병합 셀은 XML 첨부에 보존됩니다.';

  @override
  String get importsExcelXmlKept => '원본 Excel 표를 XML 첨부로 보존했습니다.';

  @override
  String get importsFileTooLarge => '클립보드 파일이 200 MB를 초과합니다.';

  @override
  String get importsItemLimit => '처음 20개 항목만 읽었습니다. 나머지는 따로 붙여넣으세요.';

  @override
  String get importsItemUnreadable =>
      '클립보드 항목 하나를 읽지 못했습니다. 나머지 읽을 수 있는 내용은 보존했습니다.';

  @override
  String get importsLocalImageNotRead =>
      '로컬 연결 이미지는 자동으로 읽지 않습니다. 이미지를 붙여넣거나 원본 파일을 가져오세요.';

  @override
  String get importsMergedTable =>
      '병합 셀을 읽을 수 있는 표로 변환했습니다. 원래 서식은 HTML 첨부에 보존됩니다.';

  @override
  String get importsOfficeBusy => '다른 앱이 클립보드를 사용 중입니다. Office 개체를 읽지 않았습니다.';

  @override
  String get importsOfficeEmbeddedKept =>
      'Office 내장 개체는 원본 첨부로 보존합니다. 차트, 수식, 레이아웃은 원래 앱에서 편집하세요.';

  @override
  String get importsOfficeExportFailed =>
      'Office 개체가 제한을 초과했거나 내보내지 못했습니다. 원래 앱에서 저장한 뒤 가져오세요.';

  @override
  String get importsOfficeReadFailed =>
      'Office 내용을 읽지 못했습니다. 다른 클립보드 내용은 사용할 수 있습니다.';

  @override
  String get importsOfficeUnavailable => 'Office 클립보드를 일시적으로 사용할 수 없습니다.';

  @override
  String get importsOfficeUnreadable =>
      '원본 Office 개체를 읽지 못했습니다. 다른 사용 가능한 내용은 보존했습니다.';

  @override
  String get importsRichFallback => '일부 서식을 변환하지 못했습니다. 읽을 수 있는 텍스트는 보존했습니다.';

  @override
  String get importsRichTooLarge => '서식 있는 텍스트가 2 MB를 초과합니다. 문서를 첨부로 가져오세요.';

  @override
  String get importsRtfTooLarge => 'RTF 내용이 너무 큽니다. 원본 문서를 가져오세요.';

  @override
  String get importsSpreadsheetTooLarge => '표가 너무 큽니다. Excel 파일을 가져오세요.';

  @override
  String get importsTableConverted =>
      '표를 Markdown으로 변환했습니다. 전체 데이터는 TSV 첨부로 보존됩니다.';

  @override
  String get importsTextTooLarge => '텍스트가 2 MB를 초과합니다. 파일로 가져오세요.';

  @override
  String get importsTotalTooLarge => '붙여넣는 파일의 합계가 200 MB를 초과합니다. 나눠서 가져오세요.';

  @override
  String get importsUnsupported => '여기서는 클립보드를 지원하지 않습니다. 파일을 가져오세요.';

  @override
  String get mainActiveProjects => '진행 중';

  @override
  String get mainAdjustCustomTone => '색조 조정';

  @override
  String get mainAmbientDetail => '흐르는 빛이 은은한 색을 더합니다.';

  @override
  String get mainAppTitle => 'Morrow — 아이디어를 위한 공간';

  @override
  String get mainAppearance => '모양';

  @override
  String get mainArrangeIdeas => '아이디어 정렬';

  @override
  String mainAttachmentCount(int count) {
    return '첨부 파일 · $count';
  }

  @override
  String mainAttachmentHint(int count, String name) {
    return '첨부 $count개 · $name';
  }

  @override
  String get mainAttachmentLimit => '기록당 첨부 파일은 최대 20개입니다.';

  @override
  String get mainAutosaveNotice => '모양과 아이디어는 로컬에 저장됩니다';

  @override
  String get mainAwaitDiscovery => '새로운 발견을 기다립니다';

  @override
  String get mainBackToWorkbench => '작업 공간으로 돌아가기';

  @override
  String get mainBackgroundCanvas => '배경 캔버스';

  @override
  String get mainBackgroundSound => '배경 소리 재생';

  @override
  String get mainBodyHint => '생각을 적거나 내용을 붙여넣으세요…\n\n# 제목, 목록, 표, 코드 블록 지원';

  @override
  String get mainBrightWhite => '흰색';

  @override
  String get mainBuiltinTexture => '기본 텍스처 사용';

  @override
  String get mainCanvasCompass => '캔버스 색상환';

  @override
  String get mainCaptureIdea => '아이디어 기록';

  @override
  String get mainCaptureNow => '생각 기록하기';

  @override
  String mainCardAttachments(int count, String name) {
    return '파일 $count개 · $name';
  }

  @override
  String get mainCategoryExperiment => '실험';

  @override
  String get mainCategoryIdea => '아이디어';

  @override
  String get mainCategoryProject => '프로젝트';

  @override
  String get mainCategoryPrompt => '보관 위치';

  @override
  String get mainChangeFailed => '변경 사항을 저장하지 못했습니다. 초안은 유지되며 다시 시도할 수 있습니다.';

  @override
  String get mainCheckAgain => '다시 확인';

  @override
  String get mainClearSearch => '검색 지우기';

  @override
  String get mainClipboardEmpty =>
      '클립보드에 읽을 수 있는 텍스트나 파일이 없습니다. 파일 관리자에서 파일을 복사하거나 파일 가져오기를 사용하세요.';

  @override
  String get mainClipboardReadFailed =>
      '내용을 읽지 못했습니다. 파일을 가져오거나 파일 및 클립보드 권한을 확인하세요.';

  @override
  String get mainClipboardSupport =>
      'Markdown, Office 서식 있는 텍스트와 표, 스크린샷, 파일을 지원합니다. 복잡한 개체는 원본 첨부로 보존합니다. 최대 20개, 각 200 MB까지 가능합니다.';

  @override
  String get mainCollapseSidebar => '사이드바 접기';

  @override
  String get mainCompletedProjects => '완료됨';

  @override
  String get mainComponentCompass => '구성 요소 색상환';

  @override
  String get mainComponentEmpty => '빈 상태';

  @override
  String get mainComponentFooter => '하단 팁과 가사';

  @override
  String get mainComponentHero => '개요 카드';

  @override
  String get mainComponentNavigation => '사이드바 탐색';

  @override
  String get mainComponentQuickCapture => '빠른 기록';

  @override
  String get mainComponentSearch => '검색창';

  @override
  String get mainComponentSettings => '구성 요소 및 카드 · 설정';

  @override
  String get mainContentCommittedRefreshFailed =>
      '작업이 저장되었지만 최신 내용을 불러오지 못했습니다. 새로고침하여 확인하세요.';

  @override
  String get mainContentProtection => '콘텐츠 보호';

  @override
  String get mainContentRead => '내용을 읽었습니다';

  @override
  String mainContentReadFiles(int count) {
    return '내용을 읽고 첨부 $count개를 보존했습니다';
  }

  @override
  String get mainCornerRadius => '모서리 반경';

  @override
  String get mainCredentialSettings => '자격 증명';

  @override
  String get mainCrystal => '맑게';

  @override
  String get mainCrystalDetail => '가볍고 맑게, 색이 빛나도록.';

  @override
  String get mainCuriosity => '흥미로운 일은\n작은 호기심에서 시작됩니다.';

  @override
  String get mainCustomCompass => '색상환 · 사용자 지정';

  @override
  String get mainCustomLightness => '사용자 지정 밝기';

  @override
  String get mainCustomTheme => '사용자 지정';

  @override
  String get mainDaily => '소소한 일상';

  @override
  String get mainDailyExplore => '10분 동안 탐색하기';

  @override
  String get mainDailyIdea => '아이디어 하나 적기';

  @override
  String get mainDailyWater => '물 한 잔 마시기';

  @override
  String get mainDarkTheme => '어둡게';

  @override
  String get mainDeepBlack => '검정';

  @override
  String get mainDefaultCanvas => '기본';

  @override
  String get mainDefaultGlobalColor => '기본 테마 색상 · 모든 컨트롤';

  @override
  String get mainDelete => '삭제';

  @override
  String mainDeleted(String title) {
    return '“$title” 삭제됨';
  }

  @override
  String get mainDiagnosticDetails => '진단 세부 정보';

  @override
  String get mainDone => '완료';

  @override
  String get mainDraftHandoffRecoveryAssets => '고정된 첨부 파일';

  @override
  String get mainDraftHandoffRecoveryCancel => '인계 취소';

  @override
  String get mainDraftHandoffRecoveryCancelBody =>
      '완료되지 않은 인계를 취소합니다. 원래 작업으로 후속 초안을 다시 만들 수 없게 됩니다. 상위 초안은 그대로 유지됩니다.';

  @override
  String get mainDraftHandoffRecoveryCancelled => '인계가 취소되었습니다';

  @override
  String get mainDraftHandoffRecoveryChildCommitted =>
      '후속 초안이 저장되었지만 상위 초안은 아직 종료되지 않았습니다';

  @override
  String get mainDraftHandoffRecoveryComplete => '후속 초안 만들기';

  @override
  String get mainDraftHandoffRecoveryCompleteBody =>
      '저장된 원래 제안으로 후속 초안을 만듭니다. 정식 카드를 다시 제출하지 않습니다.';

  @override
  String get mainDraftHandoffRecoveryConfirmTitle => '초안 인계 처리 확인';

  @override
  String get mainDraftHandoffRecoveryConfirmed => '작업이 확인되어 저장되었습니다.';

  @override
  String get mainDraftHandoffRecoveryConflict => '현재 상태가 변경되었습니다. 확인이 필요합니다';

  @override
  String get mainDraftHandoffRecoveryEmpty => '확인할 인계 기록이 없습니다';

  @override
  String get mainDraftHandoffRecoveryFailed =>
      '작업을 완료할 수 없습니다. 새로고침한 뒤 기록을 확인하세요.';

  @override
  String get mainDraftHandoffRecoveryFields => '원래 초안 내용';

  @override
  String get mainDraftHandoffRecoveryInspect => '보기 및 확인';

  @override
  String get mainDraftHandoffRecoveryIntro =>
      '저장된 인계 기록만 확인합니다. 이 페이지를 열어도 내용이 자동으로 제출되지 않습니다.';

  @override
  String get mainDraftHandoffRecoveryParentRetired => '인계가 완료되었습니다';

  @override
  String get mainDraftHandoffRecoveryPending => '후속 초안이 아직 생성되지 않았습니다';

  @override
  String get mainDraftHandoffRecoveryReadOnly => '현재 작업 공간은 보기만 허용됩니다';

  @override
  String get mainDraftHandoffRecoveryRetire => '상위 초안 종료';

  @override
  String get mainDraftHandoffRecoveryRetireBody =>
      '후속 초안이 저장되었음을 확인한 뒤 상위 초안을 종료 상태로 표시합니다.';

  @override
  String get mainDraftHandoffRecoverySelection => '기록을 선택하여 전체 초안을 확인하세요.';

  @override
  String get mainDraftHandoffRecoveryTitle => '초안 인계 복구';

  @override
  String get mainDraftHandoffRecoveryUnknown =>
      '작업 결과가 아직 확인되지 않았습니다. 원래 기록을 확인하고 작업을 반복하지 마세요.';

  @override
  String get mainDraftImportRecoveryCancelBody =>
      '취소하면 원래 포기 요청은 더 이상 실행되지 않습니다. 첨부 파일이 삭제되거나 다른 변경 사항이 되돌려지지는 않습니다.';

  @override
  String get mainDraftImportRecoveryCancelDecision => '포기 요청 취소';

  @override
  String get mainDraftImportRecoveryCancelTitle => '이 포기 요청을 취소할까요?';

  @override
  String get mainDraftImportRecoveryCancelled =>
      '취소됨: 원래 포기 요청은 더 이상 실행되지 않습니다.';

  @override
  String get mainDraftImportRecoveryClose => '닫기';

  @override
  String get mainDraftImportRecoveryCommitted => '완료: 이 첨부 파일 가져오기를 포기했습니다.';

  @override
  String get mainDraftImportRecoveryConfirmedRefreshFailed =>
      '작업은 확인되었지만 목록이 새로고침되지 않았습니다. 새로고침하여 최신 상태를 확인하세요.';

  @override
  String get mainDraftImportRecoveryConflict =>
      '충돌: 초안이 변경되었습니다. 이 요청은 취소만 할 수 있습니다.';

  @override
  String get mainDraftImportRecoveryEmpty => '검토할 첨부 파일 결정이 없습니다.';

  @override
  String get mainDraftImportRecoveryFailed =>
      '이 결정을 검토할 수 없습니다. 새로고침한 뒤 다시 시도하세요.';

  @override
  String get mainDraftImportRecoveryPending => '대기 중: 원래 포기 요청이 확인되지 않았습니다.';

  @override
  String get mainDraftImportRecoveryRetry => '원래 포기 요청 다시 시도';

  @override
  String get mainDraftImportRecoveryTitle => '첨부 파일 결정 검토';

  @override
  String get mainEdit => '편집';

  @override
  String get mainEditIdeaTitle => '아이디어 다듬기';

  @override
  String get mainEditorClosedUnknown =>
      '편집기가 닫혔지만 저장 결과는 확인되지 않았습니다. 다른 사본을 만들기 전에 작업 공간을 다시 열어 확인하세요.';

  @override
  String get mainEditorContinueDraft => '계속 편집';

  @override
  String get mainEditorContinueFailed =>
      '이전 편집은 확정되었고 새 초안은 이 창에 남아 있습니다. 다음 편집기를 열지 못했습니다. 다시 시도하세요.';

  @override
  String get mainEditorNewerDraft => '이전 편집이 저장되었습니다. 새 초안은 아직 저장되지 않았습니다.';

  @override
  String get mainEditorPendingDraft =>
      '계속 입력할 수 있습니다. 이전 저장을 먼저 확인하세요. 새 편집 내용은 자동으로 전송되지 않습니다.';

  @override
  String get mainEditorRecoveryAbandonBody =>
      '확정되지 않은 이 편집을 폐기할까요? 원래 작업은 다시 실행할 수 없게 됩니다. 저장된 내용과 새 초안은 변경되지 않습니다. 이미 확정된 편집은 취소되지 않습니다.';

  @override
  String get mainEditorRecoveryAbandonTitle => '이전 편집 폐기';

  @override
  String get mainEditorRecoveryCommitted =>
      '원래 편집이 저장되었습니다. 현재 내용을 확인하고 승인합니다. 다시 저장하지 않습니다.';

  @override
  String get mainEditorRecoveryConfirm => '확인 및 승인';

  @override
  String get mainEditorRecoveryConflict =>
      '기준 내용 또는 작업 기록이 변경되었습니다. 원래 제안은 유지되며 현재 내용을 덮어쓸 수 없습니다.';

  @override
  String get mainEditorRecoveryEmpty => '확인할 저장된 편집 제안이 없습니다.';

  @override
  String get mainEditorRecoveryFailed =>
      '확인을 완료하지 못했습니다. 원래 제안은 유지됩니다. 새로 고친 후 다시 시도하세요.';

  @override
  String get mainEditorRecoveryPending =>
      '원래 편집이 확인되지 않았습니다. 계속하면 해당 제안만 다시 시도하며 새 변경 사항은 보내지 않습니다.';

  @override
  String get mainEditorRecoveryResume => '원래 편집 계속';

  @override
  String get mainEditorRecoveryTitle => '미확인 편집 확인';

  @override
  String get mainEditorSubtitle => '글, 표, 이미지. 아이디어가 형태를 갖출 때까지 여기에 담으세요.';

  @override
  String get mainEditorUnavailable =>
      '편집기를 사용할 수 없습니다. 콘텐츠 서비스를 확인한 뒤 다시 시도하세요.';

  @override
  String get mainEndpointSettings => '아웃바운드 엔드포인트';

  @override
  String get mainExpandSettings => '설정 펼치기';

  @override
  String get mainExpandSidebar => '사이드바 펼치기';

  @override
  String get mainExtensionPlugins => '확장 기능';

  @override
  String get mainFavoriteAttachments => '즐겨찾는 첨부 파일';

  @override
  String get mainFavoriteRecords => '즐겨찾는 기록';

  @override
  String mainFavoriteTooltip(String title) {
    return '$title 즐겨찾기 추가';
  }

  @override
  String get mainFavoritesIntro => '즐겨찾는 글, 이미지, 파일을 한곳에 모으세요.';

  @override
  String mainFieldLimit(int limit) {
    return '최대 $limit자입니다. 내용을 줄이거나 파일로 가져오세요.';
  }

  @override
  String get mainFilterAll => '전체';

  @override
  String get mainFilterAttachments => '파일 있음';

  @override
  String get mainFilterFavorites => '즐겨찾기만';

  @override
  String get mainFilterFile => '파일';

  @override
  String get mainFilterImage => '이미지';

  @override
  String get mainFilterMedia => '오디오 / 동영상';

  @override
  String get mainFilterPending => '할 일';

  @override
  String get mainFilterText => '텍스트';

  @override
  String get mainFollowTheme => '테마 따르기';

  @override
  String get mainFontApply => '글꼴 적용';

  @override
  String get mainFontDefault => '기본 글꼴';

  @override
  String get mainFontFailed => '글꼴을 불러올 수 없습니다. 파일을 확인하거나 앱을 다시 시작한 후 재시도하세요.';

  @override
  String get mainFontFamily => '시스템 글꼴 이름';

  @override
  String get mainFontHelp =>
      'TTF / OTF, 최대 20 MiB. 설치되지 않은 시스템 글꼴은 자동으로 대체됩니다.';

  @override
  String get mainFontHint => '예: Arial 또는 Microsoft YaHei';

  @override
  String get mainFontImport => '글꼴 파일 가져오기';

  @override
  String get mainFontReset => '기본값 복원';

  @override
  String get mainFontSettings => '글꼴';

  @override
  String get mainFontUnavailable => '저장된 글꼴을 사용할 수 없어 기본 글꼴을 임시로 사용합니다.';

  @override
  String get mainFrostDetail => '배경을 부드럽게 하고 생각에 여유를 주세요.';

  @override
  String get mainFrostEffect => '흐림';

  @override
  String get mainFrostOpacity => '유리 불투명도';

  @override
  String get mainFrostUnavailable =>
      '바탕 화면 흐림 효과를 사용할 수 없습니다. 색조와 불투명도는 조정할 수 있습니다.';

  @override
  String get mainFrosted => '반투명';

  @override
  String get mainGlassTexture => '유리 스타일';

  @override
  String mainGlobalColor(String color) {
    return '$color · 모든 컨트롤';
  }

  @override
  String get mainGreeting => '아이디어를 키워 보세요.';

  @override
  String get mainGreetingDetail => '일상의 작은 발견과 영감의 순간을 담으세요.';

  @override
  String get mainHeroBody => '생각 하나, 작은 할 일, 문득 떠오른 가능성.\n여기에서 시작됩니다.';

  @override
  String get mainHeroCaption => '가능성의 공간';

  @override
  String get mainHeroTitle => '작게 시작해도 괜찮아요.';

  @override
  String get mainHideAppearance => '모양 설정 숨기기';

  @override
  String get mainHideCustomTone => '사용자 지정 색조 숨기기';

  @override
  String get mainHidePreview => '미리보기 숨기기';

  @override
  String get mainHttpSettings => 'HTTP 작업';

  @override
  String get mainHypothesis => '가설';

  @override
  String get mainHypothesisPrompt => '검증할 가설';

  @override
  String get mainHypothesisSection => '가설 / 시도할 것';

  @override
  String get mainIdeaDetails => '세부 내용을 남기고 다음 단계를 명확히 하세요.';

  @override
  String get mainIdeaNameHint => '이름을 지어 주세요';

  @override
  String get mainIdeaNameRequired => '먼저 아이디어를 적어 주세요';

  @override
  String get mainIdeaSaved => '아이디어를 저장했습니다.';

  @override
  String get mainImportFailed => '미디어를 가져오지 못했습니다. 파일과 저장 공간을 확인하세요.';

  @override
  String get mainImportFile => '파일 가져오기';

  @override
  String get mainInboxIntro => '먼저 기록하고 나중에 정리하세요. 좋은 아이디어를 작은 프로젝트로 키워 보세요.';

  @override
  String get mainIoNoDeclarations => '파일 또는 네트워크 접근을 선언한 플러그인이 없습니다.';

  @override
  String get mainIoSettings => '네트워크 및 파일';

  @override
  String get mainIoSettingsGuide =>
      '모양 설정과 별도로 플러그인의 파일 및 네트워크 권한을 관리합니다. 기능을 승인해도 모든 파일이나 엔드포인트에 접근할 수 있는 것은 아닙니다. 가능한 작업은 현재 백엔드에 따라 달라집니다.';

  @override
  String get mainIoSettingsSummary => '권한, 자격 증명, 엔드포인트 및 API 서비스';

  @override
  String get mainJustNow => '방금';

  @override
  String get mainLabIntro => '가설로 시작하세요. 시도, 관찰, 뜻밖의 발견을 남기세요.';

  @override
  String get mainLanguage => '언어';

  @override
  String get mainLanguageChinese => '简体中文';

  @override
  String get mainLanguageEnglish => 'English';

  @override
  String get mainLanguageSystem => '시스템 기본값';

  @override
  String get mainLavender => '라벤더';

  @override
  String get mainLegacyStageComplete =>
      '이전 형식은 모든 할 일을 완료로 표시하고 프로젝트를 완료합니다. 계속할까요?';

  @override
  String mainLegacyStageReopen(String task) {
    return '다른 단계로 돌아가면 마지막 할 일 “$task”와 같은 이름의 모든 할 일이 미완료로 바뀝니다. 계속할까요?';
  }

  @override
  String get mainLegacyTodoContinue => '계속';

  @override
  String mainLegacyTodoGroup(String task) {
    return '이전 형식은 텍스트로 할 일을 구분합니다. 이름이 “$task”인 모든 항목이 바뀝니다. 계속할까요?';
  }

  @override
  String get mainLegacyTodoTitle => '이전 형식의 할 일 연동';

  @override
  String get mainLightOpacity => '20% · 옅게';

  @override
  String get mainLiquidAllCanvases => '네 가지 배경 유형 모두에서 개별적으로 사용할 수 있습니다';

  @override
  String get mainLiquidDetail => '공중에 떠 있는 물방울처럼 흐르는 빛과 부드러운 굴절.';

  @override
  String get mainLiquidEffect => '리퀴드 글래스 효과';

  @override
  String get mainLiquidGlass => '리퀴드 글래스';

  @override
  String get mainLivePreview => '실시간 미리보기';

  @override
  String get mainLocalMedia => '로컬 미디어';

  @override
  String get mainMakeYours => '나만의 스타일';

  @override
  String get mainMarkOrganized => '정리됨으로 표시';

  @override
  String get mainMarkdownBody => '본문 · Markdown';

  @override
  String get mainMediaLimits => '이미지 / GIF ≤ 25 MB, 동영상 ≤ 150 MB';

  @override
  String get mainMonochrome => '단색';

  @override
  String mainMoreSteps(int count) {
    return '$count개 단계 더 있음. 열어서 보기';
  }

  @override
  String mainMovedProject(String title) {
    return '“$title”을(를) 프로젝트로 이동했습니다';
  }

  @override
  String get mainMusic => '음악 플레이어';

  @override
  String get mainMySpace => '내 공간';

  @override
  String get mainNavigation => '탐색';

  @override
  String get mainNewIdea => '새 아이디어';

  @override
  String get mainNewIdeaTitle => '새 아이디어 담기';

  @override
  String get mainNoHypothesis => '아직 가설이 없습니다';

  @override
  String get mainNoMatches => '일치하는 아이디어가 없습니다';

  @override
  String get mainNoResultYet => '결과는 나중에 나와도 괜찮아요. 과정도 기록할 가치가 있습니다.';

  @override
  String get mainNotNow => '나중에';

  @override
  String get mainObservationSection => '관찰 / 발견한 것';

  @override
  String get mainObservations => '관찰 및 결론';

  @override
  String get mainObservationsPrompt => '관찰, 과정 및 결론';

  @override
  String get mainOneHourAgo => '1시간 전';

  @override
  String get mainOnlineMedia => '온라인 미디어';

  @override
  String get mainOpaqueFallback => '현재 테마 색상 위에 맑은 패널을 표시합니다.';

  @override
  String get mainOpenNextStep => '프로젝트를 열어 다음 단계 편집';

  @override
  String get mainOrganizedCount => '정리됨';

  @override
  String get mainOriginalColors => '원래 색상';

  @override
  String get mainPageFavorites => '즐겨찾기';

  @override
  String get mainPageInbox => '수집함';

  @override
  String get mainPageLaboratory => '실험실';

  @override
  String get mainPageOverview => '개요';

  @override
  String get mainPageProjects => '프로젝트';

  @override
  String mainPageSummary(String page) {
    return '$page · 개요';
  }

  @override
  String get mainPasteChanged => '붙여넣는 동안 입력이 변경되었습니다. 편집기를 다시 여세요.';

  @override
  String get mainPasteContent => '내용 붙여넣기';

  @override
  String get mainPause => '일시 정지';

  @override
  String get mainPersonalWorkspace => '개인 작업 공간';

  @override
  String get mainPlay => '재생';

  @override
  String get mainPluginSettings => '플러그인 및 서비스';

  @override
  String get mainPluginSettingsSummary => '기본 도구, 확장 기능, 네트워크, 파일 및 콘텐츠 보호';

  @override
  String get mainPreviewEmpty => '여기에 미리보기가 표시됩니다';

  @override
  String mainProgress(int done, int total) {
    return '작은 걸음 · $done/$total';
  }

  @override
  String get mainProjectIntro => '체크리스트로 진행하세요. 작은 걸음마다 목표에 가까워집니다.';

  @override
  String get mainQueryAgain => '새 쿼리 실행';

  @override
  String get mainQueryCapacity => '쿼리 기록이 가득 찼습니다';

  @override
  String get mainQueryCapacityDetail =>
      '내용은 보존됩니다. 이 버전에서는 아직 쿼리 기록을 지울 수 없습니다.';

  @override
  String get mainQueryLoading => '아이디어 찾는 중…';

  @override
  String get mainQueryRetry => '쿼리 다시 시도';

  @override
  String get mainQueryTerminated => '이 쿼리는 종료되었습니다';

  @override
  String get mainQueryUnknown => '결과가 아직 확인되지 않았습니다';

  @override
  String get mainQuickHint => '방금 무엇이 떠올랐나요?';

  @override
  String get mainReadOnlySettings =>
      '콘텐츠가 읽기 전용입니다. 편집을 복원하려면 작업 공간 플러그인을 확인하세요.';

  @override
  String get mainRecentThoughts => '최근 아이디어';

  @override
  String get mainRecordedCount => '관찰 기록 있음';

  @override
  String get mainRestoreDefault => '기본값 복원';

  @override
  String get mainRetry => '다시 시도';

  @override
  String get mainRetrySave => '저장 다시 시도';

  @override
  String get mainSage => '세이지';

  @override
  String get mainSampleBody0 => '문득 떠오른 생각을 여기에.\n완성을 서두르지 말고 시작해 보세요.';

  @override
  String get mainSampleBody1 => '좋아하는 글귀와 음악,\n일상의 작은 순간을 위한 페이지.';

  @override
  String get mainSampleBody2 => '생성 예술에 도전해 보세요. 코드가\n뜻밖의 형태로 자라나게 하세요.';

  @override
  String get mainSampleBody3 => '놓치기 쉬운 작은 일을\n기억하게 돕는 조용한 친구.';

  @override
  String get mainSampleTitle0 => '아이디어를 위한 집';

  @override
  String get mainSampleTitle1 => '조용한 디지털 정원';

  @override
  String get mainSampleTitle2 => '재미로 무언가 만들기';

  @override
  String get mainSampleTitle3 => '나의 작은 도우미';

  @override
  String get mainSampleTodo0 => '첫 모음 정리하기';

  @override
  String get mainSampleTodo1 => '정원 입구 디자인하기';

  @override
  String get mainSampleTodo2 => '새 아이디어 심기';

  @override
  String get mainSampleTodo3 => '작은 시제품 구상하기';

  @override
  String get mainSampleTodo4 => '알림 동작 설계하기';

  @override
  String get mainSaveConnectionUnknown =>
      '연결이 끊겨 저장 결과를 알 수 없습니다. 다시 시도하기 전에 라이브러리를 다시 열어 확인하세요.';

  @override
  String get mainSaveFailed => '저장하지 못했습니다. 변경 사항은 이번 세션에 유지됩니다.';

  @override
  String get mainSaveIdea => '아이디어 저장';

  @override
  String get mainSaveNotSubmitted =>
      '제출되지 않았습니다. 초안과 첨부 파일은 보존되며 수정 후 다시 저장할 수 있습니다.';

  @override
  String get mainSaveReadOnly =>
      '변경 사항이 저장되지 않았습니다. 플러그인 및 서비스에서 작업 공간 플러그인을 켜고 다시 시도하세요.';

  @override
  String get mainSaveReadbackPending =>
      '설정은 저장되었지만 다시 읽기는 확인되지 않았습니다. 초안을 유지하며 재시도 시 원래 제출을 먼저 확인합니다.';

  @override
  String get mainSaveRecoveryAbandon => '이전 제안 버리기';

  @override
  String get mainSaveRecoveryAbandonConfirm =>
      '저장되지 않은 제안만 버릴까요? 현재 초안과 저장된 내용은 유지됩니다. 이미 저장된 설정은 취소되지 않습니다.';

  @override
  String get mainSaveRecoveryCommitted =>
      '이전 설정은 저장되었습니다. 확인해도 현재 초안은 바뀌지 않습니다.';

  @override
  String get mainSaveRecoveryConflict =>
      '라이브러리가 변경되었습니다. 이전 제안으로 새 설정을 덮어쓸 수 없습니다. 보류하거나 저장되지 않은 제안을 버리세요.';

  @override
  String get mainSaveRecoveryDone =>
      '이전 제안을 처리했습니다. 초안은 그대로입니다. 준비되면 저장을 다시 시도하세요.';

  @override
  String get mainSaveRecoveryEmpty => '확인할 영구 저장 제안이 없습니다.';

  @override
  String get mainSaveRecoveryPending =>
      '이전 제안의 저장이 확인되지 않았습니다. 계속하면 원래 저장을 확인하고 시도합니다. 새 초안은 유지됩니다.';

  @override
  String get mainSaveRecoveryResolve => '원래 저장 확인';

  @override
  String get mainSaveRecoveryReview => '저장 확인';

  @override
  String get mainSaveRecoveryTitle => '확인이 필요한 저장이 있습니다';

  @override
  String get mainSaveUnknown =>
      '저장 결과가 확인되지 않았습니다. 초안과 첨부 파일은 보존됩니다. 이 제출을 다시 시도하세요. 닫으면 작업 공간을 새로 고쳐 확인합니다.';

  @override
  String get mainSaving => '저장 중…';

  @override
  String get mainSearchHint => '아이디어 검색…';

  @override
  String get mainServiceRunSettings => '서비스 실행';

  @override
  String get mainServiceSettings => 'API 서비스';

  @override
  String get mainSettings => '설정';

  @override
  String get mainShowAppearance => '모양 설정 표시';

  @override
  String get mainSidebarMotto => '조금의 정리, 발견을 위한 여유.';

  @override
  String get mainSlowProgress => '작은 걸음도 앞으로 나아갑니다.';

  @override
  String get mainSolidCanvas => '단색';

  @override
  String get mainSolidDetail => '차분한 단색 배경.';

  @override
  String get mainSolidOpacity => '100% · 불투명';

  @override
  String get mainSortFavorites => '즐겨찾기 우선';

  @override
  String get mainSortRecent => '최근 추가순';

  @override
  String get mainSortTitle => '제목순';

  @override
  String get mainSquareCorners => '0으로 설정하면 직각 모서리가 됩니다';

  @override
  String get mainStageActive => '진행 중';

  @override
  String get mainStageCompleted => '완료됨';

  @override
  String get mainStageOrganized => '정리됨';

  @override
  String get mainStagePlanned => '계획됨';

  @override
  String get mainStageRecorded => '기록됨';

  @override
  String mainStageTooltip(String title) {
    return '$title 단계 변경';
  }

  @override
  String get mainStageUnsorted => '정리 대기';

  @override
  String get mainStageUnverified => '검증 대기';

  @override
  String get mainStageVerifying => '검증 중';

  @override
  String get mainStayCurious => '호기심을 잃지 말고, 나답게.';

  @override
  String mainSteps(int done, int total) {
    return '$done/$total단계';
  }

  @override
  String get mainStorageUnavailable =>
      '로컬 저장소를 사용할 수 없습니다. 변경 사항은 이번 세션에만 유지됩니다.';

  @override
  String get mainStorageUnreadable =>
      '저장된 내용을 읽지 못했습니다. 원본 데이터는 보존되며 덮어쓰지 않습니다.';

  @override
  String get mainStyleBrutalist => '브루탈리즘';

  @override
  String get mainStyleBrutalistDescription => '각진 모서리, 굵은 테두리와 어긋난 그림자';

  @override
  String get mainStyleClay => '클레이';

  @override
  String get mainStyleClayDescription => '둥근 형태와 부드럽게 떠오른 색면';

  @override
  String get mainStyleDepth => '입체 깊이';

  @override
  String get mainStyleDepthGuide => '유리 불투명도를 바꾸지 않고 돌출과 함몰의 강도를 조절합니다.';

  @override
  String get mainStyleDepthReset => '100%로 초기화';

  @override
  String get mainStyleExperimental => '실험적';

  @override
  String get mainStyleFlat => '플랫 · 기본';

  @override
  String get mainStyleFlatDescription => '가벼운 테두리와 명확한 계층';

  @override
  String get mainStyleFluent => 'Fluent';

  @override
  String get mainStyleFluentDescription => '반투명한 층과 강조색 선';

  @override
  String get mainStyleIndustrial => '인더스트리얼';

  @override
  String get mainStyleIndustrialDescription => '금속 느낌의 패널, 간결한 컨트롤과 정교한 선';

  @override
  String get mainStyleNeumorphism => '뉴모피즘';

  @override
  String get mainStyleNeumorphismDescription => '부드러운 빛과 그림자의 입체감';

  @override
  String get mainStylePaper => '페이퍼';

  @override
  String get mainStylePaperDescription => '무광 종이 표면, 얇은 테두리와 절제된 깊이감';

  @override
  String get mainTaskAdd => '작업 추가';

  @override
  String get mainTaskAmbiguousDecision =>
      '이름이 같은 이전 형식 작업의 완료 상태가 불확실합니다. 각 작업을 개별적으로 확인하세요.';

  @override
  String get mainTaskCompleteAllAndSetStage => '모두 완료하고 단계 설정';

  @override
  String mainTaskCompleteAllConfirm(String stage) {
    return '모든 작업을 완료로 표시하고 단계를 “$stage”(으)로 설정할까요?';
  }

  @override
  String get mainTaskLegacyReadOnly =>
      '이전 형식은 텍스트로 작업을 구분합니다. 업그레이드 후 같은 이름의 작업을 개별적으로 관리할 수 있습니다.';

  @override
  String get mainTaskMarkComplete => '완료로 확인';

  @override
  String get mainTaskMarkIncomplete => '미완료로 확인';

  @override
  String get mainTaskMoveDown => '아래로 이동';

  @override
  String get mainTaskMoveUp => '위로 이동';

  @override
  String mainTaskProgressThreeWay(int ambiguous, int complete, int incomplete) {
    return '완료 $complete · 미완료 $incomplete · 확인 필요 $ambiguous';
  }

  @override
  String mainTaskRemoveConfirm(String task) {
    return '작업 “$task”을(를) 삭제할까요?';
  }

  @override
  String get mainTaskRename => '이름 변경';

  @override
  String get mainTaskSetStage => '단계만 변경';

  @override
  String get mainTaskStagePrompt => '프로젝트 단계';

  @override
  String get mainTaskTextPrompt => '작업 내용';

  @override
  String get mainTenMinutesAgo => '10분 전';

  @override
  String get mainTextureCanvas => '텍스처';

  @override
  String get mainTextureDetail => '고운 종이 질감이 촉감을 더합니다.';

  @override
  String get mainThemeCompass => '테마 색상환';

  @override
  String get mainThemeGrayscale => '테마 회색조';

  @override
  String get mainThemeTone => '테마 색상';

  @override
  String get mainThreeHoursAgo => '3시간 전';

  @override
  String get mainTintOpacity => '색조 불투명도';

  @override
  String get mainToProject => '프로젝트로 이동';

  @override
  String get mainTodosPrompt => '다음 단계(한 줄에 하나, 선택 사항)';

  @override
  String get mainTransparencyUnavailable =>
      '시스템 투명 효과를 켤 수 없습니다. 기본 배경을 사용할 수 있습니다.';

  @override
  String get mainTransparentCanvas => '투명';

  @override
  String get mainTransparentDetail => '창 뒤의 공간을 보여 줍니다. 웹에서는 호스트 배경을 표시합니다.';

  @override
  String get mainUndo => '실행 취소';

  @override
  String mainUnfavoriteTooltip(String title) {
    return '$title 즐겨찾기 해제';
  }

  @override
  String get mainUnsortedCount => '정리할 항목';

  @override
  String get mainUnverifiedCount => '검증 대기';

  @override
  String get mainView => '보기';

  @override
  String get mainViewAll => '모두 보기';

  @override
  String get mainVisualStyle => '인터페이스 스타일';

  @override
  String get mainWarmSand => '따뜻한 모래';

  @override
  String get mainWhiteTheme => '밝게';

  @override
  String get mainWindowRadius => '창 모서리';

  @override
  String get mainWindowRadiusDetail => '창 테두리를 별도로 조정합니다. 최대화하면 모서리가 직각이 됩니다';

  @override
  String get mainWindowsFrostOnly => '바탕 화면 흐림은 Windows에서만 지원됩니다';

  @override
  String get mainWorkbench => '작업 공간';

  @override
  String get mainWorkbenchPlugin => '작업 공간 플러그인';

  @override
  String get mainWriteHypothesis => '기록을 열고 검증하고 싶은 내용을 적으세요.';

  @override
  String get mainYesterday => '어제';

  @override
  String get pluginsApprovalUnknown => '활성화 또는 권한 변경을 확인하지 못했습니다';

  @override
  String get pluginsApproveEnable => '승인하고 활성화';

  @override
  String get pluginsApproveWorkbench => '읽기와 편집을 허용하고 활성화';

  @override
  String get pluginsAttachment => '첨부 파일 읽기';

  @override
  String get pluginsBackingUp => '백업 중…';

  @override
  String get pluginsBackupLibrary => '라이브러리 백업';

  @override
  String get pluginsBackupLibraryType => '라이브러리 백업';

  @override
  String get pluginsBackupProtection => '보호 파일 백업';

  @override
  String get pluginsBackupUnknown =>
      '백업이 아직 확인되지 않았습니다. 생성된 파일을 보존하고 대상 위치를 확인하세요.';

  @override
  String pluginsBinaryPreview(String hex) {
    return '바이너리 콘텐츠: $hex';
  }

  @override
  String get pluginsBuiltin => '기본 작업 공간';

  @override
  String pluginsBuiltinCount(int count) {
    return '$count자 · 이 세션에서만 사용하며 카드로 저장되지 않음';
  }

  @override
  String get pluginsBuiltinEmpty => '텍스트를 입력하여 대문자 변환 미리 보기';

  @override
  String get pluginsBuiltinHeading => '텍스트 도구';

  @override
  String get pluginsBuiltinInput => '텍스트 입력';

  @override
  String get pluginsCancel => '취소';

  @override
  String get pluginsChoosePackage => '플러그인 파일 선택';

  @override
  String get pluginsChooseSmallFile => '작은 파일 선택';

  @override
  String get pluginsCloseTextTool => '텍스트 도구 숨기기';

  @override
  String get pluginsCloseUnknown => '보기 닫기를 확인하지 못했습니다';

  @override
  String get pluginsCloseView => '보기 닫기';

  @override
  String get pluginsConnectionLost => '연결이 중단되었습니다. 플러그인 보기를 다시 여세요.';

  @override
  String get pluginsContentPermissions => '콘텐츠 권한';

  @override
  String get pluginsCreate => '콘텐츠 만들기';

  @override
  String get pluginsCredentialCancel => '양식 닫기';

  @override
  String get pluginsCredentialCreateTitle => '새 자격 증명';

  @override
  String pluginsCredentialDays(int days) {
    return '$days일';
  }

  @override
  String get pluginsCredentialDetails =>
      '승인된 API 연결의 자격 증명을 안전하게 저장합니다. 저장해도 서버가 승인되거나 플러그인이 활성화되지 않습니다. 저장된 비밀 값은 볼 수 없습니다.';

  @override
  String get pluginsCredentialDisable => '비활성화';

  @override
  String get pluginsCredentialDisabled => '비활성화됨';

  @override
  String get pluginsCredentialDisabledDone => '자격 증명을 비활성화했습니다.';

  @override
  String get pluginsCredentialEmpty => '저장된 자격 증명 없음';

  @override
  String get pluginsCredentialExpired => '만료됨';

  @override
  String pluginsCredentialExpires(String date) {
    return '만료일: $date';
  }

  @override
  String get pluginsCredentialHeader => '헤더 이름';

  @override
  String get pluginsCredentialInvalid =>
      '헤더 이름을 확인하고 새 비밀 값을 입력하세요. 비밀 값 필드가 지워졌습니다.';

  @override
  String get pluginsCredentialLifetime => '유효 기간';

  @override
  String get pluginsCredentialLoadFailed =>
      '자격 증명을 일관되게 읽지 못했습니다. 상태를 새로 고쳐 재시도하세요.';

  @override
  String get pluginsCredentialNew => '자격 증명 추가';

  @override
  String get pluginsCredentialReading => '자격 증명 읽는 중…';

  @override
  String pluginsCredentialReference(String reference) {
    return '자격 증명 $reference';
  }

  @override
  String get pluginsCredentialRefresh => '상태 새로 고침';

  @override
  String get pluginsCredentialReplace => '비밀 값 교체';

  @override
  String pluginsCredentialReplaceTitle(String reference) {
    return '자격 증명 $reference 교체';
  }

  @override
  String get pluginsCredentialSave => '자격 증명 저장';

  @override
  String get pluginsCredentialSaved =>
      '자격 증명을 저장했습니다. API 연결에는 여전히 별도 승인이 필요합니다.';

  @override
  String get pluginsCredentialSecret => '새 비밀 값';

  @override
  String get pluginsCredentialStored => '저장됨';

  @override
  String get pluginsCredentialTitle => 'API 자격 증명';

  @override
  String get pluginsCredentialUnknown =>
      '결과를 확인하지 못했습니다. 비밀 값 필드가 지워졌습니다. 다른 변경 전에 상태를 새로 고치세요.';

  @override
  String pluginsDeclared(String permissions) {
    return '선언된 권한: $permissions';
  }

  @override
  String get pluginsDependenciesNotice =>
      '종속성은 호스트에서 구성해야 합니다. 이 페이지에서는 승인하지 않습니다.';

  @override
  String get pluginsDisable => '비활성화';

  @override
  String get pluginsDisableWorkbench => '작업 공간 플러그인 비활성화';

  @override
  String get pluginsDisabled => '비활성화됨';

  @override
  String get pluginsDisabledDetails =>
      '비활성화됨. 편집기와 도구를 사용하려면 콘텐츠 읽기 및 편집을 허용하세요.';

  @override
  String get pluginsEdit => '콘텐츠 편집';

  @override
  String get pluginsEmptyLibrary => '가져온 타사 플러그인이 없습니다.';

  @override
  String get pluginsEmptyResult => '(빈 결과)';

  @override
  String get pluginsEnabled => '활성화됨';

  @override
  String get pluginsEnabledDetails =>
      '활성화됨. 이 플러그인은 콘텐츠를 읽고 편집할 수 있습니다. 비활성화해도 데이터는 보존됩니다.';

  @override
  String get pluginsEndpointAdvanced => '정책 한도(별도 표시가 없으면 바이트)';

  @override
  String get pluginsEndpointCertificate => 'DER 신뢰 루트 선택';

  @override
  String get pluginsEndpointCertificateDetails =>
      '선택적 HTTPS 신뢰 루트: 32 KiB 이하의 바이너리 DER 인증서(.der 또는 .cer) 한 개. PEM 및 인증서 묶음은 지원하지 않습니다. HTTP로 전환하기 전에 신뢰 루트를 제거하세요.';

  @override
  String get pluginsEndpointCertificateInvalid =>
      '32 KiB 이하의 유효한 바이너리 DER 인증서(.der 또는 .cer) 한 개를 선택하세요.';

  @override
  String pluginsEndpointCertificateSelected(int bytes) {
    return 'DER 신뢰 루트 선택됨($bytes바이트)';
  }

  @override
  String get pluginsEndpointConcurrency => '동시 요청 수(1~128)';

  @override
  String get pluginsEndpointCreateTitle => '새 엔드포인트 승인';

  @override
  String get pluginsEndpointCredential => '자격 증명 참조';

  @override
  String get pluginsEndpointCredentialLifetime =>
      '선택한 자격 증명은 엔드포인트의 전체 유효 기간 동안 유효해야 합니다. 만료일은 연장되지 않습니다.';

  @override
  String get pluginsEndpointCredentialUnavailable =>
      '패키지에 선언되고 승인된 자격 증명 사용 권한과 유효한 저장 참조가 필요합니다.';

  @override
  String get pluginsEndpointCredentialsFailed =>
      '자격 증명 참조를 읽지 못했습니다. 상태를 새로 고친 후 선택하세요.';

  @override
  String get pluginsEndpointDetails =>
      '특정 패키지와 다이제스트에 대한 서버 정책을 저장합니다. 저장해도 네트워크에 연결하거나 플러그인을 활성화하지 않으며 네트워크 작업이 즉시 사용 가능해지지 않습니다.';

  @override
  String get pluginsEndpointDigest => '패키지 다이제스트';

  @override
  String get pluginsEndpointDisabledDone => '엔드포인트 승인을 비활성화했습니다.';

  @override
  String get pluginsEndpointEmpty => '저장된 엔드포인트 승인 없음';

  @override
  String get pluginsEndpointFrameBytes => '프레임 한도(1~131072바이트)';

  @override
  String get pluginsEndpointHeaderBytes => '최대 헤더 바이트(1~16384)';

  @override
  String get pluginsEndpointInvalid =>
      '패키지, 오리진, 메서드, 1~30일 유효 기간, 자격 증명 권한, 인증서 및 정책 한도를 확인하세요.';

  @override
  String get pluginsEndpointLifetime => '유효 기간(1~30일)';

  @override
  String get pluginsEndpointLoadFailed =>
      '엔드포인트 승인을 일관되게 읽지 못했습니다. 상태를 새로 고쳐 재시도하세요.';

  @override
  String get pluginsEndpointLocalHttp => '로컬 HTTP';

  @override
  String get pluginsEndpointLocalHttps => '로컬 HTTPS';

  @override
  String get pluginsEndpointMethods => '허용된 요청 메서드';

  @override
  String get pluginsEndpointNew => '엔드포인트 추가';

  @override
  String get pluginsEndpointNoCredential => '자격 증명 없음';

  @override
  String get pluginsEndpointOrigin => '오리진만 입력(예: https://api.example.com)';

  @override
  String get pluginsEndpointPackage => '패키지';

  @override
  String get pluginsEndpointPackageUnavailable =>
      '패키지를 사용할 수 없거나 승인된 HTTP 권한이 없습니다. 기존 승인은 비활성화할 수 있습니다.';

  @override
  String get pluginsEndpointProfile => '연결 프로필';

  @override
  String get pluginsEndpointPublicHttps => '공용 HTTPS';

  @override
  String get pluginsEndpointRemoveCertificate => '신뢰 루트 제거';

  @override
  String get pluginsEndpointReplace => '승인 교체';

  @override
  String get pluginsEndpointReplaceTitle => '현재 패키지 다이제스트로 승인 교체';

  @override
  String get pluginsEndpointRequestBytes => '최대 요청 바이트(1~65536)';

  @override
  String get pluginsEndpointResponseBytes => '최대 응답 바이트(1~65536)';

  @override
  String get pluginsEndpointSave => '엔드포인트 승인 저장';

  @override
  String get pluginsEndpointSaved => '엔드포인트 승인을 저장했습니다. 네트워크 연결은 하지 않았습니다.';

  @override
  String get pluginsEndpointTimeout => '시간 제한(1~30000밀리초)';

  @override
  String get pluginsEndpointTitle => 'API 엔드포인트 승인';

  @override
  String get pluginsEndpointUnknown =>
      '결과를 확인하지 못했습니다. 다른 변경 전에 상태를 새로 고치세요. 요청은 자동으로 다시 전송되지 않습니다.';

  @override
  String get pluginsEndpointWorking => '엔드포인트 상태 갱신 중…';

  @override
  String get pluginsExistingVersion =>
      '이 버전은 이미 설치되어 있습니다. 활성화 상태는 변경되지 않았습니다.';

  @override
  String pluginsFileLimit(int limit) {
    return '파일이 너무 큽니다. $limit바이트 이하의 파일을 선택하세요.';
  }

  @override
  String get pluginsHttpTaskAbandon => '이 시도 관측 종료';

  @override
  String get pluginsHttpTaskAbandonDetails =>
      '최신 상태에서 활성 작업이 없고 원래 라이브러리가 사용 가능함을 확인한 후에만 관측을 종료할 수 있습니다. 원격 영향이 없었다는 증거는 아닙니다. 식별자와 불확실성은 기록에 남으며 새 요청은 명시적으로 제출해야 합니다.';

  @override
  String get pluginsHttpTaskAbsent => '전달된 결과 없음';

  @override
  String get pluginsHttpTaskAccepted => '수락됨';

  @override
  String get pluginsHttpTaskAcknowledge => '완료된 작업 확인';

  @override
  String get pluginsHttpTaskArchivedUnknown =>
      '사용자가 관측을 종료했습니다. 이전 원격 영향은 여전히 미확인 상태이며 이 시도는 재실행되지 않았습니다.';

  @override
  String get pluginsHttpTaskBase64 => 'Base64';

  @override
  String get pluginsHttpTaskBody => '요청 본문';

  @override
  String get pluginsHttpTaskBodyFormat => '요청 본문 인코딩';

  @override
  String get pluginsHttpTaskBusy => '사용 중';

  @override
  String get pluginsHttpTaskCancel => '취소 요청';

  @override
  String get pluginsHttpTaskCancelled => '취소 감지됨 · 원격 영향이 이미 발생했을 수 있음';

  @override
  String get pluginsHttpTaskCatalogUnavailable =>
      '플러그인 목록 상태를 사용할 수 없습니다. 새 제출 전에 플러그인 라이브러리를 새로 고치세요. 기존 작업은 계속 제어할 수 있습니다.';

  @override
  String get pluginsHttpTaskClosed => '닫힘';

  @override
  String get pluginsHttpTaskCompleted => '완료됨';

  @override
  String get pluginsHttpTaskConflict => '충돌';

  @override
  String get pluginsHttpTaskConsumed => '결과 소비됨';

  @override
  String get pluginsHttpTaskControlUnknown =>
      '제어 결과를 확인하지 못했습니다. 다음 작업을 결정하기 전에 상태를 새로 고치세요.';

  @override
  String pluginsHttpTaskCounters(String bytes, String calls) {
    return 'IO 호출: $calls; 계상된 바이트: $bytes';
  }

  @override
  String get pluginsHttpTaskDeadline => '기한 초과';

  @override
  String get pluginsHttpTaskDenied => '거부됨';

  @override
  String get pluginsHttpTaskDetails =>
      '승인된 엔드포인트와 실험적 HTTP 전달 처리기가 있는 활성 플러그인으로 명시적 요청을 한 번 실행합니다. 콘텐츠 라이브러리가 사용 중이어도 작업 상태를 확인할 수 있습니다.';

  @override
  String get pluginsHttpTaskDisconnect => '연결 정리 실패';

  @override
  String get pluginsHttpTaskEndpoint => '승인된 엔드포인트';

  @override
  String get pluginsHttpTaskEndpointsFailed =>
      '엔드포인트를 일관되게 읽지 못했거나 라이브러리가 사용 중입니다. 작업 제어는 계속 가능합니다. 라이브러리가 반환되면 엔드포인트를 새로 고치세요.';

  @override
  String get pluginsHttpTaskEvidenceUnavailable => '결과 증거 사용 불가';

  @override
  String pluginsHttpTaskExecution(int code, String fault) {
    return '게스트 실행: $fault; 종료 코드: $code';
  }

  @override
  String pluginsHttpTaskExit(
    String disconnect,
    String execution,
    String maintenance,
  ) {
    return '워커 종료 — 실행: $execution; 연결 해제: $disconnect; 유지 관리: $maintenance';
  }

  @override
  String get pluginsHttpTaskExplicit =>
      '제출하면 실제 요청을 한 번 보냅니다. 클릭할 때마다 새 식별자가 생성됩니다. 결과가 불명확한 제출이나 읽기는 자동 재실행되지 않습니다. 취소가 원격 작업의 롤백을 증명하지는 않습니다.';

  @override
  String get pluginsHttpTaskFailed => '실패';

  @override
  String get pluginsHttpTaskHeaders => '일반 요청 헤더, 한 줄에 이름: 값 하나';

  @override
  String get pluginsHttpTaskHeadersHint =>
      '중복 헤더는 별도로 유지됩니다. 자격 증명 및 연결 헤더는 런타임에서만 제공합니다.';

  @override
  String get pluginsHttpTaskHistory => '이전 작업 관측(최대 5개)';

  @override
  String pluginsHttpTaskHttpResult(int code, String status) {
    return 'HTTP 결과: $status; 원격 상태: $code';
  }

  @override
  String get pluginsHttpTaskInactive => '연결 비활성';

  @override
  String get pluginsHttpTaskInvalid =>
      '엔드포인트, 메서드, 상대 대상, 일반 헤더, 본문 인코딩 및 시간 제한이 승인 한도에 맞는지 확인하세요.';

  @override
  String get pluginsHttpTaskInvalidOptions => '잘못된 옵션';

  @override
  String pluginsHttpTaskKey(String identity) {
    return '작업 식별자: $identity';
  }

  @override
  String get pluginsHttpTaskLimit => '할당량 또는 한도 도달';

  @override
  String get pluginsHttpTaskLoadingEndpoints => '승인된 엔드포인트 읽는 중…';

  @override
  String get pluginsHttpTaskLocal => '라이브러리 사용 가능 · 활성 작업 없음';

  @override
  String get pluginsHttpTaskModule => '잘못된 게스트 모듈';

  @override
  String get pluginsHttpTaskNew => '새 요청 준비';

  @override
  String get pluginsHttpTaskNoEndpoints =>
      '활성화되고 승인된 HTTP 전달 플러그인과 일치하는 엔드포인트가 없습니다.';

  @override
  String get pluginsHttpTaskNotFound => '찾을 수 없음';

  @override
  String get pluginsHttpTaskOk => '정상';

  @override
  String get pluginsHttpTaskOutcomeUnknown =>
      '원격 결과 불명 · 롤백되었다고 가정하거나 재전송하지 마세요';

  @override
  String get pluginsHttpTaskPackageChanged => '패키지 바인딩 변경됨';

  @override
  String get pluginsHttpTaskPending => '결과 대기 중';

  @override
  String get pluginsHttpTaskPoll => '작업 확인';

  @override
  String get pluginsHttpTaskProtocol => '작업 프로토콜 오류';

  @override
  String get pluginsHttpTaskRead => '결과 한 번 읽기';

  @override
  String get pluginsHttpTaskReadBound => '읽기 한도 초과';

  @override
  String get pluginsHttpTaskReadPending =>
      '반환된 결과가 없습니다. 명시적으로 다시 읽기 전에 상태를 확인하세요.';

  @override
  String get pluginsHttpTaskReadUnknown =>
      '결과 읽기를 확인하지 못했으며 이미 결과가 소비되었을 수 있습니다. 다시 읽지 않습니다. 상태와 종료 여부는 확인할 수 있습니다.';

  @override
  String get pluginsHttpTaskReady =>
      '결과 준비 완료: 명시적으로 읽으세요. 워커가 종료되었다는 뜻은 아닙니다.';

  @override
  String get pluginsHttpTaskReclaimed => '워커 종료 · 원래 라이브러리 반환됨';

  @override
  String get pluginsHttpTaskRecoveryRequired => '워커 종료 · 정리 또는 유지 관리 복구 필요';

  @override
  String get pluginsHttpTaskRefresh => '작업 상태 새로 고침';

  @override
  String get pluginsHttpTaskRefreshEndpoints => '승인된 엔드포인트 새로 고침';

  @override
  String get pluginsHttpTaskRemoteError =>
      '원격 서버가 4xx/5xx 응답을 반환했습니다. HTTP 교환은 완료되었으며 게스트 실행 오류와는 별개입니다.';

  @override
  String get pluginsHttpTaskRepair => '정리 복구';

  @override
  String get pluginsHttpTaskResponseBase64 => '응답 본문: 정확한 Base64';

  @override
  String get pluginsHttpTaskResponseHeaders => '응답 헤더(중복 유지, 바이너리 값은 Base64)';

  @override
  String get pluginsHttpTaskResponseText => '응답 본문: 일반 텍스트 미리 보기';

  @override
  String get pluginsHttpTaskResultUnavailable => '결과 전달 사용 불가';

  @override
  String get pluginsHttpTaskRevoked => '승인 취소됨';

  @override
  String get pluginsHttpTaskRunning => '실행 중 · 워커가 라이브러리 소유';

  @override
  String get pluginsHttpTaskSpawn => '워커를 시작하지 못함';

  @override
  String get pluginsHttpTaskStart => '새 요청 제출';

  @override
  String get pluginsHttpTaskStartUnknown =>
      '제출 결과를 알 수 없습니다. 식별자는 유지됩니다. 상태를 조회하여 같은 작업을 확인하세요. 요청은 다시 전송되지 않습니다.';

  @override
  String get pluginsHttpTaskStatusFailed =>
      '작업 상태를 확인하지 못했습니다. 상태를 새로 고치세요. 요청은 재실행되지 않았습니다.';

  @override
  String get pluginsHttpTaskStopping => '중지 중 · 실제 워커 종료 대기';

  @override
  String pluginsHttpTaskSubmission(String identity) {
    return '제출 식별자: $identity';
  }

  @override
  String get pluginsHttpTaskTarget => '상대 대상(예: /v1/items?limit=10)';

  @override
  String get pluginsHttpTaskText => 'UTF-8 텍스트';

  @override
  String get pluginsHttpTaskTimeout => '시간 제한(1~30000밀리초, 승인 범위 내)';

  @override
  String get pluginsHttpTaskTitle => 'HTTP 작업';

  @override
  String get pluginsHttpTaskTrap => '게스트 실행 트랩 발생';

  @override
  String get pluginsHttpTaskUnavailable => '원래 라이브러리 사용 불가 · 복구 필요';

  @override
  String get pluginsHttpTaskUnsupported => '지원하지 않는 작업';

  @override
  String get pluginsHttpTaskWorking => '작업 제어 응답 대기 중…';

  @override
  String get pluginsImport => '가져오기';

  @override
  String get pluginsImportDetails =>
      '가져온 후 활성화 여부를 선택합니다. 비활성화하거나 제거해도 콘텐츠는 보존됩니다.';

  @override
  String pluginsImportPreview(String name) {
    return '가져오기 미리 보기: $name';
  }

  @override
  String get pluginsImportUnknown => '가져오기를 확인하지 못했습니다';

  @override
  String get pluginsImportedDisabled => '가져왔으며 비활성화 상태입니다. 허용할 권한을 선택하세요.';

  @override
  String get pluginsInputFailed => '입력 파일을 읽지 못했습니다';

  @override
  String get pluginsInputTooLong => '입력 한도에 도달했습니다. 텍스트를 줄인 후 다시 시도하세요.';

  @override
  String get pluginsInspectFailed => '플러그인 미리 보기를 불러오지 못했습니다';

  @override
  String get pluginsInspectedOnly => '파일 검사만 완료했습니다. 가져온 후 플러그인을 별도로 활성화하세요.';

  @override
  String get pluginsInsufficientApproval =>
      '플러그인이 활성화되었지만 콘텐츠 권한이 필요합니다. 작업 공간은 읽기 전용입니다. 비활성화한 후 권한을 다시 확인하세요.';

  @override
  String pluginsIoApproved(String permissions) {
    return '승인됨: $permissions';
  }

  @override
  String get pluginsIoCredentialUse => '승인된 자격 증명 사용';

  @override
  String pluginsIoDeclared(String permissions) {
    return '요청한 네트워크 및 파일 권한: $permissions';
  }

  @override
  String get pluginsIoFileCreate => '파일 만들기';

  @override
  String get pluginsIoFileDelete => '파일 삭제';

  @override
  String get pluginsIoFileList => '승인된 폴더 탐색';

  @override
  String get pluginsIoFileRead => '승인된 파일 읽기';

  @override
  String get pluginsIoFileReplace => '파일 교체';

  @override
  String get pluginsIoHttpListen => '네트워크 연결 수신 대기';

  @override
  String get pluginsIoHttpPublish => 'API 서비스 제공';

  @override
  String get pluginsIoHttpRequest => '네트워크 API 호출';

  @override
  String get pluginsIoNoneApproved => '승인된 네트워크 또는 파일 권한 없음';

  @override
  String get pluginsIoRevoke => '모든 네트워크 및 파일 권한 취소';

  @override
  String get pluginsIoSave => '네트워크 및 파일 권한 저장';

  @override
  String get pluginsIoScopeNotice =>
      '여기서는 권한 종류만 저장합니다. 서버 주소, 파일 접근, 자격 증명은 별도 승인이 필요하며 사용할 수 없는 기능이 활성화되지는 않습니다. 변경 후 플러그인 양식을 다시 여세요.';

  @override
  String get pluginsIoTitle => '네트워크 및 파일 권한';

  @override
  String get pluginsIoWebSocketConnect => 'WebSocket 서비스에 연결';

  @override
  String get pluginsListUnknown => '플러그인 목록을 확인하지 못했습니다';

  @override
  String get pluginsManageAbove => '위의 작업 공간 플러그인 제어 항목에서 관리하세요.';

  @override
  String get pluginsManagementUnavailable =>
      '플러그인 관리를 사용할 수 없습니다. 기존 콘텐츠는 계속 읽을 수 있습니다.';

  @override
  String get pluginsNoPermissions => '선언된 콘텐츠 권한이 없습니다.';

  @override
  String get pluginsOpenTextTool => '텍스트 도구 열기';

  @override
  String get pluginsOpenView => '보기 열기';

  @override
  String get pluginsOpeningView => '플러그인 보기 여는 중…';

  @override
  String get pluginsOperation => '작업 결과 조회';

  @override
  String pluginsOtherCapability(String name) {
    return '기타 선언된 권한: $name';
  }

  @override
  String get pluginsPackageFile => 'Morrow 플러그인';

  @override
  String get pluginsPreviewOnly => '미리 보기 전용입니다. 결과는 기존 콘텐츠에 자동 저장되지 않습니다.';

  @override
  String get pluginsPreviewTruncated => '…처음 4,096자만 표시';

  @override
  String get pluginsProtection => '콘텐츠 보호';

  @override
  String get pluginsProtectionDetails =>
      '이 시스템 계정으로 복구할 수 있도록 원본 보호 파일을 백업하세요. 카드나 첨부 파일은 포함되지 않습니다.';

  @override
  String get pluginsProtectionFileType => '라이브러리 보호 파일';

  @override
  String get pluginsProtectionSaved =>
      '보호 파일을 백업했습니다. 시작에 실패하면 복구 시 이 파일을 선택하세요.';

  @override
  String get pluginsRead => '콘텐츠 읽기';

  @override
  String get pluginsReadingState => '플러그인 상태 불러오는 중…';

  @override
  String pluginsRefreshFailed(String reason) {
    return '$reason. 목록을 새로 고치지 못했습니다. ‘목록 새로 고침’을 선택하여 다시 읽으세요.';
  }

  @override
  String get pluginsRefreshList => '목록 새로 고침';

  @override
  String get pluginsRefreshState => '상태 새로 고침';

  @override
  String get pluginsRename => '이름 바꾸기';

  @override
  String pluginsResultBytes(int count, String preview) {
    return '$count바이트\n$preview';
  }

  @override
  String get pluginsSavePermissions => '권한 저장';

  @override
  String pluginsSelectedFile(String name) {
    return '선택한 파일: $name';
  }

  @override
  String get pluginsServiceAcknowledgeUncertain => '갱신된 기록을 확인했습니다';

  @override
  String get pluginsServiceAddScope => '콘텐츠 범위 추가';

  @override
  String get pluginsServiceAttachmentId => '정확한 첨부 파일 식별자';

  @override
  String get pluginsServiceAuthenticationUnavailable =>
      '선택한 인증이 없거나, 비활성화 또는 만료되었거나 다른 주체에 속합니다. 범위 초안은 유지됩니다. 유효한 인증으로 교체하거나 명시적으로 제거하세요.';

  @override
  String get pluginsServiceAuthorities => '인증 및 게시 기록';

  @override
  String get pluginsServiceCardId => '정확한 카드 식별자';

  @override
  String get pluginsServiceCatalogChanged =>
      '패키지 목록이 변경되었거나 사용할 수 없습니다. 초안은 유지됩니다. 저장 전에 선택을 명시적으로 새로 고치세요.';

  @override
  String get pluginsServiceClearToken => '토큰 지우기';

  @override
  String get pluginsServiceCloseEditor => '편집기 닫기';

  @override
  String get pluginsServiceConfigDigest => '구성 다이제스트';

  @override
  String get pluginsServiceConfiguration => '저장된 구성';

  @override
  String get pluginsServiceConfigurations => '저장된 구성';

  @override
  String get pluginsServiceCopyClear => '토큰 복사 후 지우기';

  @override
  String get pluginsServiceCreated => '생성일(UTC)';

  @override
  String get pluginsServiceDays => '요청 유효 기간(1~30일)';

  @override
  String get pluginsServiceDigestFixed =>
      '편집 시 원래 패키지 다이제스트를 유지합니다. 일치하는 패키지를 선택해야 하며 선택해도 활성화되지 않습니다.';

  @override
  String get pluginsServiceDisable => '비활성화';

  @override
  String get pluginsServiceDisabled => '비활성화됨';

  @override
  String get pluginsServiceEditConfig => '구성 편집';

  @override
  String get pluginsServiceEditPublication => '게시 편집';

  @override
  String get pluginsServiceExpired => '만료되었거나 아직 유효하지 않음';

  @override
  String get pluginsServiceExpires => '실제 만료일(UTC)';

  @override
  String get pluginsServiceHandler => '선언된 서비스 처리기';

  @override
  String get pluginsServiceIdentity => '서비스 식별자';

  @override
  String get pluginsServiceInvalid => '저장 전에 필드, 선택한 승인 및 현재 패키지를 확인하세요.';

  @override
  String get pluginsServiceIssue => '토큰 발급';

  @override
  String get pluginsServiceIssuedToken => '한 번만 표시되는 Bearer 토큰';

  @override
  String get pluginsServiceListenAddress => '숫자 수신 주소 및 포트';

  @override
  String get pluginsServiceLoadFailed => '기록을 새로 고치지 못했습니다. 변경 전에 다시 새로 고치세요.';

  @override
  String get pluginsServiceManagementOnly =>
      '저장된 구성과 승인을 관리합니다. 저장해도 리스너 시작, 패키지 실행 또는 서비스 활성화는 수행하지 않습니다.';

  @override
  String get pluginsServiceMethod => 'HTTP 메서드';

  @override
  String get pluginsServiceNewAuthentication => '새 인증';

  @override
  String get pluginsServiceNewConfig => '새 구성';

  @override
  String get pluginsServiceNo => '아니요';

  @override
  String get pluginsServiceNoAuthentication => '먼저 현재 유효한 인증 기록을 만드세요.';

  @override
  String get pluginsServiceNoAuthorities => '인증 또는 게시 기록이 없습니다.';

  @override
  String get pluginsServiceNoConfigurations => '서비스 구성이 없습니다.';

  @override
  String get pluginsServicePackage => '선언되고 승인된 패키지';

  @override
  String get pluginsServicePackageDigest => '패키지 다이제스트';

  @override
  String get pluginsServicePackageUnavailable =>
      '일치하는 패키지 또는 수신/게시 승인을 사용할 수 없습니다. 이전 기록은 계속 읽거나 비활성화할 수 있습니다.';

  @override
  String get pluginsServicePath => '정확한 요청 경로';

  @override
  String get pluginsServicePolicyChanged =>
      '원본 기록이 변경되었거나 사용할 수 없습니다. 선택을 새로 고치거나 현재 기록에서 편집기를 다시 여세요. 초안은 유지됩니다.';

  @override
  String get pluginsServicePrincipalId => '주체 식별자';

  @override
  String get pluginsServicePrincipals => '허용된 주체 및 콘텐츠 범위';

  @override
  String get pluginsServicePublicationEditor => '게시 승인';

  @override
  String get pluginsServicePublicationHelp =>
      '승인은 정확히 이 구성, 리비전 및 참조에 연결됩니다. 실제 만료는 선택한 모든 인증 기록에 의해 제한되며 요청한 기간보다 짧을 수 있습니다. 저장해도 수신 대기는 시작하지 않습니다.';

  @override
  String get pluginsServicePublicationMismatch =>
      '게시가 현재 구성과 더 이상 일치하지 않습니다. 검토한 후 대체 승인을 명시적으로 저장하세요.';

  @override
  String get pluginsServiceQueryPath => '별도 결과 조회 경로(선택)';

  @override
  String get pluginsServiceReference => '승인 참조';

  @override
  String get pluginsServiceRefresh => '기록 새로 고침';

  @override
  String get pluginsServiceRefreshSelection => '선택 새로 고침';

  @override
  String get pluginsServiceRemovePrincipal => '주체 제거';

  @override
  String get pluginsServiceRemoveScope => '범위 제거';

  @override
  String get pluginsServiceRetention => '요청 기록 보관 기간(밀리초, 최대 30일)';

  @override
  String get pluginsServiceRevision => '리비전';

  @override
  String get pluginsServiceRotate => '토큰 교체';

  @override
  String get pluginsServiceRotateAuthentication => '인증 교체';

  @override
  String get pluginsServiceRunAbandon => '기록을 유지하고 시도 종료';

  @override
  String get pluginsServiceRunAdvanced => '요청 및 워커 한도';

  @override
  String get pluginsServiceRunAttempt => '미해결 시작 시도';

  @override
  String get pluginsServiceRunBoundsHint =>
      '한도는 플러그인 선언 및 저장된 승인에도 맞아야 합니다. 예약된 작업은 취소되어도 누적 한도를 소비합니다. 만료 시 실행이 중지되며 자동 갱신되지 않습니다.';

  @override
  String get pluginsServiceRunBytes => '실행 바이트 한도(최대 67,108,864)';

  @override
  String get pluginsServiceRunCalls => '작업당 호출 수(최대 1,024)';

  @override
  String get pluginsServiceRunCancelled => '취소됨';

  @override
  String get pluginsServiceRunClosed => '닫힘';

  @override
  String get pluginsServiceRunConcurrent => '동시 작업 수(최대 128)';

  @override
  String get pluginsServiceRunControlUnknown =>
      '제어 결과를 알 수 없습니다. 다른 작업 전에 원래 서비스 상태를 새로 고치세요.';

  @override
  String get pluginsServiceRunDenied => '거부됨';

  @override
  String get pluginsServiceRunExited => '서비스 종료됨';

  @override
  String get pluginsServiceRunHeaderBytes => '최대 헤더 크기(바이트, 최대 65,536)';

  @override
  String get pluginsServiceRunHint =>
      '승인된 게시와 유한한 실행 한도를 선택하고 서비스를 명시적으로 시작하세요. 중지 후 원래 작업 공간 소유자가 복귀할 때까지 기다린 후 결과를 확인하세요.';

  @override
  String pluginsServiceRunHostFailure(String detail) {
    return '호스트 진단: $detail';
  }

  @override
  String get pluginsServiceRunHttpPanel =>
      'API 서비스가 이 작업을 소유합니다. 위 서비스 실행 패널에서 중지하거나 종료를 확인하세요. HTTP 요청 초안은 유지됩니다.';

  @override
  String get pluginsServiceRunIdentityChanged =>
      '다른 작업이 콘텐츠를 소유합니다. 이 패널은 이전 서비스 식별자로 해당 작업을 제어하지 않습니다.';

  @override
  String get pluginsServiceRunInvalid =>
      '선택한 서비스와 숫자 한도를 확인하세요. 새 실행은 제출하지 않았습니다.';

  @override
  String get pluginsServiceRunInvalidOutcome => '잘못된 구성';

  @override
  String get pluginsServiceRunJobBytes => '작업당 바이트(최대 16,777,216)';

  @override
  String get pluginsServiceRunJobs => '전체 작업 예약 수(최대 1,000,000)';

  @override
  String get pluginsServiceRunLastObservation => '마지막 관측 표시 중 · 현재 상태 미확인';

  @override
  String get pluginsServiceRunLifetime => '실행 시간(밀리초, 최대 3,600,000)';

  @override
  String get pluginsServiceRunLimit => '한도 도달';

  @override
  String get pluginsServiceRunLocal => '콘텐츠 로컬 사용 가능';

  @override
  String pluginsServiceRunNetwork(
    String bind,
    String listener,
    String supervision,
  ) {
    return '바인딩: $bind; 리스너: $listener; 감독: $supervision';
  }

  @override
  String get pluginsServiceRunNextSettings => '다음 명시적 실행 설정';

  @override
  String get pluginsServiceRunNoSelection =>
      '사용 가능한 승인된 게시가 없습니다. 플러그인, 구성 및 인증 상태를 확인하세요.';

  @override
  String get pluginsServiceRunOutboundAttempt => '이 시작 시도에 연결된 엔드포인트';

  @override
  String get pluginsServiceRunOutboundClear => '엔드포인트 선택 해제';

  @override
  String get pluginsServiceRunOutboundFailed =>
      '엔드포인트 목록을 확인하지 못했습니다. 선택한 항목을 사용하기 전에 새로 고치세요.';

  @override
  String get pluginsServiceRunOutboundHint =>
      '외부 API(선택, 최대 8개). 이 패키지에 승인된 엔드포인트만 표시됩니다. 모두 선택 해제하면 외부 호출이 차단됩니다.';

  @override
  String get pluginsServiceRunOutboundStale =>
      '선택한 엔드포인트가 변경되었거나 사용할 수 없습니다. 현재 버전을 명시적으로 선택하거나 선택을 해제하세요.';

  @override
  String get pluginsServiceRunOwned => '실행 중인 서비스가 콘텐츠 관리';

  @override
  String get pluginsServiceRunPending => '대기 중';

  @override
  String get pluginsServiceRunReclaimed => '콘텐츠 소유권 회수됨 · 확인 필요';

  @override
  String get pluginsServiceRunReclaiming => '콘텐츠 소유권 회수 대기';

  @override
  String get pluginsServiceRunRecovery => '정리 복구 필요';

  @override
  String get pluginsServiceRunRequestBytes => '최대 요청 크기(바이트)';

  @override
  String get pluginsServiceRunResponseBytes => '최대 응답 크기(바이트)';

  @override
  String get pluginsServiceRunRunning => '서비스 실행 중';

  @override
  String get pluginsServiceRunSelection => '승인된 서비스 게시';

  @override
  String get pluginsServiceRunStale =>
      '선택한 패키지, 구성 또는 승인이 변경되었습니다. 기록을 새로 고치고 시작 전에 다시 선택하세요.';

  @override
  String get pluginsServiceRunStart => '한도 있는 서비스 시작';

  @override
  String get pluginsServiceRunStartRejected =>
      '시작 응답에서 오류를 보고했습니다. 현재 작업은 확인했습니다. 원인과 정리 상태를 검토한 후 진행하세요.';

  @override
  String get pluginsServiceRunStartUnknown =>
      '시작 결과를 알 수 없습니다. 시도 식별자는 유지됩니다. 새로 고쳐 확인하세요. 자동으로 다시 시작하지 않습니다.';

  @override
  String get pluginsServiceRunStarting => '서비스 시작 중';

  @override
  String get pluginsServiceRunStatusFailed =>
      '현재 서비스 상태를 확인하지 못했습니다. 다음 작업 전에 새로 고치세요.';

  @override
  String get pluginsServiceRunStop => '서비스 중지';

  @override
  String get pluginsServiceRunStopping => '중지 중 · 리스너 및 워커 종료 대기';

  @override
  String get pluginsServiceRunSucceeded => '성공';

  @override
  String get pluginsServiceRunTask => '현재 작업 식별자';

  @override
  String get pluginsServiceRunTimeout => '작업 시간 제한(밀리초, 최대 30,000)';

  @override
  String get pluginsServiceRunTimeoutOutcome => '시간 초과';

  @override
  String get pluginsServiceRunTitle => 'API 서비스 실행';

  @override
  String get pluginsServiceRunTotalBytes => '워커 바이트 한도(최대 67,108,864)';

  @override
  String get pluginsServiceRunTransport => '전송 실패';

  @override
  String get pluginsServiceRunUnavailable => '콘텐츠 저장소 사용 불가';

  @override
  String get pluginsServiceSaveConfig => '구성 저장';

  @override
  String get pluginsServiceSavePublication => '게시 승인 저장';

  @override
  String get pluginsServiceSaved => '저장했습니다. 아래에서 반환된 리비전과 실제 만료일을 확인하세요.';

  @override
  String get pluginsServiceScopeAttachment => '첨부 파일 읽기';

  @override
  String get pluginsServiceScopeCreate => '콘텐츠 만들기';

  @override
  String get pluginsServiceScopeEdit => '콘텐츠 편집';

  @override
  String get pluginsServiceScopeKind => '허용된 콘텐츠 작업';

  @override
  String get pluginsServiceScopeQuery => '작업 조회';

  @override
  String get pluginsServiceScopeRead => '콘텐츠 읽기';

  @override
  String get pluginsServiceScopeRename => '카드 이름 바꾸기';

  @override
  String get pluginsServiceScopeSummary => '요약 읽기';

  @override
  String get pluginsServiceScopesHelp =>
      '인증을 명시적으로 선택하세요. 허용할 작업과 정확한 객체 식별자를 아래에 추가하세요. 범위나 주체 삭제는 전용 버튼으로 수행하며 편집 중 기존 범위는 유지됩니다.';

  @override
  String get pluginsServiceTitle => '서비스 구성';

  @override
  String get pluginsServiceTls => 'TLS 필수';

  @override
  String get pluginsServiceTlsAttempt => '이 시작 시도에 연결된 인증서 PEM 다이제스트';

  @override
  String get pluginsServiceTlsCertificate => '인증서 체인 선택';

  @override
  String get pluginsServiceTlsChecked =>
      '인증서와 키 쌍을 확인했습니다. 인증서 PEM SHA-256이 아래에 표시됩니다. 클라이언트는 호스트 이름, 유효 기간 및 신뢰 체인을 계속 검증해야 합니다.';

  @override
  String get pluginsServiceTlsChecking => '인증서 선택 처리 중…';

  @override
  String get pluginsServiceTlsFailed =>
      '인증서 검사에 실패했습니다. PEM 파일, 키 쌍 및 로컬 경로를 확인한 후 다시 시도하세요.';

  @override
  String get pluginsServiceTlsHelp =>
      '루프백이 아닌 주소에는 TLS가 필요합니다. 요구 사항만 저장하며 리스너나 TLS ID를 만들지 않습니다.';

  @override
  String get pluginsServiceTlsHint =>
      'PEM 인증서 체인과 개인 키를 선택한 후 검사하세요. 시작 시 파일을 다시 검사하며 활성 인증서는 자동 교체되지 않습니다.';

  @override
  String get pluginsServiceTlsInspect => '인증서 검사';

  @override
  String get pluginsServiceTlsOutsideValidity =>
      '인증서 체인이 아직 유효하지 않거나 만료되었습니다. 인증서를 확인 또는 교체하고 시작 전에 다시 검사하세요.';

  @override
  String get pluginsServiceTlsPrivateKey => '개인 키 선택';

  @override
  String get pluginsServiceTlsRecheck =>
      '시작 전에 인증서를 다시 검사하세요. 시계가 변경되어도 이전 선택은 복원되지 않습니다.';

  @override
  String get pluginsServiceTlsUnavailable => '이 백엔드는 로컬 TLS 인증서 선택을 지원하지 않습니다.';

  @override
  String pluginsServiceTlsValidity(String end, String start) {
    return '인증서 체인의 공통 유효 기간(UTC): $start ~ $end. 만료 후 서비스가 중지됩니다.';
  }

  @override
  String get pluginsServiceTokenDiscarded =>
      '패널을 닫아 한 번만 표시되는 토큰이 지워졌습니다. 필요하면 새 토큰을 명시적으로 발급하세요.';

  @override
  String get pluginsServiceTokenHelp =>
      '토큰은 지금만 표시됩니다. 필요하면 명시적으로 복사하세요. 지우거나 패널을 닫으면 세션에서 제거되며 목록에서 다시 가져올 수 없습니다. 교체 시 이전 토큰이 대체됩니다.';

  @override
  String get pluginsServiceUncertainHelp =>
      '먼저 원본 기록을 새로 고치고 확인하세요. 이 알림 확인은 다른 명시적 작업을 허용할 뿐, 이전 변경의 실패를 증명하거나 재실행하지 않습니다.';

  @override
  String get pluginsServiceUnsupported => '지원하지 않는 이전 값';

  @override
  String get pluginsServiceWorking => '처리 중…';

  @override
  String get pluginsServiceWriteUnknown =>
      '마지막 변경 결과를 알 수 없습니다. 다시 전송하지 않았습니다.';

  @override
  String get pluginsServiceYes => '예';

  @override
  String get pluginsSettingsUnknown =>
      '설정이 아직 확인되지 않았습니다. 상태를 새로 고친 후 다시 선택하세요.';

  @override
  String get pluginsSnapshotDetails =>
      '백업에는 카드, 첨부 파일 및 감사 기록이 포함됩니다. 외부 리소스는 참조로 유지됩니다. 복구에는 원래 시스템 계정이 필요합니다.';

  @override
  String get pluginsSnapshotSaved => '첨부 파일과 원본 보호 파일을 포함하여 라이브러리를 백업했습니다.';

  @override
  String get pluginsStateUnavailable => '플러그인 상태를 불러오지 못했습니다. 다시 시도하세요.';

  @override
  String get pluginsSummary => '요약 읽기';

  @override
  String get pluginsTextInput => '입력 텍스트';

  @override
  String get pluginsThirdParty => '타사 플러그인';

  @override
  String get pluginsTlsIdentitiesDisable => 'ID 비활성화';

  @override
  String get pluginsTlsIdentitiesEmpty => '이 라이브러리에 저장된 ID가 없습니다.';

  @override
  String get pluginsTlsIdentitiesFileMode => '다음 시작: 검사된 로컬 파일.';

  @override
  String get pluginsTlsIdentitiesHint =>
      'ID를 명시적으로 선택하세요. 교체하거나 비활성화하면 이를 사용하는 서비스가 중지됩니다. 새 시작은 항상 명시적으로 수행합니다.';

  @override
  String get pluginsTlsIdentitiesImport => '가져오기 또는 교체할 인증서 파일 준비';

  @override
  String get pluginsTlsIdentitiesReplace => '검사된 파일로 교체';

  @override
  String get pluginsTlsIdentitiesSave => '새 ID로 저장';

  @override
  String get pluginsTlsIdentitiesSaved =>
      '저장했습니다. 아래 ID와 리비전을 검토한 후 새 시작에 사용하도록 선택하세요.';

  @override
  String get pluginsTlsIdentitiesSavedMode =>
      '다음 시작: 저장된 ID. 호스트가 시작 시 인증서 유효 기간을 확인합니다.';

  @override
  String get pluginsTlsIdentitiesSelect => '다음 시작에 사용';

  @override
  String get pluginsTlsIdentitiesStale =>
      '선택한 ID가 변경 또는 비활성화되었거나 새로 고쳐지지 않았습니다. 현재 ID를 다시 선택하세요.';

  @override
  String get pluginsTlsIdentitiesTitle => '저장된 TLS ID';

  @override
  String get pluginsTlsIdentitiesUnknownHint =>
      '확인 전에 기록을 새로 고치고 검토하세요. 영수증이 없다고 변경이 실패한 것은 아닙니다. 확인 없이 다시 만들지 마세요.';

  @override
  String get pluginsTlsIdentitiesUseFile => '다음 시작에 검사된 파일 사용';

  @override
  String get pluginsTransform => '변환';

  @override
  String get pluginsTransformUnknown => '변환을 확인하지 못했습니다';

  @override
  String get pluginsUiExecution => '플러그인 실행이 완료되지 않았습니다. 보기를 다시 열고 재시도하세요.';

  @override
  String get pluginsUiRejected => '플러그인 작업이 수락되지 않았습니다. 입력 내용과 현재 권한을 확인하세요.';

  @override
  String get pluginsUiUnavailable => '플러그인을 사용할 수 없습니다. 상태를 확인한 후 보기를 다시 여세요.';

  @override
  String get pluginsUnavailableView => '플러그인 보기를 사용할 수 없음';

  @override
  String pluginsUnconfirmed(String reason) {
    return '$reason. 작업이 확인되지 않았습니다. 갱신된 상태를 확인한 후 다시 선택하세요.';
  }

  @override
  String get pluginsUninstallKeepContent => '제거(콘텐츠 유지)';

  @override
  String get pluginsUninstallUnknown => '제거를 확인하지 못했습니다';

  @override
  String get pluginsUninstalled => '제거했습니다. 기존 콘텐츠는 보존되었습니다.';

  @override
  String get pluginsUpdatingView => '미리 보기 갱신 중…';

  @override
  String get pluginsUseText => '텍스트로 입력';

  @override
  String get pluginsUseTransform => '변환 사용';

  @override
  String get pluginsViewFailed => '플러그인 보기를 열지 못했습니다';

  @override
  String get pluginsWorkbench => '작업 공간 플러그인';

  @override
  String get pluginsWorkbenchReadOnly =>
      '플러그인은 허용되었지만 작업 공간은 읽기 전용입니다. 라이브러리나 플러그인 문제를 해결한 후 상태를 새로 고치세요.';

  @override
  String get recoveryAllFiles => '모든 파일';

  @override
  String get recoveryBackupExists => '백업 위치에 파일이 이미 있습니다. 새 이름을 선택하세요.';

  @override
  String get recoveryBackupFile => '라이브러리 백업';

  @override
  String get recoveryBackupUnknown =>
      '백업 결과를 확인해야 합니다. 현재 파일을 보존하고 저장 위치를 확인하세요.';

  @override
  String get recoveryBindingMissing =>
      '이 라이브러리에 연결된 보호 파일이 없어 선택한 파일을 대조할 수 없습니다.';

  @override
  String get recoveryBusy => '다른 프로세스가 라이브러리를 사용 중입니다. 다른 창을 닫고 다시 시도하세요.';

  @override
  String get recoveryChooseKey => '복구 파일 선택';

  @override
  String get recoveryCloseFirst => '작업 공간이 실행 중입니다. 라이브러리를 전환하기 전에 닫으세요.';

  @override
  String get recoveryClosing =>
      '기존 서비스가 종료되기를 기다리고 있습니다. 종료가 확인될 때까지 다시 열기와 복원을 사용할 수 없습니다.';

  @override
  String get recoveryClosingUnconfirmed =>
      '종료가 아직 확인되지 않았습니다. 계속 관찰하며, 시간 초과가 실행된 작업을 취소하지는 않습니다.';

  @override
  String get recoveryFailed => '복구가 완료되지 않았습니다. 원본 파일을 보존하고 다시 시도하세요.';

  @override
  String get recoveryIdentityBusy =>
      '이 라이브러리의 다른 사본이 사용 중입니다. 해당 작업 공간을 닫은 후 여세요.';

  @override
  String get recoveryIdentityMismatch =>
      '등록된 라이브러리 식별 정보가 일치하지 않습니다. 원본 데이터를 보존하고 올바른 백업을 복원하세요.';

  @override
  String get recoveryKeyFile => '라이브러리 보호 파일';

  @override
  String get recoveryKeyGuide =>
      '보호 파일이 없거나 손상되었다면 백업을 선택하세요. 이 라이브러리의 파일이어야 하며 원래 시스템 계정이 필요합니다.';

  @override
  String get recoveryKeyMismatch =>
      '키가 일치하지 않거나 복호화할 수 없습니다. 원래 파일과 시스템 계정을 사용하세요.';

  @override
  String get recoveryKeyUnknown =>
      '복구 결과를 확인해야 합니다. 다시 열어 보세요. 이전 보호 파일이 있었다면 사본을 보존했습니다.';

  @override
  String get recoveryLibraryInvalid =>
      '라이브러리를 검증하거나 열지 못했습니다. 원래 라이브러리와 보호 키를 보존하고 다시 시도하세요.';

  @override
  String get recoveryMaintenance =>
      '라이브러리 점검이 필요합니다. 원본 파일을 보존하고 진단 내용을 확인하세요.';

  @override
  String get recoveryMigrationIncomplete =>
      '이 라이브러리의 마이그레이션이 완료되지 않았습니다. 폴더의 마이그레이션 보고서를 확인하고 원본 라이브러리를 보존한 채 새 대상 폴더로 다시 시도하세요.';

  @override
  String get recoveryMissingKey =>
      '보호 키가 없습니다. 원래 .audit-key 파일을 복원하고 다시 시도하세요.';

  @override
  String get recoveryMissingLibrary =>
      '키는 있지만 라이브러리가 없거나 비어 있습니다. 원래 라이브러리를 복원하세요.';

  @override
  String get recoveryOpenFailed =>
      '작업 공간을 열지 못했습니다. 플러그인 파일과 데이터 폴더를 확인하고 다시 시도하세요.';

  @override
  String get recoveryPluginUnavailable =>
      '작업 공간 플러그인을 사용할 수 없습니다. 기존 콘텐츠는 보고 내보낼 수 있습니다.';

  @override
  String get recoveryRegistryInvalid =>
      '활성 라이브러리 등록이 손상되었거나 지원되지 않습니다. 데이터를 보호하기 위해 열기를 중단했습니다.';

  @override
  String get recoveryRegistryUnreadable =>
      '활성 라이브러리나 등록 파일을 읽을 수 없습니다. 원래 위치를 확인하세요. 대체 라이브러리는 자동 생성하지 않습니다.';

  @override
  String get recoveryRetry => '다시 시도';

  @override
  String get recoverySnapshot => '라이브러리 백업 복원';

  @override
  String get recoverySnapshotGuide =>
      '라이브러리 백업을 새 폴더에 복원한 뒤 전환할 수 있습니다. 원래 폴더는 유지됩니다. 백업 당시의 내용으로 복원되며 원래 시스템 계정이 필요합니다.';

  @override
  String get recoverySnapshotInvalid =>
      '백업 형식 또는 무결성 검사가 올바르지 않습니다. 원래 백업 파일을 보존하세요.';

  @override
  String get recoverySnapshotUnknown =>
      '복원 결과를 확인해야 합니다. 대상 폴더를 확인하세요. 원래 라이브러리는 교체하지 않았습니다.';

  @override
  String recoverySwitchUnconfirmed(String path) {
    return '$path에 복원했지만 전환은 확인되지 않았습니다. 이 폴더를 보존하고 작업 공간을 다시 열어 확인하세요.';
  }

  @override
  String get recoverySwitchUnknown =>
      '라이브러리 전환이 확인되지 않았습니다. 작업 공간을 다시 열어 확인하세요.';

  @override
  String get recoveryTargetExists => '복원 대상이 이미 있습니다. 아직 존재하지 않는 새 폴더를 선택하세요.';

  @override
  String get recoveryTitle => '작업 공간 다시 열기';

  @override
  String get shutdownBackground => '백그라운드에서 종료 계속';

  @override
  String get shutdownBackgroundHint =>
      '서비스가 종료되면 Morrow도 닫힙니다. 오류가 발생하거나 오래 걸리면 이 창이 다시 나타납니다.';

  @override
  String get shutdownFailure => '종료 중 문제가 발생했습니다. 콘텐츠 서비스를 계속 관찰하고 있습니다.';

  @override
  String get shutdownStillRunning => '종료가 예상보다 오래 걸립니다. 콘텐츠 서비스를 계속 관찰하고 있습니다.';

  @override
  String get shutdownTitle => '작업 공간 닫는 중';

  @override
  String get shutdownWaiting =>
      '콘텐츠 서비스가 종료되기를 기다리고 있습니다. 종료가 확인될 때까지 라이브러리는 기존 서비스가 소유합니다.';

  @override
  String get visualApplyColor => '색상 적용';

  @override
  String get visualApplyComponent => '이 구성 요소에 적용';

  @override
  String get visualApplyTexture => '미디어 적용';

  @override
  String visualAttachmentDetails(String action, String extension, String size) {
    return '$extension · $size · $action';
  }

  @override
  String get visualAttachmentFailure => '파일 작업에 실패했습니다. 파일과 저장 공간을 확인하세요.';

  @override
  String get visualAttachmentPreview => '로컬 첨부 미리보기';

  @override
  String get visualAttachmentReadFailure => '첨부 파일을 읽지 못했습니다. 다시 가져오세요.';

  @override
  String get visualAudio => '오디오';

  @override
  String get visualAudioStateFailure => '오디오 상태를 확인하지 못했습니다. 다시 시도하세요.';

  @override
  String get visualAutoLyrics => '없는 가사를 온라인에서 자동 검색';

  @override
  String get visualCancel => '취소';

  @override
  String get visualChangeCover => '앨범 표지 변경';

  @override
  String get visualChooseAudio => '오디오 파일 또는 이름이 같은 LRC 가사 파일을 선택하세요.';

  @override
  String get visualChooseLyrics => 'LRC 또는 TXT 가사 파일을 선택하세요.';

  @override
  String get visualClickPreview => '선택하여 미리보기';

  @override
  String get visualClose => '닫기';

  @override
  String get visualCloseDialog => '대화 상자 닫기';

  @override
  String get visualCloseWindow => '창 닫기';

  @override
  String get visualCollapsePlaylist => '재생 목록 접기';

  @override
  String get visualColorGuide =>
      '색상환을 드래그해 색조와 채도를 고른 뒤 밝기를 조정하세요. 색상 값을 입력할 수도 있습니다.';

  @override
  String get visualColorTitle => '내 공간에 색을 더하세요';

  @override
  String visualComponentCompass(String title) {
    return '$title · 색상환';
  }

  @override
  String get visualComponents => '구성 요소 및 카드';

  @override
  String get visualComponentsGuide =>
      '각 항목은 기본적으로 테마를 따릅니다. 다른 카드에 영향을 주지 않고 하나만 설정할 수 있습니다.';

  @override
  String get visualCornerTips1 =>
      '모든 아이디어가 유용할 필요는 없어요.\n오늘을 조금 더 흥미롭게 해 주기도 하죠.';

  @override
  String get visualCornerTips2 => '적어 두고 자라게 두세요.\n처음부터 완성된 아이디어일 필요는 없어요.';

  @override
  String get visualCornerTips3 => '나를 위한 여유를 남기세요.\n호기심에도 숨 쉴 공간이 필요해요.';

  @override
  String get visualCornerTips4 => '오늘 새로운 것을 시도하세요.\n작은 우회가 뜻밖의 발견을 줄 수 있어요.';

  @override
  String get visualCornerTips5 => '공상도 어딘가로 이어질 수 있어요.\n생각이 거닐 길을 내어 주세요.';

  @override
  String get visualCornerTips6 => '좋아하는 일에 시간을 쓰세요.\n그 가치를 증명할 필요는 없어요.';

  @override
  String get visualCornerTips7 => '발전은 작아도 괜찮아요.\n시작하려는 마음만으로도 의미가 있어요.';

  @override
  String get visualCornerTips8 => '가끔 창밖을 바라보세요.\n삶도 영감의 원천이니까요.';

  @override
  String get visualCover => '표지';

  @override
  String get visualCustomCompass => '색상환 · 사용자 지정';

  @override
  String get visualCustomMaterialGuide =>
      '끄면 테마를 따릅니다. 이 항목의 사용자 지정 설정은 보존됩니다.';

  @override
  String get visualDefaultOpen => '기본 앱으로 열기';

  @override
  String get visualDownloadOpen => '다운로드하여 열기';

  @override
  String get visualEmbeddedLyrics => '오디오에 내장됨';

  @override
  String get visualExpandPlaylist => '재생 목록 펼치기';

  @override
  String get visualFile => '파일';

  @override
  String get visualFileOpenFailure =>
      '파일을 열지 못했습니다. 호환 앱을 설치하거나 첨부 파일을 저장해 그 앱에서 여세요.';

  @override
  String get visualFileRetry => '파일 작업에 실패했습니다. 다시 시도하세요.';

  @override
  String get visualFindLyrics => '가사 찾기';

  @override
  String get visualFindLyricsGuide => 'LRCLIB에서 곡명과 아티스트로 검색한 뒤 맞는 버전을 선택하세요.';

  @override
  String visualFollowChain(String path) {
    return '연결 경로: $path';
  }

  @override
  String visualFollowComponent(String name) {
    return '따르는 대상: $name';
  }

  @override
  String get visualFollowCycle => '순환 참조 발생';

  @override
  String get visualFollowGuide => '다른 구성 요소의 설정을 따릅니다. 연결을 해제하면 자체 설정으로 돌아갑니다.';

  @override
  String get visualFollowTheme => '테마 따르기';

  @override
  String get visualFooterLyrics => '하단에 가사 표시';

  @override
  String get visualFooterTips => '하단에 팁 표시';

  @override
  String get visualFooterTips1 => '급한 일은 없어요. 호기심에 시간을 주세요.';

  @override
  String get visualFooterTips10 => '매 순간을 채우지 않아도 괜찮아요. 여유를 남기세요.';

  @override
  String get visualFooterTips2 => '생각을 적어 두세요. 정리는 나중에 해도 돼요.';

  @override
  String get visualFooterTips3 => '큰 아이디어를 오늘 할 작은 한 걸음으로 나누세요.';

  @override
  String get visualFooterTips4 => '가볍게 스트레칭하고 눈을 쉬게 하세요.';

  @override
  String get visualFooterTips5 => '아이디어에 지금 당장 답이 없어도 괜찮아요.';

  @override
  String get visualFooterTips6 => '속도를 늦출 때 찾아오는 발견도 있어요.';

  @override
  String get visualFooterTips7 => '작은 기록도 아이디어를 키우는 방법이에요.';

  @override
  String get visualFooterTips8 => '오늘의 짧은 메모가 내일의 시작이 될 수 있어요.';

  @override
  String get visualFooterTips9 => '마음이 거닐게 두었다가 좋아하는 일로 돌아오세요.';

  @override
  String get visualFrosting => '흐림 효과';

  @override
  String get visualGif => '움직이는 GIF';

  @override
  String get visualHexColor => 'HEX 색상';

  @override
  String get visualHexInvalid => '6자리 16진수 색상을 입력하세요.';

  @override
  String get visualImage => '이미지';

  @override
  String get visualImageDecodeFailure => '이미지를 디코딩할 수 없습니다. 저장한 뒤 다른 앱으로 여세요.';

  @override
  String visualImageLoadFailure(String name) {
    return '이미지를 불러오지 못했습니다: $name';
  }

  @override
  String visualImageNotImported(String name) {
    return '$name(가져오지 않은 이미지)';
  }

  @override
  String visualImageUnavailable(String name) {
    return '이미지 사용 불가: $name';
  }

  @override
  String get visualImportFailure => '가져오기에 실패했습니다. 파일, 인코딩, 저장 공간을 확인하세요.';

  @override
  String get visualImportLyrics => '가사 가져오기';

  @override
  String get visualImportMusic => '음악 가져오기';

  @override
  String get visualImportMusicHint => '+를 눌러 로컬 노래 가져오기';

  @override
  String get visualIndependentMaterial => '사용자 지정 질감';

  @override
  String get visualInheritColor => '테마 색조 사용';

  @override
  String get visualLinkFailure => '링크를 열지 못했습니다. 주소를 복사한 뒤 다시 시도하세요.';

  @override
  String visualLoadImage(String name) {
    return '이미지 불러오기 · $name';
  }

  @override
  String get visualLoading => '불러오는 중…';

  @override
  String get visualLyricsEmpty => '가사 파일이 비어 있습니다.';

  @override
  String get visualLyricsFile => '가사 파일';

  @override
  String get visualLyricsImportHint => '가사 파일을 가져오거나 온라인으로 검색하세요.';

  @override
  String get visualLyricsLoading => '가사 불러오는 중…';

  @override
  String visualLyricsMatch(String album, String kind, int seconds) {
    return '$album\n$kind · $seconds초';
  }

  @override
  String get visualLyricsMissing => '가사를 찾지 못했습니다. 파일을 가져오거나 다시 검색하세요.';

  @override
  String get visualLyricsNotFound => '가사를 찾지 못했습니다. 곡명이나 아티스트를 바꿔 보세요.';

  @override
  String get visualLyricsOnPlay => '재생 중 가사 자동 불러오기';

  @override
  String get visualLyricsParseFailure => '가사를 분석하지 못했습니다. 다시 가져오세요.';

  @override
  String get visualLyricsReadFailure => '가사를 불러오지 못했습니다. 직접 가져오거나 다시 시도하세요.';

  @override
  String get visualLyricsServiceFailure =>
      '가사 서비스에 연결하지 못했습니다. 나중에 다시 시도하거나 로컬 가사를 가져오세요.';

  @override
  String get visualLyricsSize => '가사 파일은 1 MB 이하여야 합니다.';

  @override
  String get visualLyricsSources => '로컬 파일 → 내장 가사 → LRCLIB';

  @override
  String get visualLyricsVersions => '여러 버전을 찾았습니다. 검색에서 하나를 선택하세요.';

  @override
  String get visualMaterialPreview => '질감 미리보기';

  @override
  String get visualMaterialSource => '재질 설정 출처';

  @override
  String get visualMaximize => '최대화';

  @override
  String get visualMediaAddress => '미디어 주소';

  @override
  String get visualMediaAddressInvalid =>
      '로그인 정보가 없는 올바른 HTTP 또는 HTTPS 주소를 입력하세요.';

  @override
  String get visualMediaPreviewFailure =>
      '이 미디어는 미리 볼 수 없습니다. 저장한 뒤 다른 앱으로 여세요.';

  @override
  String get visualMediaType => '미디어 유형';

  @override
  String get visualMinimize => '최소화';

  @override
  String get visualMusic => '음악';

  @override
  String get visualMusicEmptyTitle => '음악을 위한 공간';

  @override
  String get visualMusicPlayer => '음악 플레이어';

  @override
  String get visualNextTrack => '다음 곡';

  @override
  String get visualNoLyricsRead => '불러온 가사 없음';

  @override
  String visualNoLyricsTitle(String title) {
    return '♪ $title · 가사 없음';
  }

  @override
  String get visualNoTimeline => '시간 정보 없음';

  @override
  String get visualOpacity => '불투명도';

  @override
  String get visualOptionalArtist => '아티스트(선택 사항)';

  @override
  String get visualOwnMaterial => '테마 또는 자체 설정';

  @override
  String get visualPauseMusic => '음악 일시 정지';

  @override
  String get visualPaused => '일시 정지됨';

  @override
  String get visualPlainLyrics => '일반 텍스트 가사';

  @override
  String get visualPlayMusic => '음악 재생';

  @override
  String get visualPlaybackFailure =>
      '노래를 재생할 수 없습니다. 파일을 확인하거나 다른 오디오 형식을 사용하세요.';

  @override
  String visualPlaybackPosition(int count, int index, String state) {
    return '$index / $count · $state';
  }

  @override
  String get visualPlaybackRequestFailure => '재생을 시작하지 못했습니다. 다시 시도하세요.';

  @override
  String get visualPlaying => '재생 중';

  @override
  String get visualPlaylistEmpty => '재생 목록이 비어 있습니다';

  @override
  String get visualPlaylistLyricsHint => '재생 목록 메뉴에서 LRC 가사 가져오기';

  @override
  String get visualPlaylistSaved => '재생 목록과 가사는 자동 저장됩니다';

  @override
  String get visualPlaylistUpdateFailure => '재생 목록을 갱신하지 못했습니다. 다시 시도하세요.';

  @override
  String get visualPreviewColor => '색상 미리보기';

  @override
  String get visualPreviousTrack => '이전 곡';

  @override
  String get visualRemoveAttachment => '첨부 파일 제거';

  @override
  String get visualRemoveTrack => '재생 목록에서 제거';

  @override
  String get visualResetMaterial => '테마로 초기화';

  @override
  String get visualRestoreWindow => '이전 크기로';

  @override
  String get visualSaveAttachment => '다른 이름으로 첨부 저장';

  @override
  String get visualSearch => '검색';

  @override
  String get visualSearchLyrics => '가사 검색';

  @override
  String get visualSongCover => '앨범 표지';

  @override
  String get visualSongTitle => '곡 제목';

  @override
  String get visualSyncedLyrics => '동기화된 가사';

  @override
  String get visualTextureFailure =>
      '미디어를 불러오지 못했습니다. 파일, 주소, 형식을 확인하세요. 웹 미디어는 교차 출처 접근도 허용해야 합니다.';

  @override
  String get visualTextureLinkGuide =>
      '이미지, GIF, 동영상의 직접 HTTP 또는 HTTPS 링크를 붙여넣으세요. 공유 웹페이지는 원본 미디어 주소를 먼저 찾으세요.';

  @override
  String get visualTextureLinkTitle => '영감 불러오기';

  @override
  String get visualTexturePlaybackGuide =>
      '동영상은 기본적으로 무음 반복 재생됩니다. 설정에서 소리를 켤 수 있습니다. 온라인 미디어는 웹의 교차 출처 로딩을 포함해 접근을 허용해야 합니다.';

  @override
  String get visualTipsMaterialGuide =>
      '끄면 팁을 투명하게 표시하고 켜면 아래 질감을 사용합니다. 사용자 값은 유지됩니다.';

  @override
  String get visualTransparentTips => '투명 오버레이(기본값)';

  @override
  String get visualUseCustomMaterial => '사용자 지정 질감 사용';

  @override
  String get visualVideo => '동영상';

  @override
  String get visualViewLyrics => '가사 보기';
}
