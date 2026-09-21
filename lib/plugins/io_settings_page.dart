import 'package:flutter/material.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import '../appearance.dart';
import '../settings_surface.dart';
import 'plugin_library.dart';

class IoSettingsPage extends StatelessWidget {
  const IoSettingsPage({
    super.key,
    required this.backend,
    required this.onChanged,
  });
  final ExternalPluginControl backend;
  final VoidCallback onChanged;

  @override
  Widget build(BuildContext context) {
    final p = AppearanceScope.of(context);
    return SettingsSurface(
      key: const ValueKey('io-settings-page'),
      title: L10n.of(context).mainIoSettings,
      child: SingleChildScrollView(
        key: const PageStorageKey('io-settings-scroll'),
        padding: const EdgeInsets.only(bottom: 20),
        child: PluginLibrary(
          mode: PluginLibraryMode.io,
          backend: backend,
          onChanged: onChanged,
          ink: p.ink,
          muted: p.muted,
          line: p.line,
          radius: p.borderRadius(12),
        ),
      ),
    );
  }
}
