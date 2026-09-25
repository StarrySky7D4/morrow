import 'dart:typed_data';

import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;
import 'package:morrow_studio/plugins/host_request.dart';
import 'package:morrow_studio/plugins/io_task_control.dart';
import 'package:morrow_studio/plugins/io_task_codec_native.dart';

Uint8List identity(int value) => Uint8List.fromList(List.filled(32, value));
HttpTaskRequest request({
  BigInt? revision,
  BigInt? registryRevision,
  Uint8List? submission,
  Uint8List? endpoint,
  Uint8List? digest,
  String method = 'POST',
  String target = '/v1/test?a=1',
  int timeoutMs = 10000,
  List<HttpTaskHeader>? headers,
  Uint8List? body,
}) => HttpTaskRequest(
  submission: submission ?? identity(1),
  endpoint: endpoint ?? identity(2),
  endpointRevision: revision ?? BigInt.one,
  packageDigest: digest ?? identity(3),
  registryRevision: registryRevision ?? BigInt.from(7),
  method: method,
  target: target,
  headers:
      headers ??
      [
        HttpTaskHeader(
          name: 'x-binary',
          value: Uint8List.fromList([9, 0x80, 0xff]),
        ),
      ],
  body: body ?? Uint8List.fromList([0, 1, 255]),
  timeoutMs: timeoutMs,
);

host.IoStateBuilder state() => MessageBuilder().initRoot(host.ioStateFactory);
host.IoResultBuilder result() =>
    MessageBuilder().initRoot(host.ioResultFactory);

void main() {
  test(
    'HTTP frame preserves all fields, binary values and full UInt64 revisions',
    () async {
      final max = (BigInt.one << 64) - BigInt.one;
      final exact = (BigInt.one << 53) + BigInt.one;
      final value = request(revision: max, registryRevision: exact);
      var writes = 0;
      await sendHostRequest(
        host.Action.httpStart,
        configure: (r) => HttpTaskCodec.writeRequest(value, r.initHttpStart()),
        send: (bytes) async {
          writes++;
          final root = MessageReader.deserialize(
            bytes,
          ).getRoot(host.requestFactory);
          expect(root.action, host.Action.httpStart);
          final r = root.httpStart!;
          expect(r.submission, orderedEquals(identity(1)));
          expect(r.endpoint, orderedEquals(identity(2)));
          expect(r.packageDigest, orderedEquals(identity(3)));
          expect(r.endpointRevisionBigInt, max);
          expect(r.registryRevisionBigInt, exact);
          expect(r.method, 'POST');
          expect(r.target, '/v1/test?a=1');
          expect(r.timeoutMs, 10000);
          expect(r.headers!.single.name, 'x-binary');
          expect(r.headers!.single.value, orderedEquals([9, 0x80, 0xff]));
          expect(r.body, orderedEquals([0, 1, 255]));
        },
      );
      expect(writes, 1);
    },
  );

  test(
    'request fields own immutable bytes before entering an asynchronous queue',
    () {
      final bytes = identity(1),
          body = Uint8List.fromList([1, 2]),
          value = Uint8List.fromList([0xff]);
      final h = HttpTaskHeader(name: 'x-test', value: value);
      final r = request(submission: bytes, body: body, headers: [h]);
      bytes[0] = 99;
      body[0] = 99;
      value[0] = 0;
      expect(r.submission.first, 1);
      expect(r.body.first, 1);
      expect(r.headers.single.value.first, 255);
      expect(() => r.body[0] = 5, throwsUnsupportedError);
      expect(() => r.headers.add(h), throwsUnsupportedError);
    },
  );

  test(
    'out-of-range integers and identities are rejected before any transport send',
    () async {
      for (final r in [
        request(revision: BigInt.zero),
        request(revision: BigInt.from(-1)),
        request(revision: BigInt.one << 64),
        request(registryRevision: BigInt.one << 64),
        request(registryRevision: BigInt.from(-1)),
        request(timeoutMs: 0),
        request(timeoutMs: 30001),
        request(timeoutMs: 0x100000001),
        request(submission: Uint8List(32)),
        request(endpoint: Uint8List(31)),
        request(digest: Uint8List(33)),
      ]) {
        var sent = false;
        await expectLater(
          sendHostRequest(
            host.Action.httpStart,
            configure: (out) =>
                HttpTaskCodec.writeRequest(r, out.initHttpStart()),
            send: (_) async {
              sent = true;
            },
          ),
          throwsFormatException,
        );
        expect(sent, isFalse);
      }
    },
  );

  test(
    'relative target, method, body and runtime-owned request headers are bounded',
    () {
      final invalid = [
        request(method: 'CONNECT'),
        request(method: 'get'),
        for (final target in [
          'https://example.com',
          '//other/a',
          '/a#b',
          '/a b',
          '/a\\b',
          '/${'a' * 2048}',
        ])
          request(target: target),
        request(body: Uint8List(65537)),
        request(
          headers: List.generate(
            65,
            (_) => HttpTaskHeader(name: 'x', value: Uint8List(0)),
          ),
        ),
        for (final name in [
          'Authorization',
          'cookie',
          'host',
          'Proxy-Foo',
          'content-length',
          'x bad',
        ])
          request(
            headers: [
              HttpTaskHeader(name: name, value: Uint8List.fromList([65])),
            ],
          ),
        request(
          headers: [
            HttpTaskHeader(name: 'x', value: Uint8List.fromList([10])),
          ],
        ),
        request(
          headers: [
            HttpTaskHeader(name: 'x', value: Uint8List.fromList([127])),
          ],
        ),
        request(
          headers: [
            HttpTaskHeader(
              name: 'x',
              value: Uint8List.fromList(List.filled(8193, 65)),
            ),
          ],
        ),
        request(
          headers: List.generate(
            2,
            (_) => HttpTaskHeader(
              name: 'x',
              value: Uint8List.fromList(List.filled(8192, 65)),
            ),
          ),
        ),
      ];
      for (final value in invalid) {
        expect(
          () => HttpTaskCodec.validateRequest(value),
          throwsFormatException,
        );
      }
      expect(
        () => HttpTaskCodec.validateRequest(
          request(body: Uint8List(65536), timeoutMs: 30000),
        ),
        returnsNormally,
      );
    },
  );

  test('ready delivery never implies actual exit or reclaimed storage', () {
    final r = state()
      ..key = identity(4)
      ..submission = identity(1)
      ..storage = 1
      ..delivery = 2;
    final value = HttpTaskCodec.snapshot(r.asReader());
    expect(value.storage, IoStoragePhase.running);
    expect(value.delivery, IoDeliveryPhase.ready);
    expect(value.exit, isNull);
    expect(value.key, orderedEquals(identity(4)));
    r.storage = 2;
    expect(
      HttpTaskCodec.snapshot(r.asReader()).storage,
      IoStoragePhase.stopping,
    );
    r.storage = 4;
    r.hasExit = true;
    r.disconnect = 9;
    r.maintenance = 7;
    final recovered = HttpTaskCodec.snapshot(r.asReader());
    expect(recovered.storage, IoStoragePhase.recoveryRequired);
    expect(recovered.exit!.execution, IoJobError.none);
    expect(recovered.exit!.disconnect, IoJobError.disconnect);
    expect(recovered.exit!.maintenance, IoJobError.limit);
  });

  test(
    'unknown state numbers, zero identities and exit contradictions are rejected',
    () {
      expect(() => HttpTaskCodec.snapshot(null), throwsFormatException);
      for (final configure in <void Function(host.IoStateBuilder)>[
        (r) => r.storage = 99,
        (r) => r.delivery = 99,
        (r) => r.execution = 99,
        (r) => r.key = Uint8List(32),
        (r) => r.submission = Uint8List(31),
        (r) => r.storage = 1,
        (r) => r.delivery = 2,
        (r) => r.execution = 1,
        (r) => r.hasExit = true,
      ]) {
        final r = state();
        configure(r);
        expect(
          () => HttpTaskCodec.snapshot(r.asReader()),
          throwsFormatException,
        );
      }
      // Acknowledgement clears the task key; the last submission may remain observable.
      final r = state()..submission = identity(1);
      expect(HttpTaskCodec.snapshot(r.asReader()).key, isNull);
    },
  );

  test(
    'read result preserves counters, binary response and 5xx completed status',
    () {
      final r = result()
        ..present = true
        ..calls = 0xffffffff
        ..chargedBytes = -1
        ..hasHttp = true
        ..status = 3
        ..httpStatus = 503
        ..body = Uint8List.fromList([0, 255]);
      final h = r.initHeaders(1)[0]
        ..name = 'set-cookie'
        ..value = Uint8List.fromList([9, 128, 255]);
      final value = HttpTaskCodec.result(r.asReader())!;
      expect(value.calls, BigInt.from(0xffffffff));
      expect(value.chargedBytes, (BigInt.one << 64) - BigInt.one);
      expect(value.http!.status, IoHttpStatus.completed);
      expect(value.http!.httpStatus, 503);
      expect(value.http!.headers.single.value, orderedEquals([9, 128, 255]));
      expect(value.http!.body, orderedEquals([0, 255]));
      h.value = Uint8List.fromList([65]);
      expect(value.http!.headers.single.value, orderedEquals([9, 128, 255]));
    },
  );

  test(
    'Unknown, cancellation and executor failures remain separate observations',
    () {
      final r = result()
        ..present = true
        ..unknown = true
        ..cancelled = true
        ..executionFault = 8
        ..hasHttp = true
        ..status = 12;
      final value = HttpTaskCodec.result(r.asReader())!;
      expect(value.unknown, isTrue);
      expect(value.cancelled, isTrue);
      expect(value.executionFault, IoExecutionFault.cancelled);
      expect(value.http!.status, IoHttpStatus.outcomeUnknown);
      expect(value.http!.httpStatus, 0);
    },
  );

  test(
    'absent results are optional while inconsistent or oversized HTTP data is rejected',
    () {
      expect(HttpTaskCodec.result(null), isNull);
      expect(HttpTaskCodec.result(result().asReader()), isNull);
      for (final configure in <void Function(host.IoResultBuilder)>[
        (r) => r.calls = 1,
        (r) {
          r.present = true;
          r.executionFault = 99;
        },
        (r) {
          r.present = true;
          r.executionFault = 1;
          r.exitCode = 1;
        },
        (r) {
          r.present = true;
          r.body = Uint8List.fromList([1]);
        },
        (r) {
          r.present = true;
          r.hasHttp = true;
          r.status = 99;
        },
        (r) {
          r.present = true;
          r.hasHttp = true;
          r.status = 0;
        },
        (r) {
          r.present = true;
          r.hasHttp = true;
          r.status = 3;
          r.httpStatus = 600;
        },
        (r) {
          r.present = true;
          r.hasHttp = true;
          r.status = 12;
          r.httpStatus = 200;
        },
        (r) {
          r.present = true;
          r.hasHttp = true;
          r.status = 3;
          r.httpStatus = 200;
          r.body = Uint8List(65537);
        },
        (r) {
          r.present = true;
          r.hasHttp = true;
          r.status = 3;
          r.httpStatus = 200;
          r.initHeaders(65);
        },
      ]) {
        final r = result();
        configure(r);
        expect(() => HttpTaskCodec.result(r.asReader()), throwsFormatException);
      }
    },
  );
}
