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
  for (final action in [
    host.Action.mutationStart,
    host.Action.mutationSubmit,
    host.Action.guestMutationStart,
    host.Action.guestMutationSubmit,
    host.Action.mutationReconcile,
    host.Action.mutationDiscover,
  ]) {
    for (final failWrite in [false, true]) {
      test(
        'mutation request copies clear after $action, failed=$failWrite',
        () async {
          final source = Uint8List(8192)..fillRange(0, 8192, 91);
          late List<Uint8List> segments;
          late Uint8List frame;
          final operation = sendHostRequest(
            action,
            configure: (r) {
              if (action == host.Action.mutationStart) {
                r.initMutationStart().selectedPath = 's' * 8192;
              } else if (action == host.Action.mutationSubmit) {
                r.initMutationCommand().bytes = source;
              } else if (action == host.Action.guestMutationStart) {
                r.initGuestMutationStart().initSelection().selectedPath =
                    's' * 8192;
              } else if (action == host.Action.guestMutationSubmit) {
                r.initGuestMutationCommand().bytes = source;
              } else if (action == host.Action.mutationDiscover) {
                r.initMutationDiscover().subject = 's' * 8192;
              } else {
                r.initMutationReconcile().plan = source;
              }
              segments = buffers(r);
            },
            send: (bytes) async {
              frame = bytes;
              expectCleared(segments);
              expect(bytes.any((value) => value != 0), isTrue);
              expect(
                MessageReader.deserialize(
                  bytes,
                ).getRoot(host.requestFactory).action,
                action,
              );
              if (failWrite) throw StateError('unknown write');
            },
          );
          if (failWrite) {
            await expectLater(operation, throwsStateError);
          } else {
            await operation;
          }
          expectCleared([...segments, frame]);
          expect(source.every((value) => value == 91), isTrue);
        },
      );
    }
  }

  test(
    'service routing bounds the complete frame and clears ordinary request copies',
    () async {
      late List<Uint8List> segments;
      await expectLater(
        sendHostRequest(
          host.Action.service,
          maxBytes: 64 * 1024,
          clearAfterSend: true,
          configure: (request) {
            request.payload = Uint8List(64 * 1024)..fillRange(0, 64 * 1024, 71);
            segments = buffers(request);
          },
          send: (_) async => fail('complete frame exceeds the nested budget'),
        ),
        throwsFormatException,
      );
      expectCleared(segments);
      await sendHostRequest(
        host.Action.service,
        clearAfterSend: true,
        configure: (r) => r.payload = Uint8List(64 * 1024),
        send: (frame) async {
          expect(frame.length, greaterThan(64 * 1024));
          expect(frame.length, lessThan(128 * 1024));
        },
      );
    },
  );
  for (final failWrite in [false, true]) {
    test(
      'nested command clears replaced segments and serialized frame, failure=$failWrite',
      () async {
        final inner = MessageBuilder();
        final credential = inner.initRoot(host.requestFactory)
          ..action = host.Action.credentialSave
          ..credentialSecret = 'nested-only-secret';
        final nested = inner.serialize();
        late List<Uint8List> segments;
        late Uint8List frame;
        final operation = sendHostRequest(
          host.Action.commandSubmit,
          configure: (request) {
            request.payload = Uint8List(8192)..fillRange(0, 8192, 83);
            request.payload = nested;
            segments = buffers(request);
            expect(segments.length, greaterThan(1));
          },
          send: (bytes) async {
            frame = bytes;
            expectCleared(segments);
            final outer = MessageReader.deserialize(
              bytes,
            ).getRoot(host.requestFactory);
            final request = MessageReader.deserialize(
              outer.payload!,
            ).getRoot(host.requestFactory);
            expect(request.credentialSecret, 'nested-only-secret');
            await Future<void>.delayed(Duration.zero);
            expect(bytes.any((byte) => byte != 0), isTrue);
            if (failWrite) throw StateError('uncertain write');
          },
        );
        if (failWrite) {
          await expectLater(operation, throwsStateError);
        } else {
          await operation;
        }
        expectCleared([...segments, frame]);
        // The caller continues owning its original input, independently of the
        // temporary wrapper copies that sendHostRequest erases.
        expect(
          MessageReader.deserialize(
            nested,
          ).getRoot(host.requestFactory).credentialSecret,
          'nested-only-secret',
        );
        nested.fillRange(0, nested.length, 0);
        for (final bytes in buffers(credential)) {
          bytes.fillRange(0, bytes.length, 0);
        }
      },
    );
  }
  test(
    'invalid nested command clears abandoned allocations before any send',
    () async {
      late List<Uint8List> segments;
      await expectLater(
        sendHostRequest(
          host.Action.commandSubmit,
          configure: (request) {
            request.payload = Uint8List(8192)..fillRange(0, 8192, 83);
            request.payload = Uint8List(16384)..fillRange(0, 16384, 84);
            segments = buffers(request);
            throw const FormatException('invalid nested frame');
          },
          send: (_) async => fail('must not send'),
        ),
        throwsFormatException,
      );
      expectCleared(segments);
    },
  );
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
