/// Stable persisted language identifiers and self-names; usable without Flutter.
const uiLanguageNames = <String, String>{
  'zh': '简体中文',
  'en': 'English',
  'ru': 'Русский',
  'fr': 'Français',
  'de': 'Deutsch',
  'es': 'Español',
  'ja': '日本語',
  'ko': '한국어',
  'pt': 'Português',
};

bool isUiLocale(String value) =>
    value == 'system' || uiLanguageNames.containsKey(value);
