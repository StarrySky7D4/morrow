import 'dart:typed_data';
import 'package:idb_shim/idb_browser.dart';

Future<Database> _open() => idbFactoryWeb.open(
  'morrow-fonts',
  version: 1,
  onUpgradeNeeded: (e) => e.database.createObjectStore('fonts'),
);
Future<void> store(String id, Uint8List bytes) async {
  final db = await _open();
  try {
    final tx = db.transaction('fonts', idbModeReadWrite);
    await tx.objectStore('fonts').put(bytes, id);
    await tx.completed;
  } finally {
    db.close();
  }
}

Future<Uint8List> read(String id, int limit, {String? libraryDirectory}) async {
  final db = await _open();
  try {
    final tx = db.transaction('fonts', idbModeReadOnly);
    final bytes = await tx.objectStore('fonts').getObject(id);
    await tx.completed;
    if (bytes is! Uint8List || bytes.length > limit) {
      throw const FormatException('Font unavailable');
    }
    return bytes;
  } finally {
    db.close();
  }
}
