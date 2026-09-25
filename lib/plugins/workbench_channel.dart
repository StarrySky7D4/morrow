import 'dart:typed_data';

/// Ordered, single-owner transport for the private host protocol.
/// Output uses the host's existing little-endian length framing. Closing input
/// requests a durable shutdown; [exitCode] completes only after ownership ends.
abstract interface class WorkbenchChannel {
  Stream<List<int>> get output;
  Stream<List<int>> get diagnostics;
  Future<int> get exitCode;
  Future<void> send(Uint8List frame);
  Future<void> closeInput();
}
