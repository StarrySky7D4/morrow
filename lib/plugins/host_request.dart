import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'generated/host.capnp.dart' as host;
import 'generated/identity.dart' as contract;

/// The sender must finish consuming the frame before its future completes.
/// Clears the credential arena and serialized frame. Immutable Dart strings,
/// dependency-owned UTF-8 temporaries and OS copies are outside this guarantee.
Future<void> sendHostRequest(
  host.Action action, {
  void Function(host.RequestBuilder)? configure,
  required Future<void> Function(Uint8List) send,
}) async {
  final builder = MessageBuilder();
  final request = builder.initRoot(host.requestFactory);
  // A command payload is a complete nested request and may contain credentials.
  final sensitive =
      action == host.Action.credentialSave ||
      action == host.Action.commandSubmit;
  Uint8List? payload;
  try {
    try {
      request.version = 1;
      request.digest = Uint8List.fromList(contract.hostDigest);
      request.action = action;
      configure?.call(request);
      payload = builder.serialize();
    } finally {
      if (sensitive) {
        final arena = request.raw.arena;
        // reset() discards extra segments without erasing them. Wipe every
        // owned segment, including allocations abandoned by field replacement.
        for (var i = 0; i < arena.segmentCount; i++) {
          final data = arena.getSegment(i).data;
          final bytes = data.buffer.asUint8List(
            data.offsetInBytes,
            data.lengthInBytes,
          );
          bytes.fillRange(0, bytes.length, 0);
        }
      }
    }
    if (payload.length > 128 * 1024) {
      throw const FormatException('请求内容过大');
    }
    await send(payload);
  } finally {
    if (sensitive && payload != null) {
      payload.fillRange(0, payload.length, 0);
    }
  }
}
