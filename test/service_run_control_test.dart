import 'dart:typed_data';

import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/generated/host.capnp.dart' as host;
import 'package:morrow_studio/plugins/host_request.dart';
import 'package:morrow_studio/plugins/io_task_models.dart';
import 'package:morrow_studio/plugins/service_run_codec_native.dart';
import 'package:morrow_studio/plugins/service_run_control.dart';

Uint8List identity(int value) => Uint8List.fromList(List.filled(32, value));
ServiceRunRequest request({
  Uint8List? submission,
  BigInt? revision,
  int lifetime = 30000,
  int timeout = 10000,
  int calls = 16,
  int concurrent = 1,
  int requestBytes = 65536,
  BigInt? jobs,
  BigInt? bytes,
}) => ServiceRunRequest(
  submission: submission ?? identity(1),
  configId: 'service',
  configDigest: identity(2),
  configRevision: revision ?? BigInt.one,
  publication: identity(3),
  publicationRevision: revision ?? BigInt.one,
  packageId: 'package',
  packageDigest: identity(4),
  registryRevision: revision ?? BigInt.one,
  lifetimeMs: lifetime,
  timeoutMs: timeout,
  maxJobs: jobs ?? BigInt.from(100),
  maxBytes: bytes ?? BigInt.from(65536),
  maxJobBytes: BigInt.from(65536),
  maxTotalBytes: BigInt.from(65536),
  maxCalls: calls,
  maxConcurrent: concurrent,
  maxRequestBytes: requestBytes,
);

host.ResponseBuilder response() =>
    MessageBuilder().initRoot(host.responseFactory);

host.ResponseBuilder command({
  int delivery = 2,
  bool started = true,
  int terminal = 0,
}) {
  final out = response();
  final row = out.initOwnerCommand();
  row.key = identity(5);
  row.submission = identity(6);
  row.delivery = delivery;
  row.started = started;
  row.terminal = terminal;
  return out;
}

void main() {
  test(
    'service request owns identities and preserves complete UInt64 revisions',
    () async {
      final source = identity(1);
      final max = (BigInt.one << 64) - BigInt.one;
      final value = request(submission: source, revision: max);
      source.fillRange(0, source.length, 0);
      expect(value.submission, orderedEquals(identity(1)));
      expect(() => value.submission[0] = 0, throwsUnsupportedError);
      await sendHostRequest(
        host.Action.serviceRunStart,
        configure: (r) =>
            ServiceRunCodec.writeRequest(value, r.initServiceRun()),
        send: (bytes) async {
          final r = MessageReader.deserialize(
            bytes,
          ).getRoot(host.requestFactory).serviceRun!;
          expect(BigInt.from(r.configRevision).toUnsigned(64), max);
          expect(BigInt.from(r.publicationRevision).toUnsigned(64), max);
          expect(BigInt.from(r.registryRevision).toUnsigned(64), max);
          expect(r.submission, orderedEquals(identity(1)));
          expect(r.configDigest, orderedEquals(identity(2)));
          expect(r.publication, orderedEquals(identity(3)));
          expect(r.packageDigest, orderedEquals(identity(4)));
          expect(r.lifetimeMs, 30000);
          expect(r.maxJobs, 100);
          expect(r.maxConcurrent, 1);
        },
      );
    },
  );

  test('bounds reject before transport send or integer narrowing', () async {
    for (final value in [
      request(revision: BigInt.zero),
      request(revision: BigInt.from(-1)),
      request(revision: BigInt.one << 64),
      request(submission: Uint8List(32)),
      request(submission: identity(1).sublist(1)),
      request(submission: Uint8List(33)),
      request(lifetime: 0),
      request(lifetime: 3600001),
      request(timeout: 30001),
      request(lifetime: 1),
      request(calls: 1025),
      request(calls: 0x100000001),
      request(concurrent: 65537),
      request(requestBytes: 64 * 1024 * 1024 + 1),
      request(jobs: BigInt.from(1000001)),
      request(bytes: BigInt.one << 64),
    ]) {
      var sent = false;
      await expectLater(
        sendHostRequest(
          host.Action.serviceRunStart,
          configure: (r) =>
              ServiceRunCodec.writeRequest(value, r.initServiceRun()),
          send: (_) async {
            sent = true;
          },
        ),
        throwsFormatException,
      );
      expect(sent, isFalse);
    }
    expect(
      () => ServiceRunValidation.command(Uint8List(65537)),
      throwsFormatException,
    );
    expect(
      () => ServiceRunValidation.command(Uint8List(7)),
      throwsFormatException,
    );
    ServiceRunValidation.command(Uint8List(65536));
  });

  test(
    'service running observation permits requested stop before supervisor tick',
    () {
      final out = response();
      final row = out.initServiceRun();
      row.submission = identity(1);
      row.phase = 1;
      row.bind = 1;
      row.address = '127.0.0.1:1234';
      final task = row.initTask();
      task.key = identity(2);
      task.submission = identity(1);
      task.storage = IoStoragePhase.stopping.index;
      task.delivery = IoDeliveryPhase.absent.index;
      final value = ServiceRunCodec.snapshot(
        out.asReader(),
        expectedKey: identity(2),
      );
      expect(value.phase, ServiceRunPhase.running);
      expect(value.task.storage, IoStoragePhase.stopping);
      row.submission = identity(9);
      expect(value.submission, orderedEquals(identity(1)));
      expect(
        () => ServiceRunCodec.snapshot(out.asReader()),
        throwsFormatException,
      );
      row.submission = identity(1);
      row.phase = 99;
      expect(
        () => ServiceRunCodec.snapshot(out.asReader()),
        throwsFormatException,
      );
    },
  );

  test(
    'command delivery validates terminal combinations and identity correlation',
    () {
      for (final out in [
        command(delivery: 99),
        command(terminal: 99),
        command(delivery: 1, terminal: 2),
        command(started: false, terminal: 5),
        command(started: true, terminal: 4),
        command(started: false),
      ]) {
        expect(
          () => ServiceRunCodec.command(out.asReader()),
          throwsFormatException,
        );
      }
      final out = command();
      expect(
        () => ServiceRunCodec.command(out.asReader(), expectedKey: identity(1)),
        throwsFormatException,
      );
      final value = ServiceRunCodec.command(
        out.asReader(),
        expectedSubmission: identity(6),
      );
      expect(value.delivery, OwnerCommandDelivery.consumed);
      expect(value.key, orderedEquals(identity(5)));
    },
  );

  test(
    'nested reply has independent mutable lifetime and dispose wipes retained alias',
    () {
      final out = command();
      out.payload = Uint8List.fromList([1, 0, 255, 4]);
      final value = ServiceRunCodec.read(
        out.asReader(),
        expectedKey: identity(5),
      );
      final alias = value.payload!;
      out.payload = Uint8List(4);
      expect(alias, orderedEquals([1, 0, 255, 4]));
      alias[0] = 8;
      value.dispose();
      value.dispose();
      expect(alias, everyElement(0));
      expect(value.disposed, isTrue);
      expect(value.payload, isNull);
      expect(value.snapshot.key, orderedEquals(identity(5)));
    },
  );

  test(
    'nested payload only belongs to successful consumed command and is bounded',
    () {
      for (final out in [
        command(delivery: 0),
        command(terminal: 5),
        command(started: false, terminal: 4),
      ]) {
        out.payload = Uint8List(8);
        expect(
          () => ServiceRunCodec.read(out.asReader(), expectedKey: identity(5)),
          throwsFormatException,
        );
      }
      final out = command();
      out.payload = Uint8List(128 * 1024 + 1);
      expect(
        () => ServiceRunCodec.read(out.asReader(), expectedKey: identity(5)),
        throwsFormatException,
      );
      expect(
        ServiceRunCodec.responseMaxBytes(host.Action.commandRead),
        256 * 1024,
      );
      for (final action in [
        host.Action.commandSubmit,
        host.Action.commandStatus,
        host.Action.page,
      ]) {
        expect(ServiceRunCodec.responseMaxBytes(action), 128 * 1024);
      }
    },
  );
}
