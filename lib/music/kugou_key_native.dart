import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:typed_data';
import 'package:ffi/ffi.dart';
import 'kugou_database.dart';

typedef _Progress = Int32 Function(Pointer<Void>);

Future<String?> lookupKugouKey(String keyId, {String? databasePath}) async {
  if (!Platform.isWindows ||
      keyId.isEmpty ||
      utf8.encode(keyId).length > 16384 ||
      keyId.contains('\u0000')) {
    return null;
  }
  final appData = Platform.environment['APPDATA'];
  if (databasePath == null && (appData == null || appData.isEmpty)) return null;
  final file = File(databasePath ?? '$appData/Kugou8/KGMusicV3.db');
  if (!await file.exists()) return null;
  final before = await file.stat();
  if (before.size < 1024 || before.size > maxKugouDatabaseBytes) return null;
  final encrypted = await file.readAsBytes();
  final after = await file.stat();
  if (before.size != after.size ||
      before.modified != after.modified ||
      encrypted.length != before.size) {
    throw const FormatException('酷狗数据库正在更新，请稍后重试。');
  }
  final clear = await decodeKugouDatabase(encrypted);
  try {
    return queryKugouKey(clear, keyId);
  } finally {
    clear.fillRange(0, clear.length, 0);
  }
}

String? queryKugouKey(Uint8List clear, String keyId) {
  if (clear.length < 1024 ||
      clear.length > maxKugouDatabaseBytes ||
      keyId.isEmpty ||
      keyId.contains('\u0000') ||
      utf8.encode(keyId).length > 16384) {
    return null;
  }
  final systemRoot = Platform.environment['SystemRoot'] ?? r'C:\Windows';
  final lib = DynamicLibrary.open('$systemRoot/System32/winsqlite3.dll');
  final open = lib
      .lookupFunction<
        Int32 Function(
          Pointer<Utf8>,
          Pointer<Pointer<Void>>,
          Int32,
          Pointer<Utf8>,
        ),
        int Function(Pointer<Utf8>, Pointer<Pointer<Void>>, int, Pointer<Utf8>)
      >('sqlite3_open_v2');
  final close = lib
      .lookupFunction<
        Int32 Function(Pointer<Void>),
        int Function(Pointer<Void>)
      >('sqlite3_close');
  final deserialize = lib
      .lookupFunction<
        Int32 Function(
          Pointer<Void>,
          Pointer<Utf8>,
          Pointer<Uint8>,
          Int64,
          Int64,
          Uint32,
        ),
        int Function(
          Pointer<Void>,
          Pointer<Utf8>,
          Pointer<Uint8>,
          int,
          int,
          int,
        )
      >('sqlite3_deserialize');
  final prepare = lib
      .lookupFunction<
        Int32 Function(
          Pointer<Void>,
          Pointer<Utf8>,
          Int32,
          Pointer<Pointer<Void>>,
          Pointer<Pointer<Utf8>>,
        ),
        int Function(
          Pointer<Void>,
          Pointer<Utf8>,
          int,
          Pointer<Pointer<Void>>,
          Pointer<Pointer<Utf8>>,
        )
      >('sqlite3_prepare_v2');
  final bind = lib
      .lookupFunction<
        Int32 Function(
          Pointer<Void>,
          Int32,
          Pointer<Utf8>,
          Int32,
          Pointer<Void>,
        ),
        int Function(Pointer<Void>, int, Pointer<Utf8>, int, Pointer<Void>)
      >('sqlite3_bind_text');
  final step = lib
      .lookupFunction<
        Int32 Function(Pointer<Void>),
        int Function(Pointer<Void>)
      >('sqlite3_step');
  final finalize = lib
      .lookupFunction<
        Int32 Function(Pointer<Void>),
        int Function(Pointer<Void>)
      >('sqlite3_finalize');
  final column = lib
      .lookupFunction<
        Pointer<Uint8> Function(Pointer<Void>, Int32),
        Pointer<Uint8> Function(Pointer<Void>, int)
      >('sqlite3_column_text');
  final size = lib
      .lookupFunction<
        Int32 Function(Pointer<Void>, Int32),
        int Function(Pointer<Void>, int)
      >('sqlite3_column_bytes');
  final progress = lib
      .lookupFunction<
        Void Function(
          Pointer<Void>,
          Int32,
          Pointer<NativeFunction<_Progress>>,
          Pointer<Void>,
        ),
        void Function(
          Pointer<Void>,
          int,
          Pointer<NativeFunction<_Progress>>,
          Pointer<Void>,
        )
      >('sqlite3_progress_handler');
  final limit = lib
      .lookupFunction<
        Int32 Function(Pointer<Void>, Int32, Int32),
        int Function(Pointer<Void>, int, int)
      >('sqlite3_limit');
  final db = calloc<Pointer<Void>>();
  final statement = calloc<Pointer<Void>>();
  final memory = calloc<Uint8>(clear.length);
  final filename = ':memory:'.toNativeUtf8();
  final sql =
      ("SELECT DISTINCT EncryptionKey FROM ShareFileItems "
              "WHERE EncryptionKeyId = ? COLLATE NOCASE "
              "AND EncryptionKey IS NOT NULL AND EncryptionKey != '' LIMIT 2")
          .toNativeUtf8();
  final id = keyId.toNativeUtf8();
  var ticks = 0;
  final callback = NativeCallable<_Progress>.isolateLocal(
    (Pointer<Void> _) => ++ticks > 10000 ? 1 : 0,
    exceptionalReturn: 1,
  );
  try {
    memory.asTypedList(clear.length).setAll(0, clear);
    if (open(filename, db, 6, nullptr) != 0) return null;
    if (deserialize(db.value, nullptr, memory, clear.length, clear.length, 4) !=
        0) {
      return null;
    }
    progress(db.value, 1000, callback.nativeFunction, nullptr);
    limit(db.value, 0, 1024 * 1024);
    limit(db.value, 1, 8192);
    limit(db.value, 3, 32);
    if (prepare(db.value, sql, -1, statement, nullptr) != 0) return null;
    if (bind(statement.value, 1, id, -1, nullptr) != 0) return null;
    if (step(statement.value) != 100) return null;
    final pointer = column(statement.value, 0);
    final length = size(statement.value, 0);
    if (pointer == nullptr || length <= 0 || length > 16384) return null;
    final bytes = pointer.asTypedList(length);
    if (bytes.any((value) => value > 127)) return null;
    final value = ascii.decode(bytes).trim();
    // Reject duplicate/conflicting matches rather than guessing a key.
    if (step(statement.value) != 101) return null;
    if (!RegExp(r'^[A-Za-z0-9+/]+={0,2}$').hasMatch(value)) return null;
    return value;
  } finally {
    if (statement.value != nullptr) finalize(statement.value);
    if (db.value != nullptr) close(db.value);
    callback.close();
    memory.asTypedList(clear.length).fillRange(0, clear.length, 0);
    calloc.free(memory);
    calloc.free(db);
    calloc.free(statement);
    calloc.free(filename);
    calloc.free(sql);
    calloc.free(id);
  }
}
