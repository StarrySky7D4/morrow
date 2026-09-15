import 'package:flutter/widgets.dart';
import 'package:flutter/services.dart';
import 'generated/app_localizations.dart';
import 'language_pack.dart';

/// Use [delegate] in the app: it validates bundled PB/LZ4 before intl execution.
abstract final class L10n {
  static AppLocalizations of(BuildContext context) =>
      AppLocalizations.of(context) ?? forLocale(const Locale('zh'));
  static AppLocalizations forLocale(Locale locale) =>
      lookupAppLocalizations(Locale(locale.languageCode == 'en' ? 'en' : 'zh'));
  static const LocalizationsDelegate<AppLocalizations> delegate =
      _VerifiedDelegate();
  static List<Locale> get supportedLocales => AppLocalizations.supportedLocales;
}

class _VerifiedDelegate extends LocalizationsDelegate<AppLocalizations> {
  const _VerifiedDelegate();
  @override
  bool isSupported(Locale locale) => ['en', 'zh'].contains(locale.languageCode);
  @override
  Future<AppLocalizations> load(Locale locale) async {
    final code = locale.languageCode;
    final data = await rootBundle.load(
      'packages/morrow_i18n/assets/languages/$code.mlang',
    );
    LanguagePackCodec.validate(
      data.buffer.asUint8List(data.offsetInBytes, data.lengthInBytes),
      code,
    );
    return AppLocalizations.delegate.load(locale);
  }

  @override
  bool shouldReload(_VerifiedDelegate old) => false;
}
