/// Music decryption backed by the same Rust core on native and Web platforms.
/// Native uses FFI; browsers use WASM through JS interop.
library;

export 'src/native.dart' if (dart.library.js_interop) 'src/web.dart';
