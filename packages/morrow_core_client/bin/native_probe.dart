import 'dart:typed_data';
import 'dart:ffi';
import 'package:morrow_core_client/morrow_core_client.dart';
import 'package:morrow_core_client/native_bridge.dart';

void main(List<String> args) {
  if (args.length != 1)
    throw ArgumentError('Pass the core dynamic library path');
  final library = DynamicLibrary.open(args.single);
  final bridge = NativeCoreBridge.library(library);
  final allocate = library
      .lookupFunction<Uint32 Function(Uint32), int Function(int)>(
        'morrow_buffer_new',
      );
  final release = library
      .lookupFunction<Uint32 Function(Uint32), int Function(int)>(
        'morrow_buffer_free',
      );
  final pointer = library
      .lookupFunction<
        Pointer<Uint8> Function(Uint32),
        Pointer<Uint8> Function(int)
      >('morrow_buffer_ptr');
  final handles = <int>[];
  try {
    for (var i = 0; i < 16; i++) {
      final handle = allocate(8);
      if (handle == 0) throw StateError('Unexpected arena limit');
      handles.add(handle);
    }
    if (allocate(8) != 0 || allocate(65537) != 0 || allocate(0) != 0)
      throw StateError('Arena limit bypass');
  } finally {
    for (final handle in handles) {
      release(handle);
    }
  }
  if (pointer(handles.first) != nullptr || release(handles.first) != 0)
    throw StateError('Stale buffer accepted');
  final next = allocate(8);
  if (next == 0 || handles.contains(next))
    throw StateError('Buffer identity reused');
  release(next);
  for (final revision in [
    BigInt.one,
    (BigInt.one << 53) + BigInt.one,
    (BigInt.one << 64) - BigInt.one,
  ]) {
    final input = RenameCommand(
      operationId: 'native-op',
      cardId: 'legacy-123',
      expectedRevision: revision,
      title: '标题 🪷',
    );
    final output = RenameCommand.decode(bridge.roundTrip(input.encode()));
    if (output.expectedRevision != revision ||
        output.title != input.title ||
        output.cardId != input.cardId)
      throw StateError('Native round trip mismatch');
  }
  for (var i = 0; i < 32; i++) {
    var rejected = false;
    try {
      bridge.roundTrip(Uint8List.fromList([0, 1, 2]));
    } on FormatException {
      rejected = true;
    }
    if (!rejected || bridge.liveBuffers != 0)
      throw StateError('Invalid-input leak');
  }
  if (bridge.liveBuffers != 0) throw StateError('Native buffer leak');
  print(
    'PASS: Dart VM -> native Rust FFI -> Dart, full UInt64 and invalid-input cleanup.',
  );
}
