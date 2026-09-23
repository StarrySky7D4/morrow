import 'dart:io';
import 'dart:typed_data';
import 'package:path_provider/path_provider.dart';

Future<File> _file(String id) async {
  if (!RegExp(r'^[0-9a-f]{64}$').hasMatch(id)) {
    throw const FormatException('Invalid font ID');
  }
  final directory = Directory(
    '${(await getApplicationSupportDirectory()).path}/fonts',
  );
  await directory.create(recursive: true);
  return File('${directory.path}/$id.sfnt');
}

Future<void> store(String id, Uint8List bytes) async {
  final file = await _file(id);
  final pending = File(
    '${file.path}.${DateTime.now().microsecondsSinceEpoch}.tmp',
  );
  await pending.writeAsBytes(bytes, flush: true);
  await pending.rename(file.path);
}

Future<Uint8List> read(String id, int limit, {String? libraryDirectory}) async {
  if (!RegExp(r'^[0-9a-f]{64}$').hasMatch(id)) {
    throw const FormatException('Invalid font ID');
  }
  final local = libraryDirectory == null
      ? null
      : File('$libraryDirectory/fonts/$id.sfnt');
  final file = local != null && await local.exists() ? local : await _file(id);
  if (await file.length() > limit) {
    throw const FormatException('Font too large');
  }
  return file.readAsBytes();
}
