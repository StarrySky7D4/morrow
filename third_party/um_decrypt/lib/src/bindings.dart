import 'dart:ffi';

final class NativeInfo extends Struct {
  @Uint64()
  external int audioOffset;
  @Uint64()
  external int audioLength;
  @Uint32()
  external int format;
  @Uint32()
  external int audioFormat;
}

typedef _NewNative = Int32 Function(Pointer<Uint8>, UintPtr, Uint32,
    Pointer<Uint8>, UintPtr, Uint32, Pointer<Uint64>);
typedef _NewDart = int Function(
    Pointer<Uint8>, int, int, Pointer<Uint8>, int, int, Pointer<Uint64>);
typedef _HintNative = Int32 Function(
    Pointer<Uint8>, UintPtr, Uint32, Pointer<Uint8>, UintPtr, Pointer<UintPtr>);
typedef _HintDart = int Function(
    Pointer<Uint8>, int, int, Pointer<Uint8>, int, Pointer<UintPtr>);

/// Internal ABI declarations; no package:ffi dependency or platform path guesses.
final class Bindings {
  final DynamicLibrary library;
  Bindings(this.library) {
    final version = library.lookupFunction<Uint32 Function(), int Function()>(
      'um_abi_version',
    )();
    if (version != 1) {
      throw UnsupportedError(
          'um-decrypt ABI $version is incompatible with ABI 1');
    }
  }

  late final alloc = library.lookupFunction<Pointer<Uint8> Function(UintPtr),
      Pointer<Uint8> Function(int)>('um_buffer_alloc');
  late final free = library.lookupFunction<
      Void Function(Pointer<Uint8>, UintPtr),
      void Function(Pointer<Uint8>, int)>('um_buffer_free');
  late final create =
      library.lookupFunction<_NewNative, _NewDart>('um_decoder_new');
  late final info = library.lookupFunction<
      Int32 Function(Uint64, Pointer<NativeInfo>),
      int Function(int, Pointer<NativeInfo>)>('um_decoder_info');
  late final decrypt = library.lookupFunction<
      Int32 Function(Uint64, Uint64, Pointer<Uint8>, UintPtr),
      int Function(int, int, Pointer<Uint8>, int)>('um_decoder_decrypt');
  late final dispose =
      library.lookupFunction<Int32 Function(Uint64), int Function(int)>(
          'um_decoder_free');
  late final hint =
      library.lookupFunction<_HintNative, _HintDart>('um_key_hint');
}
