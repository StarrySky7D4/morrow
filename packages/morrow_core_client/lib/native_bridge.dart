import 'dart:ffi';
import 'dart:typed_data';
import 'src/codec.dart';

/// Same-process trusted client only. All native buffers have one synchronous owner.
class NativeCoreBridge {
  NativeCoreBridge(String path) : this.library(DynamicLibrary.open(path));
  NativeCoreBridge.library(DynamicLibrary library) {
    _new = library.lookupFunction<Uint32 Function(Uint32), int Function(int)>(
      'morrow_buffer_new',
    );
    _ptr = library
        .lookupFunction<
          Pointer<Uint8> Function(Uint32),
          Pointer<Uint8> Function(int)
        >('morrow_buffer_ptr');
    _len = library.lookupFunction<Uint32 Function(Uint32), int Function(int)>(
      'morrow_buffer_len',
    );
    _process = library
        .lookupFunction<Uint32 Function(Uint32), int Function(int)>(
          'morrow_buffer_process',
        );
    _status = library
        .lookupFunction<Uint32 Function(Uint32), int Function(int)>(
          'morrow_buffer_status',
        );
    _free = library.lookupFunction<Uint32 Function(Uint32), int Function(int)>(
      'morrow_buffer_free',
    );
    _live = library.lookupFunction<Uint32 Function(), int Function()>(
      'morrow_buffer_live',
    );
  }
  late final int Function(int) _new, _len, _process, _status, _free;
  late final Pointer<Uint8> Function(int) _ptr;
  late final int Function() _live;
  int get liveBuffers => _live();
  Uint8List roundTrip(Uint8List bytes) {
    if (bytes.isEmpty || bytes.length > maxMessageBytes)
      throw const FormatException('Message length');
    final input = _new(bytes.length);
    if (input == 0) throw StateError('Core buffer limit');
    var output = 0;
    try {
      final inputPointer = _ptr(input);
      if (inputPointer == nullptr) throw StateError('Invalid input buffer');
      inputPointer.asTypedList(bytes.length).setAll(0, bytes);
      output = _process(input);
      if (output == 0)
        throw FormatException('Core rejected message: ${_status(input)}');
      final len = _len(output), pointer = _ptr(output);
      if (len == 0 || len > maxMessageBytes || pointer == nullptr)
        throw StateError('Invalid output buffer');
      return Uint8List.fromList(pointer.asTypedList(len));
    } finally {
      if (output != 0) _free(output);
      _free(input);
    }
  }
}
