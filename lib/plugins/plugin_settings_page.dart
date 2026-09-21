import 'package:flutter/material.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import '../appearance.dart';
import '../settings_surface.dart';
import 'plugin_tools.dart';
import 'plugin_library.dart';
import 'protection_backup.dart';
import 'workbench_backend.dart';

class PluginSettingsPage extends StatelessWidget {
  const PluginSettingsPage({
    super.key,
    required this.backend,
    required this.onChanged,
  });
  final Object backend;
  final VoidCallback onChanged;

  @override
  Widget build(BuildContext context) {
    final p = AppearanceScope.of(context);
    final backend = this.backend;
    return SettingsSurface(
      key: const ValueKey('plugin-settings-page'),
      title: L10n.of(context).mainPluginSettings,
      child: SingleChildScrollView(
        key: const PageStorageKey('plugin-settings-scroll'),
        padding: const EdgeInsets.only(bottom: 20),
        child: SettingsSections(
          children: [
            if (backend is WorkbenchPluginControl)
              Glass(
                p: p,
                componentId: 'plugin-tools',
                child: PluginTools(
                  backend: backend,
                  onChanged: onChanged,
                  ink: p.ink,
                  muted: p.muted,
                  line: p.line,
                  radius: p.borderRadius(11),
                ),
              ),
            if (backend is ExternalPluginControl)
              Glass(
                p: p,
                componentId: 'plugin-library',
                child: PluginLibrary(
                  backend: backend,
                  onChanged: onChanged,
                  ink: p.ink,
                  muted: p.muted,
                  line: p.line,
                  radius: p.borderRadius(11),
                ),
              ),
            if (backend is WorkbenchProtectionBackup)
              Glass(
                p: p,
                componentId: 'protection-backup',
                child: ProtectionBackup(
                  onBackup: backend.backupProtection,
                  onSnapshot: backend.backupSnapshot,
                  ink: p.ink,
                  muted: p.muted,
                  line: p.line,
                  radius: p.borderRadius(11),
                ),
              ),
          ],
        ),
      ),
    );
  }
}
