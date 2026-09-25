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
  List<ServiceEndpointSelection> outbound = const [],
  ServiceTlsSelection? tls,
  ServiceTlsIdentityChoice? protectedTls,
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
  outbound: outbound,
  tls: tls,
  protectedTls: protectedTls,
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
    'saved TLS start freezes identity and refuses ambiguous selection',
    () async {
      final ref = identity(23), digest = identity(24);
      final selected = ServiceTlsIdentityChoice(
        reference: ref,
        revision: BigInt.from(3),
        certificateSha256: digest,
      );
      final value = request(protectedTls: selected);
      ref.fillRange(0, 32, 0);
      digest.fillRange(0, 32, 0);
      await sendHostRequest(
        host.Action.serviceRunStart,
        configure: (r) =>
            ServiceRunCodec.writeRequest(value, r.initServiceRun()),
        send: (bytes) async {
          final row = MessageReader.deserialize(
            bytes,
          ).getRoot(host.requestFactory).serviceRun!;
          expect(row.tls, isNull);
          expect(row.protectedTls!.reference, identity(23));
          expect(row.protectedTls!.revision, 3);
          expect(row.protectedTls!.certificateSha256, identity(24));
        },
      );
      expect(
        () => ServiceRunValidation.request(
          request(
            protectedTls: selected,
            tls: ServiceTlsSelection(
              certificatePath: '/cert',
              privateKeyPath: '/key',
              certificateSha256: identity(2),
            ),
          ),
        ),
        throwsFormatException,
      );
      expect(
        () => value.protectedTls!.reference[0] = 4,
        throwsUnsupportedError,
      );
    },
  );
  test(
    'TLS request freezes selection and writes only paths and certificate digest',
    () async {
      final digest = identity(19);
      final selection = ServiceTlsSelection(
        certificatePath: '/cert.pem',
        privateKeyPath: '/key.pem',
        certificateSha256: digest,
      );
      final value = request(tls: selection);
      digest.fillRange(0, digest.length, 0);
      await sendHostRequest(
        host.Action.serviceRunStart,
        configure: (r) =>
            ServiceRunCodec.writeRequest(value, r.initServiceRun()),
        send: (bytes) async {
          final row = MessageReader.deserialize(
            bytes,
          ).getRoot(host.requestFactory).serviceRun!.tls!;
          expect(row.certificatePath, '/cert.pem');
          expect(row.privateKeyPath, '/key.pem');
          expect(row.certificateSha256, orderedEquals(identity(19)));
        },
      );
      expect(() => value.tls!.certificateSha256[0] = 1, throwsUnsupportedError);
      final out = response();
      final row = out.initServiceTls();
      row.certificatePath = '/cert.pem';
      row.privateKeyPath = '/key.pem';
      row.certificateSha256 = identity(19);
      row.initValidity()
        ..notBeforeSeconds = 1
        ..notAfterSeconds = 253402300799;
      expect(
        ServiceRunCodec.tlsSelection(
          out.asReader(),
          certificatePath: '/cert.pem',
          privateKeyPath: '/key.pem',
        ).certificateSha256,
        orderedEquals(identity(19)),
      );
      row.certificatePath = '/other.pem';
      expect(
        () => ServiceRunCodec.tlsSelection(
          out.asReader(),
          certificatePath: '/cert.pem',
          privateKeyPath: '/key.pem',
        ),
        throwsFormatException,
      );
    },
  );
  test('TLS inspection rejects missing or malformed validity metadata', () {
    final out = response();
    final row = out.initServiceTls();
    row.certificatePath = '/cert.pem';
    row.privateKeyPath = '/key.pem';
    row.certificateSha256 = identity(19);
    ServiceTlsSelection decode() => ServiceRunCodec.tlsSelection(
      out.asReader(),
      certificatePath: '/cert.pem',
      privateKeyPath: '/key.pem',
    );
    expect(decode, throwsFormatException);
    for (final bounds in [(2, 1), (-62135596801, 1), (0, 253402300800)]) {
      row.initValidity()
        ..notBeforeSeconds = bounds.$1
        ..notAfterSeconds = bounds.$2;
      expect(decode, throwsFormatException);
    }
  });
  test(
    'endpoint selection is owned and preserves UInt64 revisions on wire',
    () async {
      final source = identity(7), max = (BigInt.one << 64) - BigInt.one;
      final selections = [
        ServiceEndpointSelection(reference: source, revision: max),
      ];
      final value = request(outbound: selections);
      source.fillRange(0, source.length, 0);
      selections.clear();
      expect(value.outbound.single.reference, orderedEquals(identity(7)));
      expect(() => value.outbound.clear(), throwsUnsupportedError);
      expect(
        () => value.outbound.single.reference[0] = 0,
        throwsUnsupportedError,
      );
      await sendHostRequest(
        host.Action.serviceRunStart,
        configure: (r) =>
            ServiceRunCodec.writeRequest(value, r.initServiceRun()),
        send: (bytes) async {
          final selections = MessageReader.deserialize(
            bytes,
          ).getRoot(host.requestFactory).serviceRun!.outbound!;
          expect(selections.length, 1);
          expect(selections[0].reference, orderedEquals(identity(7)));
          expect(selections[0].revisionBigInt, max);
        },
      );
    },
  );
  test(
    'invalid selections reject before sending and old callers select none',
    () async {
      ServiceEndpointSelection endpoint(int id, BigInt revision) =>
          ServiceEndpointSelection(reference: identity(id), revision: revision);
      for (final selections in [
        List.generate(9, (i) => endpoint(i + 1, BigInt.one)),
        [endpoint(1, BigInt.one), endpoint(1, BigInt.two)],
        [endpoint(0, BigInt.one)],
        [endpoint(1, BigInt.zero)],
        [endpoint(1, BigInt.one << 64)],
        [
          ServiceEndpointSelection(
            reference: Uint8List(31),
            revision: BigInt.one,
          ),
        ],
      ]) {
        var sends = 0;
        await expectLater(
          sendHostRequest(
            host.Action.serviceRunStart,
            configure: (r) => ServiceRunCodec.writeRequest(
              request(outbound: selections),
              r.initServiceRun(),
            ),
            send: (_) async {
              sends++;
            },
          ),
          throwsFormatException,
        );
        expect(sends, 0);
      }
      expect(request().outbound, isEmpty);
    },
  );
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
          expect(r.configRevisionBigInt, max);
          expect(r.publicationRevisionBigInt, max);
          expect(r.registryRevisionBigInt, max);
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
