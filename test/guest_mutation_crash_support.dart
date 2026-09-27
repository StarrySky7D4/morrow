// Shared real guest setup for native and widget crash-recovery acceptance.
// The caller owns the host process, temporary directory, and cleanup.
import 'dart:async';
import 'dart:io';
import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/guest_mutation_execution_session.dart';
import 'package:morrow_studio/plugins/guest_mutation_models.dart';
import 'package:morrow_studio/plugins/mutation_task_models.dart';
import 'package:morrow_studio/plugins/mutation_workflow.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

import 'external_plugin_native_test.dart' show entireCatalog;

const guestCrashSubject = guestMutationExecutionSubject;

Uint8List guestCrashToken(int value) => Uint8List(32)..[0] = value;

bool guestCrashEffectHappened(String kind, String point, bool expectCrash) {
  const createPoints = {
    'after-claim',
    'after-temp',
    'after-write-chunk',
    'after-write',
    'after-flush',
    'after-publish',
    'after-effect',
    'after-observe',
  };
  const deletePoints = {'after-claim', 'after-effect', 'after-observe'};
  if (kind == 'create' && createPoints.contains(point)) {
    return !expectCrash ||
        {'after-publish', 'after-effect', 'after-observe'}.contains(point);
  }
  if (kind == 'delete' && deletePoints.contains(point)) {
    return !expectCrash || point != 'after-claim';
  }
  throw const FormatException('Invalid guest crash fault point');
}

Uint8List guestCrashContent() {
  final declared = Platform.environment['MORROW_GUEST_MUTATION_CONTENT_BYTES'];
  final length = declared == null
      ? 3 * GuestMutationValidation.maxChunkBytes + 73
      : int.tryParse(declared);
  if (length == null ||
      length <= 0 ||
      length > GuestMutationValidation.maxContentBytes) {
    throw const FormatException('Invalid guest crash content length');
  }
  final result = Uint8List(length);
  for (var offset = 0, counter = 0; offset < length; counter++) {
    final block = sha256.convert(<int>[
      ...'morrow.guest.crash.four-chunks.v1'.codeUnits,
      ...List<int>.generate(8, (index) => (counter >> (index * 8)) & 255),
    ]).bytes;
    final remaining = length - offset;
    final take = remaining < block.length ? remaining : block.length;
    result.setRange(offset, offset + take, block);
    offset += take;
  }
  return result;
}

const guestContentCrashPoints = {
  'file-content-after-bytes',
  'file-content-after-receipt',
  'file-content-before-commit',
  'file-content-after-commit',
};

/// The plan file is captured immediately after the first review, before Commit.
Future<File> saveGuestContentCrashPlan(
  Directory directory,
  Uint8List plan,
) async {
  expect(plan, isNotEmpty);
  final file = File(
    '${directory.path}${Platform.pathSeparator}expected-plan.bin',
  );
  await file.writeAsBytes(plan, flush: true);
  return file;
}

/// Runs the independent Store verifier only after every host using this Store exits.
Future<void> verifyGuestContentCrashStore({
  required Directory directory,
  required Uint8List plan,
  required Uint8List content,
  required String point,
  required bool expectCrash,
}) async {
  if (!guestContentCrashPoints.contains(point) ||
      content.length != GuestMutationValidation.maxContentBytes) {
    throw const FormatException('Invalid content crash verifier input');
  }
  final executable =
      Platform.environment['MORROW_GUEST_MUTATION_STORE_VERIFIER'];
  if (executable == null || !File(executable).existsSync()) {
    throw StateError('Missing independent Store verifier executable');
  }
  final planFile = File(
    '${directory.path}${Platform.pathSeparator}expected-plan.bin',
  );
  expect(await planFile.readAsBytes(), plan);
  final committed = !expectCrash || point == 'file-content-after-commit';
  final child = await Process.start(executable, [
    directory.path,
    planFile.path,
    committed ? '1' : '0',
  ], runInShell: false);
  final stdoutFuture = child.stdout.transform(systemEncoding.decoder).join();
  final stderrFuture = child.stderr.transform(systemEncoding.decoder).join();
  int exitCode;
  try {
    exitCode = await child.exitCode.timeout(const Duration(seconds: 30));
  } on TimeoutException {
    child.kill();
    try {
      await child.exitCode.timeout(const Duration(seconds: 5));
    } on TimeoutException {
      throw StateError('Independent Store verifier exit remains unknown');
    }
    throw StateError('Independent Store verifier timed out');
  }
  final stdout = await stdoutFuture;
  final stderr = await stderrFuture;
  expect(exitCode, 0, reason: stderr);
  final expectedBytes = committed ? content.length : 0;
  final lines = stdout.trim().split(RegExp(r'\r?\n'));
  expect(lines, contains('GUEST_CONTENT_STORE_PASS=${committed ? 1 : 0}'));
  expect(lines, contains('GUEST_CONTENT_STORE_BYTES=$expectedBytes'));
  expect(
    lines,
    contains('GUEST_CONTENT_BODY_SHA256=${sha256.convert(content)}'),
  );
  // Print only independently verified markers for the strict matrix runner.
  // ignore: avoid_print
  print('GUEST_CONTENT_STORE_PASS=${committed ? 1 : 0}');
  // ignore: avoid_print
  print('GUEST_CONTENT_STORE_BYTES=$expectedBytes');
  // ignore: avoid_print
  print('GUEST_CONTENT_BODY_SHA256=${sha256.convert(content)}');
}

final class GuestCrashPrepared {
  GuestCrashPrepared({
    required this.session,
    required this.plugin,
    required this.registryRevision,
    required this.target,
    required this.content,
    required this.plan,
    required this.planSha256,
    required this.operationId,
    required this.disposition,
  });

  final GuestMutationExecutionSession session;
  final PluginLibraryEntry plugin;
  final BigInt registryRevision;
  final File target;
  final Uint8List content, plan, planSha256;
  final String operationId;
  final MutationDisposition disposition;
}

Future<GuestCrashPrepared> prepareGuestCrash({
  required RustWorkbench backend,
  required String guestPackagePath,
  required Directory directory,
  required String kind,
  required String point,
  required String language,
}) async {
  // Existing OS-effect crash coverage only accepts native Create/Delete points.
  guestCrashEffectHappened(kind, point, true);
  final reviewed = await reviewGuestCrash(
    backend: backend,
    guestPackagePath: guestPackagePath,
    directory: directory,
    kind: kind,
    point: point,
    language: language,
  );
  final prepareClock = Stopwatch()..start();
  await reviewed.session.prepare(reviewedPlanSha256: reviewed.planSha256);
  prepareClock.stop();
  expect(reviewed.session.error, isNull);
  expect(reviewed.session.canExecute, isTrue);
  if (kind == 'create') {
    expect(reviewed.session.state?.durableContent, isTrue);
    expect(reviewed.session.stagedBytes, BigInt.from(reviewed.content.length));
    expect(reviewed.target.existsSync(), isFalse);
  } else {
    expect(reviewed.target.existsSync(), isTrue);
  }
  // ignore: avoid_print
  print('GUEST_CRASH_PREPARE_MS=${prepareClock.elapsedMilliseconds}');
  return reviewed;
}

/// Imports and reviews the exact plan without admitting content or executing it.
Future<GuestCrashPrepared> reviewGuestCrash({
  required RustWorkbench backend,
  required String guestPackagePath,
  required Directory directory,
  required String kind,
  required String point,
  required String language,
}) async {
  if (!{'rust', 'c', 'cpp'}.contains(language)) {
    throw const FormatException('Invalid guest crash fixture parameters');
  }
  if (kind == 'create' && guestContentCrashPoints.contains(point)) {
    // Commit is the faulting action; review must complete first.
  } else {
    guestCrashEffectHappened(kind, point, true);
  }
  final preview = await backend.inspectPlugin(guestPackagePath);
  final candidate = preview.entries.single;
  expect(candidate.mutationSupported, isTrue);
  expect(candidate.mutationBudget?.maxJobBytes, BigInt.from(32 << 20));
  expect(candidate.mutationBudget?.maxBytes, BigInt.from(256 << 20));
  await backend.importPlugin(
    guestPackagePath,
    candidate.digest,
    preview.revision,
  );
  var catalog = await entireCatalog(backend);
  var plugin = catalog.entries.singleWhere((entry) => entry.id == candidate.id);
  await backend.configureExternalIo(plugin, catalog.revision, [
    'file-create',
    'file-delete',
  ]);
  catalog = await entireCatalog(backend);
  plugin = catalog.entries.singleWhere((entry) => entry.id == candidate.id);
  await backend.configureExternal(plugin, catalog.revision, [], true);
  catalog = await entireCatalog(backend);
  plugin = catalog.entries.singleWhere((entry) => entry.id == candidate.id);
  expect(plugin.enabled, isTrue);
  expect(plugin.approvedIo, containsAll(['file-create', 'file-delete']));

  final targets = Directory('${directory.path}${Platform.pathSeparator}targets')
    ..createSync();
  final disposition = kind == 'create'
      ? MutationDisposition.create
      : MutationDisposition.delete;
  final target = File(
    '${targets.path}${Platform.pathSeparator}${kind == 'create' ? 'created.bin' : 'deleted.bin'}',
  );
  final content = kind == 'create'
      ? guestCrashContent()
      : Uint8List.fromList([4, 5, 6]);
  if (kind == 'delete') {
    target.writeAsBytesSync(content, flush: true);
    // Selection may hold an exclusive Windows handle; inspect beforehand.
    expect(target.readAsBytesSync(), content);
  } else {
    expect(target.existsSync(), isFalse);
  }

  final session = GuestMutationExecutionSession.forBackend(
    backend.guestMutationTasks,
    backend,
  );
  await session.refresh();
  expect(session.canStart, isTrue);
  final operationId = 'guest-crash-$language-$kind-$point';
  final reviewClock = Stopwatch()..start();
  await session.review(
    GuestMutationStartRequest(
      selection: MutationStartRequest(
        submission: guestCrashToken(1),
        packageId: plugin.id,
        packageDigest: plugin.digest,
        registryRevision: catalog.revision,
        disposition: disposition,
        selectedPath: kind == 'create' ? targets.path : target.path,
        relativePath: kind == 'create' ? 'created.bin' : '',
        subject: guestCrashSubject,
        approvalSha256: guestCrashToken(99),
        timeoutMs: 30000,
      ),
      approvedBudget: ApprovedGuestBudget(
        maxJobBytes: plugin.mutationBudget!.maxJobBytes,
        maxBytes: plugin.mutationBudget!.maxBytes,
      ),
    ),
    operationId: operationId,
    content: kind == 'create' ? content : null,
  );
  expect(session.error, isNull);
  expect(session.canPrepare, isTrue);
  reviewClock.stop();
  final plan = Uint8List.fromList(session.plan!);
  final planHash = Uint8List.fromList(session.planSha256!);
  expect(plan, isNotEmpty);
  // ignore: avoid_print
  print('GUEST_CRASH_CONTENT_BYTES=${kind == 'create' ? content.length : 3}');
  // ignore: avoid_print
  print(
    'GUEST_CRASH_CHUNKS=${kind == 'create' ? (content.length + GuestMutationValidation.maxChunkBytes - 1) ~/ GuestMutationValidation.maxChunkBytes : 0}',
  );
  // ignore: avoid_print
  print('GUEST_CRASH_REVIEW_MS=${reviewClock.elapsedMilliseconds}');
  return GuestCrashPrepared(
    session: session,
    plugin: plugin,
    registryRevision: catalog.revision,
    target: target,
    content: content,
    plan: plan,
    planSha256: planHash,
    operationId: operationId,
    disposition: disposition,
  );
}
