import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import '../settings_surface.dart';
import 'font_choice.dart';
import 'font_repository.dart';

class FontScope extends InheritedWidget {
  const FontScope({
    super.key,
    required this.value,
    required this.onChanged,
    required super.child,
  });
  final FontChoice value;
  final Future<void> Function(FontChoice) onChanged;
  static FontScope of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<FontScope>()!;
  @override
  bool updateShouldNotify(FontScope oldWidget) => value != oldWidget.value;
}

class FontSettingsPage extends StatefulWidget {
  const FontSettingsPage({super.key, this.pick});
  final Future<XFile?> Function()? pick;
  @override
  State<FontSettingsPage> createState() => _FontSettingsPageState();
}

class _FontSettingsPageState extends State<FontSettingsPage> {
  final _family = TextEditingController();
  bool _initialized = false, _busy = false;
  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (!_initialized) {
      _initialized = true;
      _family.text = FontScope.of(context).value.family;
    }
  }

  @override
  void dispose() {
    _family.dispose();
    super.dispose();
  }

  Future<void> _apply(Future<FontChoice?> Function() get) async {
    if (_busy) return;
    final scope = FontScope.of(context);
    setState(() => _busy = true);
    try {
      final font = await get();
      if (!mounted || font == null) return;
      await scope.onChanged(font);
      if (mounted) _family.text = font.family;
    } catch (_) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(L10n.of(context).mainFontFailed)),
        );
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final l = L10n.of(context), font = FontScope.of(context).value;
    return SettingsSurface(
      title: l.mainFontSettings,
      child: ListView(
        padding: const EdgeInsets.symmetric(vertical: 16),
        children: [
          Text(
            font.imported
                ? font.name
                : font.family.isEmpty
                ? l.mainFontDefault
                : font.family,
            style: Theme.of(context).textTheme.titleMedium,
          ),
          const SizedBox(height: 16),
          TextField(
            key: const ValueKey('font-family'),
            controller: _family,
            enabled: !_busy,
            maxLength: 128,
            decoration: InputDecoration(
              labelText: l.mainFontFamily,
              hintText: l.mainFontHint,
            ),
            onSubmitted: (_) =>
                _apply(() async => FontChoice(family: _family.text.trim())),
          ),
          const SizedBox(height: 12),
          Wrap(
            spacing: 12,
            runSpacing: 12,
            children: [
              FilledButton(
                key: const ValueKey('font-apply'),
                onPressed: _busy
                    ? null
                    : () => _apply(
                        () async => FontChoice(family: _family.text.trim()),
                      ),
                child: Text(l.mainFontApply),
              ),
              OutlinedButton.icon(
                key: const ValueKey('font-import'),
                icon: const Icon(Icons.upload_file),
                label: Text(l.mainFontImport),
                onPressed: _busy
                    ? null
                    : () => _apply(() async {
                        final file =
                            await (widget.pick?.call() ??
                                openFile(
                                  acceptedTypeGroups: const [
                                    XTypeGroup(
                                      label: 'TrueType / OpenType',
                                      extensions: ['ttf', 'otf'],
                                      uniformTypeIdentifiers: [
                                        'public.truetype-ttf-font',
                                        'public.opentype-font',
                                      ],
                                    ),
                                  ],
                                ));
                        return file == null
                            ? null
                            : await FontRepository.importFile(file);
                      }),
              ),
              TextButton(
                key: const ValueKey('font-reset'),
                onPressed: _busy
                    ? null
                    : () => _apply(() async => const FontChoice()),
                child: Text(l.mainFontReset),
              ),
            ],
          ),
          const SizedBox(height: 16),
          Text(l.mainFontHelp),
          const SizedBox(height: 24),
          const Text(
            'Morrow · Aa 123\n中文 · 日本語 · 한국어 · Русский',
            style: TextStyle(fontSize: 22, height: 1.6),
          ),
          if (_busy)
            const Padding(
              padding: EdgeInsets.only(top: 16),
              child: LinearProgressIndicator(),
            ),
        ],
      ),
    );
  }
}
