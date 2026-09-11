import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:typed_data';
import 'package:morrow_core_client/morrow_core_client.dart';
import 'package:morrow_core_client/native_bridge.dart';

void check(bool value, String message) {
  if (!value) throw StateError(message);
}

Future<void> main(List<String> args) async {
  if (args.length != 4)
    throw ArgumentError(
      'library, CLI, high-revision fixture, artifact directory required',
    );
  final folder = Directory(
    '${args[3]}/native-${DateTime.now().microsecondsSinceEpoch}',
  ).absolute..createSync(recursive: true);
  final db = '${folder.path}/含空格 数据.db';
  void cli(List<String> command) {
    final result = Process.runSync(args[1], command);
    check(result.exitCode == 0, 'CLI failed: ${result.stderr}');
  }

  cli(['init', db]);
  cli(['create-local', db, 'seed', 'card', 'original']);
  cli(['import-container-local', db, 'high-seed', args[2]]);
  final rawPath = '${folder.path}/large original.bin';
  final rawLength = 5 * 1024 * 1024 + 17;
  File(rawPath).writeAsBytesSync(List<int>.generate(rawLength, (i) => i % 251));
  final staged = Process.runSync(args[1], ['stage-file-local', db, rawPath]);
  check(staged.exitCode == 0, 'Stage fixture failed');
  cli([
    'create-attachment-local',
    db,
    'attachment-seed',
    'payload',
    'raw',
    staged.stdout.toString().trim(),
    'original.bin',
  ]);
  final missing = '${folder.path}/missing.db';
  var rejected = false;
  try {
    NativeHostSession(args[0], missing).close();
  } catch (_) {
    rejected = true;
  }
  check(
    rejected && !File(missing).existsSync(),
    'Open created missing database',
  );
  final session = NativeHostSession(args[0], db);
  final library = DynamicLibrary.open(args[0]);
  final allocate = library
      .lookupFunction<Uint32 Function(Uint32), int Function(int)>(
        'morrow_buffer_new',
      );
  final release = library
      .lookupFunction<Uint32 Function(Uint32), int Function(int)>(
        'morrow_buffer_free',
      );
  final pointer = library
      .lookupFunction<
        Pointer<Uint8> Function(Uint32),
        Pointer<Uint8> Function(int)
      >('morrow_buffer_ptr');
  final status = library
      .lookupFunction<Uint32 Function(Uint32), int Function(int)>(
        'morrow_buffer_status',
      );
  final open = library
      .lookupFunction<Uint32 Function(Uint32), int Function(int)>(
        'morrow_host_open',
      );
  final close = library
      .lookupFunction<Uint32 Function(Uint32), int Function(int)>(
        'morrow_host_close',
      );
  final connect = library
      .lookupFunction<Uint32 Function(Uint32), int Function(int)>(
        'morrow_host_connect',
      );
  final disconnect = library
      .lookupFunction<Uint32 Function(Uint32, Uint32), int Function(int, int)>(
        'morrow_host_disconnect',
      );
  final dispatch = library
      .lookupFunction<
        Uint32 Function(Uint32, Uint32, Uint32),
        int Function(int, int, int)
      >('morrow_host_dispatch');
  (int, int) input(Uint8List bytes, int Function(int) action) {
    final id = allocate(bytes.length);
    check(id != 0, 'Allocation failed');
    try {
      pointer(id).asTypedList(bytes.length).setAll(0, bytes);
      return (action(id), status(id));
    } finally {
      release(id);
    }
  }

  Uint8List text(String value) => Uint8List.fromList(utf8.encode(value));
  final request = RenameCommand(
    operationId: 'edit',
    cardId: 'card',
    expectedRevision: BigInt.one,
    title: 'native changed',
  );
  QueryOperationCommand query(String op, {String card = 'card'}) =>
      QueryOperationCommand(requestId: 'query', cardId: card, operationId: op);
  RuntimeReply result(NativeHostConnection c, QueryOperationCommand command) =>
      RuntimeReply.decode(
        c.dispatch(command.encode()),
        requestId: command.requestId,
      );
  final c = session.connect(), other = session.connect();
  try {
    check(input(text(db), open).$2 == 5, 'Duplicate native owner accepted');
    check(
      RuntimeReply.decode(
            c.dispatch(
              ReadSummaryCommand(requestId: 'read', cardId: 'card').encode(),
            ),
            requestId: 'read',
          ).failure ==
          'Denied',
      'Initial read allowed',
    );
    c.grant(NativeCapability.rename, 'card');
    check(
      result(c, query('edit')).failure == 'Denied',
      'Write implied result-query permission',
    );
    final held = <int>[];
    try {
      for (var i = 0; i < 15; i++) {
        final id = allocate(8);
        check(id != 0, 'Buffer setup failed');
        held.add(id);
      }
      var blocked = false;
      try {
        c.dispatch(request.encode());
      } on NativeHostException catch (error) {
        blocked = error.status == 3;
      }
      check(blocked, 'Write lacked response reservation');
    } finally {
      for (final id in held) {
        release(id);
      }
    }
    c.grant(NativeCapability.queryOperation, 'card');
    check(
      result(c, query('edit')).resultState == 'absentSnapshot',
      'Buffer failure still committed',
    );
    // Drop the successful response to simulate an ambiguous caller outcome.
    c.dispatch(request.encode());
    final recovered = result(c, query('edit'));
    check(
      recovered.resultState == 'locallyCommitted' &&
          recovered.revision == BigInt.two,
      'Lost-response query failed',
    );
    final retry = RuntimeReply.decode(
      c.dispatch(request.encode()),
      requestId: 'edit',
    );
    check(
      retry.revision == BigInt.two &&
          retry.eventId == recovered.eventId &&
          retry.contentSha256!.length == 32,
      'Retry changed result',
    );
    check(
      result(other, query('edit')).failure == 'Denied',
      'Other connection borrowed grant',
    );
    check(
      result(c, query('high-seed')).resultState == 'absentSnapshot',
      'Other card result leaked',
    );
    check(
      result(c, query('never-submitted')).resultState == 'absentSnapshot',
      'Absence overclaimed',
    );
    c.grant(NativeCapability.readSummary, 'card');
    final summary = RuntimeReply.decode(
      c.dispatch(
        ReadSummaryCommand(requestId: 'read', cardId: 'card').encode(),
      ),
      requestId: 'read',
    );
    check(
      summary.title == 'native changed' &&
          summary.revision == BigInt.two &&
          summary.typeId != null,
      'Summary decode failed',
    );
    var mismatch = false;
    try {
      RuntimeReply.decode(c.dispatch(request.encode()), requestId: 'wrong');
    } catch (_) {
      mismatch = true;
    }
    check(mismatch, 'Response correlation accepted');
    c.revoke(NativeCapability.queryOperation, 'card');
    check(
      result(c, query('edit')).failure == 'Denied',
      'Revoked query delivered',
    );
    c.grant(
      NativeCapability.queryOperation,
      'card',
      ttl: const Duration(milliseconds: 1),
    );
    await Future<void>.delayed(const Duration(milliseconds: 25));
    check(
      result(c, query('edit')).failure == 'Denied',
      'Expired query delivered',
    );
    c.grant(NativeCapability.rename, 'high');
    c.grant(NativeCapability.queryOperation, 'high');
    final high = RenameCommand(
      operationId: 'high-edit',
      cardId: 'high',
      expectedRevision: (BigInt.one << 64) - BigInt.two,
      title: '原生提交 🧭',
    );
    check(
      RuntimeReply.decode(
            c.dispatch(high.encode()),
            requestId: 'high-edit',
          ).revision ==
          (BigInt.one << 64) - BigInt.one,
      'Native UInt64 truncated',
    );
    check(
      result(c, query('high-edit', card: 'high')).revision ==
          (BigInt.one << 64) - BigInt.one,
      'Queried UInt64 truncated',
    );
    for (var i = 0; i < 32; i++) {
      var bad = false;
      try {
        c.dispatch(Uint8List.fromList([0, 1, 2]));
      } on NativeHostException catch (error) {
        bad = error.status == 2;
      }
      check(bad && session.liveBuffers == 0, 'Invalid input leaked');
    }
    ReadAttachmentCommand attachment(
      BigInt offset, {
      String id = 'attachment-1',
      BigInt? revision,
    }) => ReadAttachmentCommand(
      requestId: 'attachment-read',
      cardId: 'payload',
      attachmentId: id,
      expectedRevision: revision ?? BigInt.one,
      offset: offset,
    );
    RuntimeReply packet(ReadAttachmentCommand command) => RuntimeReply.decode(
      c.dispatch(command.encode()),
      requestId: command.requestId,
    );
    check(
      packet(attachment(BigInt.zero)).failure == 'Denied',
      'Attachment lacked independent permission',
    );
    c.grantAttachment('payload', 'attachment-1');
    check(
      packet(attachment(BigInt.zero, id: 'sibling')).failure == 'Denied',
      'Attachment scope widened',
    );
    final verifier = AttachmentTransferVerifier(
      cardId: 'payload',
      attachmentId: 'attachment-1',
      revision: BigInt.one,
    );
    var blocks = 0;
    do {
      final value = packet(attachment(verifier.nextOffset)).attachmentPart!;
      verifier.add(value);
      for (var i = 0; i < value.bytes.length; i++) {
        check(
          value.bytes[i] == (value.offset.toInt() + i) % 251,
          'Attachment bytes changed',
        );
      }
      blocks++;
    } while (verifier.nextOffset < BigInt.from(rawLength));
    verifier.finish();
    check(blocks > 160, 'Large file did not use bounded packets');
    c.revokeAttachment('payload', 'attachment-1');
    check(
      packet(attachment(BigInt.zero)).failure == 'Denied',
      'Revoked attachment read accepted',
    );
    c.grantAttachment(
      'payload',
      'attachment-1',
      ttl: const Duration(milliseconds: 1),
    );
    await Future<void>.delayed(const Duration(milliseconds: 25));
    check(
      packet(attachment(BigInt.zero)).failure == 'Denied',
      'Expired attachment read accepted',
    );
    c.grantAttachment('payload', 'attachment-1');
    cli(['rename-local', db, 'change-payload', 'payload', '1', 'new title']);
    check(
      packet(attachment(BigInt.zero)).failure == 'RevisionConflict',
      'Mixed attachment revision accepted',
    );
    check(
      packet(
            attachment(BigInt.zero, revision: BigInt.two),
          ).attachmentPart!.revision ==
          BigInt.two,
      'Explicit new revision failed',
    );
    // Exercise opaque host/connection limits directly, without exposing ids in app wrappers.
    final rawHosts = <int>[];
    try {
      for (var i = 0; i < 8; i++) {
        final path = '${folder.path}/limit-$i.db';
        cli(['init', path]);
        final (id, code) = input(text(path), open);
        if (i == 7) {
          check(id == 0 && code == 3, 'Host capacity bypass');
        } else {
          check(id != 0, 'Host creation failed');
          rawHosts.add(id);
        }
      }
      final connections = <int>[];
      for (var i = 0; i < 128; i++) {
        final id = connect(rawHosts.first);
        check(id != 0, 'Connection capacity early');
        connections.add(id);
      }
      check(connect(rawHosts.first) == 0, 'Connection capacity bypass');
      final foreign = connect(rawHosts[1]);
      final invalid = input(
        query('edit').encode(),
        (id) => dispatch(rawHosts.first, foreign, id),
      );
      check(
        invalid.$1 == 0 && invalid.$2 == 255,
        'Foreign connection accepted',
      );
      check(
        disconnect(rawHosts.first, connections.first) == 1,
        'Disconnect failed',
      );
      final replacement = connect(rawHosts.first);
      check(
        !connections.contains(replacement) && replacement != 0,
        'Connection reused',
      );
      final stale = input(
        query('edit').encode(),
        (id) => dispatch(rawHosts.first, connections.first, id),
      );
      check(stale.$1 == 0 && stale.$2 == 255, 'Stale connection accepted');
      close(rawHosts.first);
      final closed = input(
        query('edit').encode(),
        (id) => dispatch(rawHosts.first, replacement, id),
      );
      check(closed.$1 == 0 && closed.$2 == 255, 'Closed host accepted');
    } finally {
      for (final id in rawHosts) {
        close(id);
      }
    }
    check(
      session.liveHosts == 1 && session.liveBuffers == 0,
      'Native handles leaked',
    );
  } finally {
    other.close();
    c.close();
    session.close();
  }
  check(
    session.liveHosts == 0 && session.liveBuffers == 0,
    'Session cleanup leaked',
  );
  final reopened = NativeHostSession(args[0], db);
  final fresh = reopened.connect();
  try {
    check(
      result(fresh, query('edit')).failure == 'Denied',
      'Reopen revived grants',
    );
    fresh.grant(NativeCapability.queryOperation, 'card');
    check(
      result(fresh, query('edit')).revision == BigInt.two,
      'Reopen lost result',
    );
  } finally {
    fresh.close();
    reopened.close();
  }
  cli(['check', db]);
  check(
    reopened.liveHosts == 0 && reopened.liveBuffers == 0,
    'Reopen cleanup leaked',
  );
  print(
    'PASS: actual Dart FFI dispatch, scoped query, lost-response recovery, full UInt64, expiry/revocation, precommit response reservation, host/connection bounds, stale handles, 5 MiB attachment transfer with SHA verification and zero leaks.',
  );
}
