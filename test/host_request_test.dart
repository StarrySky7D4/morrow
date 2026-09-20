import 'dart:async';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;
import 'package:morrow_studio/plugins/host_request.dart';

List<Uint8List> buffers(host.RequestBuilder request) => [
  for (var i = 0; i < request.raw.arena.segmentCount; i++)
    Uint8List.view(
      request.raw.arena.getSegment(i).data.buffer,
      request.raw.arena.getSegment(i).data.offsetInBytes,
      request.raw.arena.getSegment(i).data.lengthInBytes,
    ),
];

void expectCleared(List<Uint8List> buffers) {
  expect(buffers, isNotEmpty);
  for (final bytes in buffers) {
    expect(bytes.every((byte) => byte == 0), isTrue);
  }
}

void main() {
  test(
    'credential frame survives pending write, all owned copies clear afterward',
    () async {
      final started = Completer<void>();
      final flush = Completer<void>();
      late List<Uint8List> segments;
      late Uint8List frame;
      final operation = sendHostRequest(
        host.Action.credentialSave,
        configure: (request) {
          request.credentialSecret = 'x' * 8192;
          request.credentialSecret = 'replacement-secret';
          segments = buffers(request);
          expect(segments.length, greaterThan(1));
        },
        send: (bytes) async {
          frame = bytes;
          expectCleared(segments);
          expect(
            MessageReader.deserialize(
              bytes,
            ).getRoot(host.requestFactory).credentialSecret,
            'replacement-secret',
          );
          started.complete();
          await flush.future;
          expect(bytes.any((byte) => byte != 0), isTrue);
        },
      );
      await started.future;
      expect(frame.any((byte) => byte != 0), isTrue);
      flush.complete();
      await operation;
      expectCleared([...segments, frame]);
    },
  );

  test(
    'failed transport clears builder and serialized credential frame',
    () async {
      late List<Uint8List> segments;
      late Uint8List frame;
      await expectLater(
        sendHostRequest(
          host.Action.credentialSave,
          configure: (request) {
            request.credentialSecret = 'failed-write-secret';
            segments = buffers(request);
          },
          send: (bytes) async {
            frame = bytes;
            throw StateError('write failed');
          },
        ),
        throwsStateError,
      );
      expectCleared([...segments, frame]);
    },
  );

  test(
    'configure failure clears every allocated credential segment without sending',
    () async {
      late List<Uint8List> segments;
      await expectLater(
        sendHostRequest(
          host.Action.credentialSave,
          configure: (request) {
            request.credentialSecret = 'x' * 8192;
            segments = buffers(request);
            throw const FormatException('invalid request');
          },
          send: (_) async => fail('must not send'),
        ),
        throwsFormatException,
      );
      expectCleared(segments);
    },
  );

  test(
    'oversized credential message is rejected and builder cleared before transport',
    () async {
      late List<Uint8List> segments;
      await expectLater(
        sendHostRequest(
          host.Action.credentialSave,
          configure: (request) {
            request.credentialSecret = 'x' * (130 * 1024);
            segments = buffers(request);
          },
          send: (_) async => fail('must not send oversized request'),
        ),
        throwsFormatException,
      );
      expectCleared(segments);
    },
  );
}
