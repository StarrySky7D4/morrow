import 'dart:io';
import 'dart:typed_data';

Uint8List readRecordedHistoryPacket() {
  const fixture = 'test/fixtures/history-unknown-acknowledged.capnp';
  // Flutter tests run from the app root and dart test runs from the package.
  final fromApp = File('packages/morrow_core_client/$fixture');
  return (fromApp.existsSync() ? fromApp : File(fixture)).readAsBytesSync();
}
