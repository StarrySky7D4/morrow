import 'package:file_selector/file_selector.dart';
import 'package:material_ui/material_ui.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import '../appearance.dart';
import '../settings_surface.dart';
import '../ui/workbench_strings.dart';
import 'font_choice.dart';
import 'font_repository.dart';
import 'system_fonts.dart';

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
  const FontSettingsPage({super.key, this.pick, this.loadSystemFonts});
  final Future<XFile?> Function()? pick;
  final Future<List<SystemFontEntry>> Function({String? locale, bool refresh})?
  loadSystemFonts;

  @override
  State<FontSettingsPage> createState() => _FontSettingsPageState();
}

class _FontSettingsPageState extends State<FontSettingsPage> {
  final _family = TextEditingController();
  final _search = TextEditingController();
  bool _initialized = false, _busy = false, _loadingFonts = false;
  List<SystemFontEntry> _systemFonts = const [];
  String _loadedLocale = '';
  int _fontLoadGeneration = 0;

  @override
  void initState() {
    super.initState();
    _search.addListener(() {
      if (mounted) setState(() {});
    });
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (!_initialized) {
      _initialized = true;
      _family.text = FontScope.of(context).value.family;
    }
    final localeTag = Localizations.localeOf(context).toLanguageTag();
    if (_loadedLocale != localeTag) {
      _loadedLocale = localeTag;
      _loadingFonts = true;
      _loadFonts(localeTag: localeTag, notifyStart: false);
    }
  }

  @override
  void dispose() {
    _family.dispose();
    _search.dispose();
    super.dispose();
  }

  Future<void> _loadFonts({
    String? localeTag,
    bool refresh = false,
    bool notifyStart = true,
  }) async {
    final targetLocale = localeTag ?? _loadedLocale;
    final generation = ++_fontLoadGeneration;
    if (notifyStart && mounted) {
      setState(() => _loadingFonts = true);
    }
    try {
      final loader = widget.loadSystemFonts ?? SystemFontCatalog.load;
      final fonts = await loader(locale: targetLocale, refresh: refresh);
      if (!mounted || generation != _fontLoadGeneration) return;
      setState(() {
        _systemFonts = fonts;
        _loadingFonts = false;
      });
    } catch (_) {
      if (!mounted || generation != _fontLoadGeneration) return;
      setState(() => _loadingFonts = false);
    }
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

  Widget _buildDefaultRow({
    required Palette p,
    required AppLocalizations l,
    required WorkbenchStrings ws,
    required bool selected,
  }) => _SystemFontOptionRow(
    rowKey: const ValueKey('system-font-default'),
    p: p,
    title: l.mainFontDefault,
    subtitle: ws.fontPickerDefaultSubtitle,
    sampleFamily: 'Segoe UI',
    selected: selected,
    enabled: !_busy,
    onTap: () => _apply(() async => const FontChoice()),
  );

  Widget _buildFontRow({
    required Palette p,
    required SystemFontEntry entry,
    required bool selected,
  }) => _SystemFontOptionRow(
    rowKey: ValueKey('system-font-${entry.family}'),
    p: p,
    title: entry.displayName,
    subtitle: entry.hasDistinctDisplayName ? entry.family : null,
    sampleFamily: entry.family,
    selected: selected,
    enabled: !_busy,
    onTap: () => _apply(() async => FontChoice(family: entry.family)),
  );

  @override
  Widget build(BuildContext context) {
    final l = L10n.of(context);
    final ws = WorkbenchStrings.of(context);
    final p = AppearanceScope.of(context);
    final font = FontScope.of(context).value;
    final query = _search.text.trim();
    final filtered = query.isEmpty
        ? _systemFonts
        : _systemFonts.where((e) => e.matchesQuery(query)).toList();
    final showDefaultRow = query.isEmpty;
    final totalRows = (showDefaultRow ? 1 : 0) + filtered.length;
    final activeLabel = font.imported
        ? font.name
        : font.family.isEmpty
        ? l.mainFontDefault
        : font.family;

    return SettingsSurface(
      title: l.mainFontSettings,
      child: SingleChildScrollView(
        padding: const EdgeInsets.only(bottom: 20),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Glass(
              p: p,
              radius: 16,
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Container(
                          padding: const EdgeInsets.symmetric(
                            horizontal: 8,
                            vertical: 3,
                          ),
                          decoration: BoxDecoration(
                            color: p.accent.withValues(alpha: 0.14),
                            borderRadius: p.borderRadius(999),
                          ),
                          child: Text(
                            ws.fontPickerActiveBadge,
                            style: TextStyle(
                              fontSize: 11.5,
                              fontWeight: FontWeight.w600,
                              color: p.accent,
                            ),
                          ),
                        ),
                        const SizedBox(width: 10),
                        Expanded(
                          child: Text(
                            activeLabel,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: Theme.of(context).textTheme.titleMedium
                                ?.copyWith(
                                  fontWeight: FontWeight.w600,
                                  color: p.ink,
                                ),
                          ),
                        ),
                        const SizedBox(width: 8),
                        TextButton(
                          key: const ValueKey('font-reset'),
                          onPressed: _busy
                              ? null
                              : () => _apply(() async => const FontChoice()),
                          child: Text(l.mainFontReset),
                        ),
                      ],
                    ),
                    const SizedBox(height: 10),
                    Text(
                      'Morrow · Aa 123\n中文 · 日本語 · 한국어 · Русский',
                      style: TextStyle(
                        fontSize: 22,
                        height: 1.45,
                        color: p.ink,
                      ),
                    ),
                    if (_busy)
                      const Padding(
                        padding: EdgeInsets.only(top: 10),
                        child: LinearProgressIndicator(),
                      ),
                  ],
                ),
              ),
            ),
            const SizedBox(height: 14),
            Glass(
              key: const ValueKey('system-font-picker'),
              p: p,
              radius: 16,
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Row(
                      children: [
                        Icon(
                          Icons.font_download_outlined,
                          size: 18,
                          color: p.ink,
                        ),
                        const SizedBox(width: 8),
                        Text(
                          ws.fontPickerTitle,
                          style: Theme.of(context).textTheme.titleSmall
                              ?.copyWith(
                                fontWeight: FontWeight.w600,
                                color: p.ink,
                              ),
                        ),
                        const SizedBox(width: 8),
                        if (_systemFonts.isNotEmpty)
                          Container(
                            padding: const EdgeInsets.symmetric(
                              horizontal: 8,
                              vertical: 2,
                            ),
                            decoration: BoxDecoration(
                              color: p.ink.withValues(alpha: 0.06),
                              borderRadius: p.borderRadius(999),
                            ),
                            child: Text(
                              ws.fontPickerCount(
                                filtered.length,
                                _systemFonts.length,
                              ),
                              style: TextStyle(fontSize: 11.5, color: p.muted),
                            ),
                          ),
                        const Spacer(),
                        IconButton(
                          key: const ValueKey('system-font-refresh'),
                          tooltip: ws.fontPickerRefreshTooltip,
                          iconSize: 18,
                          visualDensity: VisualDensity.compact,
                          onPressed: _loadingFonts
                              ? null
                              : () => _loadFonts(refresh: true),
                          icon: const Icon(Icons.refresh_rounded),
                        ),
                      ],
                    ),
                    const SizedBox(height: 10),
                    TextField(
                      key: const ValueKey('system-font-search'),
                      controller: _search,
                      enabled: !_busy,
                      decoration: InputDecoration(
                        isDense: true,
                        hintText: ws.fontPickerSearchHint,
                        prefixIcon: const Icon(Icons.search_rounded, size: 18),
                        suffixIcon: _search.text.isEmpty
                            ? null
                            : IconButton(
                                iconSize: 18,
                                onPressed: () => _search.clear(),
                                icon: const Icon(Icons.close_rounded),
                              ),
                      ),
                    ),
                    const SizedBox(height: 10),
                    Container(
                      height: 240,
                      decoration: BoxDecoration(
                        borderRadius: p.borderRadius(12),
                        border: Border.all(color: p.line),
                        color: p.surface.withValues(alpha: 0.28),
                      ),
                      child: _loadingFonts && _systemFonts.isEmpty
                          ? Center(
                              child: Row(
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  Icon(
                                    Icons.font_download_outlined,
                                    size: 16,
                                    color: p.muted,
                                  ),
                                  const SizedBox(width: 8),
                                  Text(
                                    ws.fontPickerLoading,
                                    style: TextStyle(color: p.muted),
                                  ),
                                ],
                              ),
                            )
                          : filtered.isEmpty && query.isNotEmpty
                          ? Center(
                              child: Text(
                                ws.fontPickerEmpty,
                                style: TextStyle(color: p.muted),
                              ),
                            )
                          : ClipRRect(
                              borderRadius: p.borderRadius(12),
                              child: ListView.builder(
                                key: const ValueKey('system-font-list'),
                                primary: false,
                                itemExtent: 52,
                                itemCount: totalRows,
                                itemBuilder: (context, index) {
                                  if (showDefaultRow && index == 0) {
                                    return _buildDefaultRow(
                                      p: p,
                                      l: l,
                                      ws: ws,
                                      selected:
                                          !font.imported && font.family.isEmpty,
                                    );
                                  }
                                  final fontIndex = showDefaultRow
                                      ? index - 1
                                      : index;
                                  final entry = filtered[fontIndex];
                                  final selected =
                                      !font.imported &&
                                      font.family.toLowerCase() ==
                                          entry.family.toLowerCase();
                                  return _buildFontRow(
                                    p: p,
                                    entry: entry,
                                    selected: selected,
                                  );
                                },
                              ),
                            ),
                    ),
                  ],
                ),
              ),
            ),
            const SizedBox(height: 14),
            Glass(
              p: p,
              radius: 16,
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      ws.fontPickerManualTitle,
                      style: Theme.of(context).textTheme.titleSmall?.copyWith(
                        fontWeight: FontWeight.w600,
                        color: p.ink,
                      ),
                    ),
                    const SizedBox(height: 10),
                    TextField(
                      key: const ValueKey('font-family'),
                      controller: _family,
                      enabled: !_busy,
                      maxLength: 128,
                      decoration: InputDecoration(
                        isDense: true,
                        labelText: l.mainFontFamily,
                        hintText: l.mainFontHint,
                      ),
                      onSubmitted: (_) => _apply(
                        () async => FontChoice(family: _family.text.trim()),
                      ),
                    ),
                    const SizedBox(height: 8),
                    Wrap(
                      spacing: 12,
                      runSpacing: 12,
                      children: [
                        FilledButton(
                          key: const ValueKey('font-apply'),
                          onPressed: _busy
                              ? null
                              : () => _apply(
                                  () async =>
                                      FontChoice(family: _family.text.trim()),
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
                      ],
                    ),
                    const SizedBox(height: 8),
                    Text(
                      l.mainFontHelp,
                      style: TextStyle(fontSize: 12, color: p.muted),
                    ),
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _SystemFontOptionRow extends StatelessWidget {
  const _SystemFontOptionRow({
    required this.rowKey,
    required this.p,
    required this.title,
    required this.subtitle,
    required this.sampleFamily,
    required this.selected,
    required this.enabled,
    required this.onTap,
  });

  final Key rowKey;
  final Palette p;
  final String title;
  final String? subtitle;
  final String sampleFamily;
  final bool selected;
  final bool enabled;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final background = selected
        ? p.accent.withValues(alpha: p.dark ? 0.22 : 0.12)
        : Colors.transparent;
    return Material(
      type: MaterialType.transparency,
      child: InkWell(
        key: rowKey,
        onTap: enabled ? onTap : null,
        child: AnimatedContainer(
          duration: const Duration(milliseconds: 160),
          curve: Curves.easeOutCubic,
          padding: const EdgeInsets.symmetric(horizontal: 12),
          decoration: BoxDecoration(
            color: background,
            border: Border(
              bottom: BorderSide(color: p.line.withValues(alpha: 0.45)),
            ),
          ),
          child: Row(
            children: [
              Expanded(
                child: Column(
                  mainAxisAlignment: MainAxisAlignment.center,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      title,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        fontSize: 13.5,
                        height: 1.25,
                        fontWeight: selected
                            ? FontWeight.w600
                            : FontWeight.w500,
                        color: selected ? p.accent : p.ink,
                      ),
                    ),
                    if (subtitle != null)
                      Text(
                        subtitle!,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: TextStyle(
                          fontSize: 11.5,
                          height: 1.2,
                          color: p.muted,
                        ),
                      ),
                  ],
                ),
              ),
              const SizedBox(width: 12),
              ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 112),
                child: Text(
                  'Aa 永 123',
                  maxLines: 1,
                  softWrap: false,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontFamily: sampleFamily,
                    fontFamilyFallback: const [
                      'Segoe UI',
                      'Microsoft YaHei',
                      'Arial',
                    ],
                    fontSize: 14,
                    color: selected ? p.accent : p.muted,
                  ),
                ),
              ),
              if (selected) ...[
                const SizedBox(width: 8),
                Icon(Icons.check_rounded, size: 17, color: p.accent),
              ],
            ],
          ),
        ),
      ),
    );
  }
}
