import 'dart:io';

import 'package:path_provider/path_provider.dart';

/// Windows derives its preferences directory from the executable's metadata.
/// Import the old snapshot once; never overwrite Morrow data or delete sources.
Future<void> migrateLegacyStorage() async {
  if (!Platform.isWindows) return;
  final current = await getApplicationSupportDirectory();
  final legacy = Directory(
    '${current.parent.parent.path}/dev.daemon/daemon_studio',
  );
  await copyLegacyPreferences(legacy: legacy, current: current);
}

Future<void> copyLegacyPreferences({
  required Directory legacy,
  required Directory current,
}) async {
  final target = File('${current.path}/shared_preferences.json');
  final source = File('${legacy.path}/shared_preferences.json');
  if (await target.exists() || !await source.exists()) return;
  await current.create(recursive: true);
  // Stored media uses absolute paths. Keep the original textures directory so
  // existing attachments, backgrounds, and music continue to resolve there.
  final staging = File(
    '${target.path}.$pid.${DateTime.now().microsecondsSinceEpoch}.migrating',
  );
  try {
    await staging.writeAsBytes(await source.readAsBytes(), flush: true);
    if (!await target.exists()) await staging.rename(target.path);
  } finally {
    if (await staging.exists()) await staging.delete();
  }
}
