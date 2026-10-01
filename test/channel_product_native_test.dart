// Actual supervised RustWorkbench, original tool-packed archives, native sources and canonical Directory.
import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:math';
import 'dart:typed_data';
import 'package:crypto/crypto.dart';
import 'package:ffi/ffi.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/channel_task_models.dart';
import 'package:morrow_studio/plugins/plugin_library.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';
import 'package:morrow_studio/plugins/workbench_channel_supervised.dart';
import 'package:morrow_studio/plugins/workbench_supervision.dart';
import 'external_plugin_native_test.dart' show entireCatalog;

// Staging helpers for the independent real product qualification.
// No package rebuilding, invented callbacks, or protocol fixture emulation.

const expectedTranscript =
    '12a8659000a14e107b88bc40ee759d70c4749f68c01ee54157537b71008d6d16';

class PinnedChannelInput {
  const PinnedChannelInput(
    this.language,
    this.packageKey,
    this.wasmKey,
    this.archiveSha,
    this.wasmSha, {
    this.packageId = 'org.example.channel.sdk-native',
  });
  final String language, packageKey, wasmKey, archiveSha, wasmSha, packageId;
  String get package => Platform.environment[packageKey]!;
  Future<Map<String, Object>> verify() async {
    final archive = await File(package).readAsBytes();
    final wasm = await File(Platform.environment[wasmKey]!).readAsBytes();
    expect(
      sha256.convert(archive).toString(),
      archiveSha,
      reason: '$language original archive changed',
    );
    expect(
      sha256.convert(wasm).toString(),
      wasmSha,
      reason: '$language original Wasm changed',
    );
    return {
      'language': language,
      'archive_sha256': archiveSha,
      'wasm_sha256': wasmSha,
      'original_artifacts_unchanged': true,
    };
  }
}

const pinnedChannelInputs = [
  PinnedChannelInput(
    'Rust',
    'MORROW_SDK_CHANNEL_PACKAGE_RUST',
    'MORROW_RUST_CHANNEL_WASM',
    '7f9d2576e669338651d113681d95d8f5e32a1f97ce1beac5f8e75d68ff64e5d4',
    'b2996038c38e2888c492d10a6ce8758ec66590d60f8eaa5f3d549b2bda162036',
  ),
  PinnedChannelInput(
    'C',
    'MORROW_SDK_CHANNEL_PACKAGE_C',
    'MORROW_SDK_CHANNEL_GUEST_C',
    '59b7e0d104c6f7fd862e4c5f130643a937183c534677b241c544e7332ecba5aa',
    'b1067a9cef86095291b03732f9d6f37c52918a548f074069f733c86fc3dab928',
  ),
  PinnedChannelInput(
    'C++',
    'MORROW_SDK_CHANNEL_PACKAGE_CPP',
    'MORROW_SDK_CHANNEL_GUEST_CPP',
    'c3bd959883a91b8a378b7b47227c02daa3a30d0052f3de4c83c3ebf534ec8d2d',
    'f619497e97ab6d08d3543a5672a2e00f428f05ba8f10eb958561e467b4ec811e',
  ),
];

Uint8List framePayload(int sequence) =>
    Uint8List.fromList(List.generate(32768, (i) => (i + sequence * 17) % 251));

Map<String, Object> verifyChannelSummary(Uint8List summary, int mode) {
  expect(summary.length, 64);
  expect(summary.sublist(0, 4), [0x43, 0x48, 0x56, 0x31]);
  final value = ByteData.sublistView(summary);
  expect(value.getUint32(4, Endian.little), mode);
  final status = value.getUint32(8, Endian.little);
  // Original guests report the cleanup state at their exact Completion point.
  // A later host join must be asserted separately using production status.
  expect(status, anyOf(5, 6));
  final reclaimed = value.getUint32(12, Endian.little);
  expect(reclaimed, anyOf(0, 1));
  if (status == 6) {
    expect(reclaimed, 0, reason: 'ClosingUnconfirmed cannot prove reclamation');
  }
  expect(value.getUint64(16, Endian.little), 5);
  expect(value.getUint64(24, Endian.little), 163840);
  final digest = summary
      .sublist(32)
      .map((v) => v.toRadixString(16).padLeft(2, '0'))
      .join();
  expect(digest, expectedTranscript);
  return {
    'mode': mode,
    'completion_status': status,
    'completion_reported_resource_reclaimed': reclaimed == 1,
    'frames': 5,
    'bytes': 163840,
    'sha256': digest,
  };
}

const pinnedDirectoryInputs = [
  PinnedChannelInput(
    'Rust',
    'MORROW_SDK_CHANNEL_DIRECTORY_PACKAGE_RUST',
    'MORROW_SDK_CHANNEL_DIRECTORY_WASM_RUST',
    '89519a112990813f194cfe6a60b521fd6cd37502ce127e319cdf0a974168904f',
    'aa0bfd43c67d7194de70900546e4a4c3dc0ca25d32d68c06beccf1fac009cc38',
    packageId: 'org.morrow.channel.directory.rust',
  ),
  PinnedChannelInput(
    'C',
    'MORROW_SDK_CHANNEL_DIRECTORY_PACKAGE_C',
    'MORROW_SDK_CHANNEL_DIRECTORY_WASM_C',
    'f6266b3cf6ebfe363c84f68b2ae5d029f80825ecd4dd1f7474d5b30117594b5b',
    'e3d5e65ee6f83df32a3a57dd1e9a64ec6e96db81c60ed54dd64fdafb6445c491',
    packageId: 'org.morrow.channel.directory.c',
  ),
  PinnedChannelInput(
    'C++',
    'MORROW_SDK_CHANNEL_DIRECTORY_PACKAGE_CPP',
    'MORROW_SDK_CHANNEL_DIRECTORY_WASM_CPP',
    'b3bd950e92a1be11d06779069e3e764c1d2203294473a8aea33a804738d98c1d',
    '957cf4367f2d8e0a883796866d1f8eba8b6d1ebda1fb57918594ca6f496bf01b',
    packageId: 'org.morrow.channel.directory.cpp',
  ),
];

// Only the exact owned host process may receive the deliberate native fault.

final class HeldChannelHost {
  HeldChannelHost(this.pid, this.creationFiletime) {
    handle = _open(0x00101001, 0, pid);
    if (handle == nullptr) throw StateError('Cannot retain owned host handle');
    try {
      verify();
    } catch (_) {
      dispose();
      rethrow;
    }
  }
  static final _kernel = DynamicLibrary.open('kernel32.dll');
  static final _open = _kernel
      .lookupFunction<
        Pointer<Void> Function(Uint32, Int32, Uint32),
        Pointer<Void> Function(int, int, int)
      >('OpenProcess');
  static final _pid = _kernel
      .lookupFunction<
        Uint32 Function(Pointer<Void>),
        int Function(Pointer<Void>)
      >('GetProcessId');
  static final _times = _kernel
      .lookupFunction<
        Int32 Function(
          Pointer<Void>,
          Pointer<Uint64>,
          Pointer<Uint64>,
          Pointer<Uint64>,
          Pointer<Uint64>,
        ),
        int Function(
          Pointer<Void>,
          Pointer<Uint64>,
          Pointer<Uint64>,
          Pointer<Uint64>,
          Pointer<Uint64>,
        )
      >('GetProcessTimes');
  static final _terminate = _kernel
      .lookupFunction<
        Int32 Function(Pointer<Void>, Uint32),
        int Function(Pointer<Void>, int)
      >('TerminateProcess');
  static final _wait = _kernel
      .lookupFunction<
        Uint32 Function(Pointer<Void>, Uint32),
        int Function(Pointer<Void>, int)
      >('WaitForSingleObject');
  static final _close = _kernel
      .lookupFunction<
        Int32 Function(Pointer<Void>),
        int Function(Pointer<Void>)
      >('CloseHandle');
  final int pid, creationFiletime;
  late final Pointer<Void> handle;
  bool _disposed = false;

  void verify() {
    if (_disposed || _pid(handle) != pid) {
      throw StateError('Held owned process identity mismatch');
    }
    final values = calloc<Uint64>(4);
    try {
      if (_times(handle, values, values + 1, values + 2, values + 3) == 0 ||
          values[0] != creationFiletime) {
        throw StateError('Held owned process creation identity mismatch');
      }
    } finally {
      calloc.free(values);
    }
  }

  void crash() {
    verify();
    if (_terminate(handle, 74) == 0) {
      throw StateError('Deliberate owned host fault failed');
    }
  }

  Future<Map<String, Object>> exited() async {
    final timer = Stopwatch()..start();
    while (_wait(handle, 0) == 0x102) {
      if (timer.elapsed > const Duration(seconds: 8)) {
        throw StateError('Owned host process exit remained unconfirmed');
      }
      await Future<void>.delayed(const Duration(milliseconds: 10));
    }
    if (_wait(handle, 0) != 0) {
      throw StateError('Owned host wait did not prove actual exit');
    }
    verify();
    return {
      'pid': pid,
      'creation_filetime': creationFiletime,
      'held_kernel_handle': true,
      'actual_process_exit': true,
      'elapsed_ms': timer.elapsedMilliseconds,
    };
  }

  void dispose() {
    if (!_disposed) {
      _disposed = true;
      _close(handle);
    }
  }
}

// Staging fixture. Channel calls are added only after A/B publish stable types.

final class ChannelProductFixture {
  ChannelProductFixture._(this.directory, this.backend);
  final Directory directory;
  final RustWorkbench backend;
  late PluginLibraryEntry plugin;
  late BigInt registryRevision;
  int observationBatch = 0;
  SupervisedWorkbenchChannel get transport =>
      backend.channel as SupervisedWorkbenchChannel;

  static Future<ChannelProductFixture> open(
    String scenario,
    String package, {
    String expectedPackageId = 'org.example.channel.sdk-native',
  }) async {
    final directory = Directory(
      '${Platform.environment['MORROW_CHANNEL_PRODUCT_EVIDENCE']}/$scenario',
    );
    expect(
      directory.existsSync(),
      isFalse,
      reason: 'Production qualification requires a new evidence directory',
    );
    await directory.create(recursive: true);
    final backend = await RustWorkbench.open(
      executable: Platform.environment['MORROW_WORKBENCH_HOST']!,
      supervisorExecutable:
          Platform.environment['MORROW_WORKBENCH_SUPERVISOR']!,
      package: Platform.environment['MORROW_WORKBENCH_PACKAGE']!,
      directory: directory,
      managed: true,
    );
    final fixture = ChannelProductFixture._(directory, backend);
    try {
      expect(backend.channel, isA<SupervisedWorkbenchChannel>());
      final initialOwner = fixture.transport.supervision.value!.record;
      expect(initialOwner['phase'], 'Running');
      expect(initialOwner['child_creation_filetime'], greaterThan(0));
      final preview = await backend.inspectPlugin(package);
      expect(preview.entries.single.id, expectedPackageId);
      expect(preview.entries.single.available, isTrue);
      expect(
        preview.entries.single.digest
            .map((b) => b.toRadixString(16).padLeft(2, '0'))
            .join(),
        sha256.convert(await File(package).readAsBytes()).toString(),
        reason: 'Actual native catalog must decode the pinned raw archive',
      );
      await backend.importPlugin(
        package,
        preview.entries.single.digest,
        preview.revision,
      );
      await fixture.refresh();
      expect(fixture.plugin.digest, preview.entries.single.digest);
      await backend.configureExternal(
        fixture.plugin,
        fixture.registryRevision,
        [],
        true,
      );
      await fixture.refresh();
      expect(fixture.plugin.enabled, isTrue);
      expect(fixture.plugin.approved, isEmpty);
      expect(fixture.plugin.approvedIo, isEmpty);
      await fixture.record('owner-initial', {
        'supervision': initialOwner,
        'package_id': fixture.plugin.id,
        'package_digest': fixture.plugin.digest
            .map((b) => b.toRadixString(16).padLeft(2, '0'))
            .join(),
        'registry_revision': fixture.registryRevision.toString(),
      });
      return fixture;
    } catch (_) {
      await backend.close();
      rethrow;
    }
  }

  Future<void> refresh() async {
    final catalog = await entireCatalog(backend);
    plugin = catalog.entries.singleWhere((entry) => !entry.builtin);
    registryRevision = catalog.revision;
  }

  Future<void> record(String name, Map<String, Object?> value) async {
    await File('${directory.path}/$name.json').writeAsString(jsonEncode(value));
  }

  Future<void> close() async {
    await backend.close();
    expect(await backend.process.exitCode, 0);
    expect(transport.supervision.value!.released, isTrue);
    await record('owner-final', {
      'supervision': transport.supervision.value!.record,
      'actual_supervised_exit': true,
      'artificial_ack': false,
    });
  }
}

// Generic production input qualification; no old SDK65 input adapter.
void addDirectoryTests(bool enabled, List<PinnedChannelInput> inputs) {
  for (final input in inputs) {
    for (final kind in ChannelSourceKind.values) {
      final mode = kind == ChannelSourceKind.events ? 2 : 0;
      test(
        'original ${input.language} canonical Directory ${kind.name} executes through production API',
        () async {
          final originals = await input.verify();
          final fixture = await ChannelProductFixture.open(
            'directory-${input.language.replaceAll('+', 'p').toLowerCase()}-${kind.name}',
            input.package,
            expectedPackageId: input.packageId,
          );
          try {
            expect(fixture.backend.supportsLocalChannels, isTrue);
            expect(fixture.plugin.channelSupported, isTrue);
            expect(fixture.plugin.channelHandlers, [
              'channel.directory.consume',
            ]);
            final handler = fixture.plugin.handlers.single;
            expect(handler.inputType, 'morrow.channel.directory.v1');
            expect(handler.outputType, 'bytes');
            await expectLater(
              fixture.backend.transformExternal(
                fixture.plugin,
                fixture.registryRevision,
                handler,
                Uint8List(0),
              ),
              throwsStateError,
            );
            final previous = await fixture.backend.prepareChannel(
              prepareRequest(
                fixture,
                kind: kind,
                duplex: false,
                frames: 1,
                bytes: 32768,
                directoryInput: true,
              ),
            );
            final previousClosed = await fixture.backend.closeChannel(
              previous.key,
            );
            expect(previousClosed.resourceReclaimed, isTrue);
            expect(previousClosed.cleanupProof, ChannelCleanupProof.noProducer);
            expect(
              previousClosed.workerJoined,
              isFalse,
              reason: 'Never-run task cannot invent an execution thread join',
            );
            final prepared = await fixture.backend.prepareChannel(
              prepareRequest(
                fixture,
                kind: kind,
                duplex: false,
                frames: 5,
                bytes: 163840,
                directoryInput: true,
              ),
            );
            expect(
              hex(prepared.directory.scopeSha256),
              isNot(hex(previous.directory.scopeSha256)),
            );
            expect(hex(prepared.sourceEpoch), isNot(hex(previous.sourceEpoch)));
            for (var sequence = 1; sequence <= 5; sequence++) {
              final cursor = ByteData(8)..setUint64(0, sequence, Endian.little);
              final uploaded = await fixture.backend.appendChannel(
                prepared.key,
                ChannelSourceFrame(
                  sequence: BigInt.from(sequence),
                  bytes: framePayload(sequence),
                  cursor: kind == ChannelSourceKind.events
                      ? cursor.buffer.asUint8List()
                      : const [],
                ),
              );
              expect(uploaded.sourceFrames, 0);
              expect(uploaded.sourceBytes, BigInt.zero);
            }
            // The production typed profile verifies the exact canonical
            // Directory before creating its executor/source. This is a new
            // explicit host attempt, not replay of any admitted business call.
            await expectLater(
              fixture.backend.runChannel(prepared.key, previous.directory.wire),
              throwsStateError,
            );
            final refused = await fixture.backend.statusChannel(prepared.key);
            expect(refused.sourceFrames, 0);
            expect(refused.sourceBytes, BigInt.zero);
            expect(refused.taskState, ChannelTaskState.pending);
            expect(refused.lastAcked, BigInt.zero);
            expect(refused.acceptedSequence, BigInt.zero);
            expect(refused.workerJoined, isFalse);
            final invocation = prepared.directory.wire;
            await fixture.backend.runChannel(prepared.key, invocation);
            await expectLater(
              fixture.backend.runChannel(prepared.key, invocation),
              throwsStateError,
            );
            final completed = await waitJob(
              fixture,
              prepared,
              (value) => value.workerJoined && value.resourceReclaimed,
            );
            expect(
              completed.taskState,
              ChannelTaskState.success,
              reason: completed.taskError,
            );
            expect(completed.outputType, 'bytes');
            expect(
              hex(completed.inputSha256),
              sha256.convert(invocation).toString(),
            );
            final summary = verifyChannelSummary(completed.output, mode);
            expect(completed.lastAcked, BigInt.from(5));
            expect(completed.acceptedSequence, BigInt.zero);
            expect(completed.observedSequence, BigInt.zero);
            expect(completed.sourceFrames, 5);
            expect(completed.sourceBytes, BigInt.from(163840));
            expect(completed.cleanupProof, ChannelCleanupProof.joined);
            await expectLater(
              fixture.backend.runChannel(prepared.key, invocation),
              throwsStateError,
            );
            await fixture.record('preclose-result', {
              'snapshot': snapshotRecord(completed),
              'summary': summary,
              'observed_task_result_is_retained_history': true,
            });
            final closed = await fixture.backend.closeChannel(prepared.key);
            expect(closed.resourceReclaimed, isTrue);
            expect(closed.workerJoined, isTrue);
            expect(closed.cleanupProof, ChannelCleanupProof.joined);
            expect(closed.taskState, ChannelTaskState.unknown);
            expect(closed.output, isEmpty);
            await fixture.record('flow-result', {
              'status': 'passed',
              'originals': await input.verify(),
              'input_profile': 'morrow.channel.directory.v1',
              'canonical_input': true,
              'old_sdk65_adapter': false,
              'input_value_bytes': invocation.length,
              'summary': summary,
              'previous_directory_refused_before_source_start': snapshotRecord(
                refused,
              ),
              'final': snapshotRecord(closed),
              'preclose_task_result': snapshotRecord(completed),
              'unknown_did_not_become_success': true,
              'task_completion_is_not_consumption_ack': true,
              'source_join_is_not_worker_join': true,
              'duplicate_run_rejected_without_replay': true,
            });
            // Retain both the archive hash proof and an explicit typed-profile
            // record even if a subsequent graceful shutdown check fails.
            await fixture.record('original-inputs', originals);
          } finally {
            await fixture.close();
          }
        },
        skip: !enabled,
        timeout: const Timeout(Duration(minutes: 3)),
      );
    }
  }
}

// Channel-dependent section staged against A's published production models.
// Install only once B confirms the native status/lifecycle contract is stable.

String hex(List<int> value) =>
    value.map((b) => b.toRadixString(16).padLeft(2, '0')).join();

Uint8List submission() => Uint8List.fromList(
  List.generate(32, (_) => 1 + Random.secure().nextInt(255)),
);

Uint8List exerciseInput(ChannelTaskSnapshot job, int mode) =>
    Uint8List.fromList([mode, ...job.reference, ...job.sourceEpoch]);

Map<String, Object?> snapshotRecord(ChannelTaskSnapshot job) => {
  'key': hex(job.key),
  'submission': hex(job.submission),
  'reference': hex(job.reference),
  'source_epoch': hex(job.sourceEpoch),
  'directory_scope_sha256': hex(job.directory.scopeSha256),
  'phase': job.phase.name,
  'status': job.status.name,
  'last_acked': job.lastAcked.toString(),
  'accepted_sequence': job.acceptedSequence.toString(),
  'observed_sequence': job.observedSequence.toString(),
  'cleanup_proof': job.cleanupProof.name,
  'producer_outcome': job.producerOutcome.name,
  'task_state': job.taskState.name,
  'task_error': job.taskError,
  'output_type': job.outputType,
  'output_hex': hex(job.output),
  'input_sha256': hex(job.inputSha256),
  'uploaded_frames': job.uploadedFrames,
  'uploaded_bytes': job.uploadedBytes.toString(),
  'source_frames': job.sourceFrames,
  'source_bytes': job.sourceBytes.toString(),
  'resource_reclaimed': job.resourceReclaimed,
  'close_requested': job.closeRequested,
  'observed_bytes': job.observedBytes.toString(),
  'observed_sha256': hex(job.observedSha256),
  'worker_joined': job.workerJoined,
  'snapshot_pending': job.snapshotPending,
};

void sameBinding(ChannelTaskSnapshot actual, ChannelTaskSnapshot original) {
  expect(actual.key, original.key);
  expect(actual.submission, original.submission);
  expect(actual.reference, original.reference);
  expect(actual.sourceEpoch, original.sourceEpoch);
  expect(actual.directory.scopeSha256, original.directory.scopeSha256);
  expect(actual.directory.wire, original.directory.wire);
}

Future<ChannelTaskSnapshot> waitJob(
  ChannelProductFixture fixture,
  ChannelTaskSnapshot original,
  bool Function(ChannelTaskSnapshot) done,
) async {
  final watch = Stopwatch()..start();
  final batch = ++fixture.observationBatch;
  final observations = <Map<String, Object?>>[];
  final states = <String>{};
  while (watch.elapsed < const Duration(seconds: 12)) {
    final current = await fixture.backend.statusChannel(original.key);
    sameBinding(current, original);
    if (!current.snapshotPending) {
      expect(
        current.lastAcked,
        lessThanOrEqualTo(BigInt.from(current.sourceFrames)),
      );
      expect(
        BigInt.from(current.sourceFrames),
        lessThanOrEqualTo(current.lastAcked + BigInt.one),
        reason: 'Actual source credit keeps at most one unconsumed frame',
      );
      expect(
        current.observedSequence,
        lessThanOrEqualTo(current.acceptedSequence),
      );
      if (current.resourceReclaimed) {
        expect(current.cleanupProof, isNot(ChannelCleanupProof.pending));
      }
    }
    final record = snapshotRecord(current);
    final signature = jsonEncode(record);
    if (states.add(signature)) observations.add(record);
    if (!current.snapshotPending && done(current)) {
      await fixture.record('observations-${hex(original.key)}-$batch', {
        'snapshots': observations,
        'elapsed_ms': watch.elapsedMilliseconds,
        'polls_are_read_only_not_business_retries': true,
      });
      return current;
    }
    await Future<void>.delayed(const Duration(milliseconds: 20));
  }
  await fixture.record('observations-timeout-${hex(original.key)}-$batch', {
    'snapshots': observations,
    'elapsed_ms': watch.elapsedMilliseconds,
  });
  throw StateError(
    'Original channel lifecycle did not reach its required state',
  );
}

ChannelPrepareRequest prepareRequest(
  ChannelProductFixture fixture, {
  required ChannelSourceKind kind,
  required bool duplex,
  required int frames,
  required int bytes,
  Uint8List? id,
  int lifetimeMs = 30000,
  bool directoryInput = false,
}) {
  final budget = fixture.plugin.channelBudget!;
  expect(budget.maxChannels, 1);
  expect(budget.maxFrameBytes, directoryInput ? 65536 : 32768);
  expect(budget.maxBytes, BigInt.from(2 * 1024 * 1024));
  expect(budget.maxMessages, BigInt.from(directoryInput ? 32 : 64));
  expect(budget.maxRequests, BigInt.from(directoryInput ? 128 : 512));
  expect(budget.maxDurationMs, BigInt.from(30000));
  return ChannelPrepareRequest(
    submission: id ?? submission(),
    packageId: fixture.plugin.id,
    packageDigest: fixture.plugin.digest,
    registryRevision: fixture.registryRevision,
    handler: directoryInput ? 'channel.directory.consume' : 'channel.exercise',
    kind: kind,
    duplex: duplex,
    budget: budget,
    lifetimeMs: lifetimeMs,
    frameCount: frames,
    totalBytes: BigInt.from(bytes),
  );
}

Future<ChannelTaskSnapshot> prepareSink(
  ChannelProductFixture fixture, {
  int lifetimeMs = 30000,
}) async {
  final job = await fixture.backend.prepareChannel(
    prepareRequest(
      fixture,
      kind: ChannelSourceKind.byteStream,
      duplex: true,
      frames: 0,
      bytes: 0,
      lifetimeMs: lifetimeMs,
    ),
  );
  expect(job.sourceFrames, 0);
  expect(job.sourceBytes, BigInt.zero);
  expect(job.acceptedSequence, BigInt.zero);
  expect(job.lastAcked, BigInt.zero);
  return job;
}

Future<ChannelTaskSnapshot> stalledSink(ChannelProductFixture fixture) async {
  final job = await prepareSink(fixture);
  // Actual sink waits for Send while the original compiled guest requests
  // Receive. This creates a real protocol wait, with no fake clock or delay.
  await fixture.backend.runChannel(job.key, exerciseInput(job, 0));
  final running = await waitJob(
    fixture,
    job,
    (value) => value.phase == ChannelJobPhase.running,
  );
  expect(running.taskState, ChannelTaskState.pending);
  expect(running.workerJoined, isFalse);
  expect(running.resourceReclaimed, isFalse);
  expect(running.cleanupProof, ChannelCleanupProof.pending);
  expect(running.lastAcked, BigInt.zero);
  expect(running.acceptedSequence, BigInt.zero);
  return running;
}

void main() {
  final requiredEnvironment = [
    'MORROW_WORKBENCH_HOST',
    'MORROW_WORKBENCH_SUPERVISOR',
    'MORROW_WORKBENCH_PACKAGE',
    'MORROW_CHANNEL_PRODUCT_EVIDENCE',
    for (final input in pinnedChannelInputs) ...[
      input.packageKey,
      input.wasmKey,
    ],
    for (final input in pinnedDirectoryInputs) ...[
      input.packageKey,
      input.wasmKey,
    ],
  ];
  final enabled =
      Platform.isWindows &&
      requiredEnvironment.every(
        (key) => Platform.environment[key]?.isNotEmpty == true,
      );
  if (Platform.isWindows && !enabled) {
    final missing = requiredEnvironment
        .where((key) => Platform.environment[key]?.isNotEmpty != true)
        .join(', ');
    throw StateError(
      'Real channel product qualification requires every pinned input: $missing',
    );
  }
  addDirectoryTests(enabled, pinnedDirectoryInputs);

  for (final input in pinnedChannelInputs) {
    for (final mode in [0, 2, 1]) {
      final label = ['bytes', 'duplex', 'events'][mode];
      test(
        'original ${input.language} $label uses supervised production channel jobs',
        () async {
          final originals = await input.verify();
          final fixture = await ChannelProductFixture.open(
            '${input.language.replaceAll('+', 'p').toLowerCase()}-$label',
            input.package,
          );
          try {
            expect(fixture.backend.supportsLocalChannels, isTrue);
            expect(fixture.plugin.channelSupported, isTrue);
            expect(fixture.plugin.channelHandlers, ['channel.exercise']);
            await expectLater(
              fixture.backend.transformExternal(
                fixture.plugin,
                fixture.registryRevision,
                fixture.plugin.handlers.single,
                Uint8List(65),
              ),
              throwsStateError,
            );
            final kind = mode == 2
                ? ChannelSourceKind.events
                : ChannelSourceKind.byteStream;
            if (mode == 1) {
              final revision = fixture.registryRevision;
              expect(
                () => prepareRequest(
                  fixture,
                  kind: kind,
                  duplex: true,
                  frames: 5,
                  bytes: 163840,
                ),
                throwsFormatException,
              );
              await fixture.refresh();
              expect(fixture.registryRevision, revision);
            }
            final request = prepareRequest(
              fixture,
              kind: kind,
              duplex: mode == 1,
              frames: mode == 1 ? 0 : 5,
              bytes: mode == 1 ? 0 : 163840,
            );
            final prepared = await fixture.backend.prepareChannel(request);
            expect(prepared.directory.channels, hasLength(1));
            expect(
              prepared.directory.channels.single.reference,
              prepared.reference,
            );
            expect(
              prepared.directory.channels.single.sourceEpoch,
              prepared.sourceEpoch,
            );
            expect(prepared.directory.channels.single.kind, kind);
            expect(prepared.lastAcked, BigInt.zero);
            expect(prepared.acceptedSequence, BigInt.zero);
            await expectLater(
              fixture.backend.prepareChannel(request),
              throwsStateError,
            );
            var uploaded = prepared;
            for (
              var sequence = 1;
              sequence <= (mode == 1 ? 0 : 5);
              sequence++
            ) {
              final cursor = ByteData(8)..setUint64(0, sequence, Endian.little);
              uploaded = await fixture.backend.appendChannel(
                prepared.key,
                ChannelSourceFrame(
                  sequence: BigInt.from(sequence),
                  bytes: framePayload(sequence),
                  cursor: mode == 2 ? cursor.buffer.asUint8List() : const [],
                ),
              );
              sameBinding(uploaded, prepared);
              expect(uploaded.uploadedFrames, sequence);
              expect(uploaded.uploadedBytes, BigInt.from(sequence * 32768));
              expect(uploaded.sourceFrames, 0);
              expect(uploaded.sourceBytes, BigInt.zero);
              expect(uploaded.lastAcked, BigInt.zero);
            }
            await fixture.record('prepared-source', {
              'originals': originals,
              'snapshot': snapshotRecord(uploaded),
              'append_is_finite_host_staging_not_business_delivery': true,
            });
            final invocation = exerciseInput(prepared, mode);
            expect(invocation.length, 65);
            final admitted = await fixture.backend.runChannel(
              prepared.key,
              invocation,
            );
            sameBinding(admitted, prepared);
            await expectLater(
              fixture.backend.runChannel(prepared.key, invocation),
              throwsStateError,
            );
            final completed = await waitJob(
              fixture,
              prepared,
              (value) => value.workerJoined && value.resourceReclaimed,
            );
            expect(
              completed.taskState,
              ChannelTaskState.success,
              reason: completed.taskError,
            );
            expect(completed.outputType, 'bytes');
            expect(
              hex(completed.inputSha256),
              sha256.convert(invocation).toString(),
            );
            final summary = verifyChannelSummary(completed.output, mode);
            expect(completed.cleanupProof, ChannelCleanupProof.joined);
            if (mode == 1) {
              expect(completed.lastAcked, BigInt.zero);
              expect(completed.acceptedSequence, BigInt.from(5));
              expect(completed.observedSequence, BigInt.from(5));
              expect(completed.observedBytes, BigInt.from(163840));
              expect(hex(completed.observedSha256), expectedTranscript);
              for (var sequence = 1; sequence <= 5; sequence++) {
                final sent = await fixture.backend.readChannelSent(
                  prepared.key,
                  BigInt.from(sequence),
                );
                expect(sent, isNotNull);
                expect(sent!.sequence, BigInt.from(sequence));
                expect(sent.bytes, framePayload(sequence));
              }
              expect(
                await fixture.backend.readChannelSent(
                  prepared.key,
                  BigInt.from(6),
                ),
                isNull,
              );
            } else {
              expect(completed.lastAcked, BigInt.from(5));
              expect(completed.acceptedSequence, BigInt.zero);
              expect(completed.observedSequence, BigInt.zero);
              expect(completed.sourceFrames, 5);
              expect(completed.sourceBytes, BigInt.from(163840));
            }
            await expectLater(
              fixture.backend.runChannel(prepared.key, invocation),
              throwsStateError,
            );
            await fixture.record('preclose-result', {
              'snapshot': snapshotRecord(completed),
              'summary': summary,
              'observed_task_result_is_retained_history': true,
            });
            final closed = await fixture.backend.closeChannel(prepared.key);
            sameBinding(closed, prepared);
            expect(closed.workerJoined, isTrue);
            expect(closed.resourceReclaimed, isTrue);
            expect(closed.cleanupProof, ChannelCleanupProof.joined);
            expect(closed.taskState, ChannelTaskState.unknown);
            expect(closed.output, isEmpty);
            expect(closed.lastAcked, completed.lastAcked);
            expect(closed.acceptedSequence, completed.acceptedSequence);
            expect(closed.observedSequence, completed.observedSequence);
            await fixture.record('flow-result', {
              'status': 'passed',
              'originals': await input.verify(),
              'summary': summary,
              'final': snapshotRecord(closed),
              'preclose_task_result': snapshotRecord(completed),
              'unknown_did_not_become_success': true,
              'task_completion_is_not_consumption_ack': true,
              'send_admission_is_not_peer_receipt': true,
              'source_join_is_not_worker_join': true,
              'duplicate_run_rejected_without_replay': true,
              if (mode == 1) ...{
                'outgoing_duplex_sink_has_zero_incoming_frames': true,
                'positive_frame_duplex_rejected': true,
                'positive_frame_duplex_refusal_boundary':
                    'typed Dart API before transport',
              },
            });
          } finally {
            await fixture.close();
          }
        },
        skip: !enabled,
        timeout: const Timeout(Duration(minutes: 3)),
      );
    }
  }

  final original = pinnedChannelInputs.first;
  test(
    'real pending guest wait cancels through the original control and actually joins',
    () async {
      await original.verify();
      final fixture = await ChannelProductFixture.open(
        'cancel',
        original.package,
      );
      try {
        final running = await stalledSink(fixture);
        final foreign = await ChannelProductFixture.open(
          'foreign-owner',
          original.package,
        );
        try {
          await expectLater(
            foreign.backend.statusChannel(running.key),
            throwsStateError,
          );
          await expectLater(
            foreign.backend.runChannel(running.key, exerciseInput(running, 1)),
            throwsStateError,
          );
          await expectLater(
            foreign.backend.closeChannel(running.key),
            throwsStateError,
          );
        } finally {
          await foreign.close();
        }
        final afterForeign = await waitJob(
          fixture,
          running,
          (value) => value.phase == ChannelJobPhase.running,
        );
        expect(afterForeign.lastAcked, BigInt.zero);
        expect(afterForeign.acceptedSequence, BigInt.zero);
        final stop = await fixture.backend.closeChannel(running.key);
        expect(stop.closeRequested, isTrue);
        await expectLater(
          fixture.backend.runChannel(running.key, exerciseInput(running, 1)),
          throwsStateError,
        );
        final closed = await waitJob(
          fixture,
          running,
          (value) => value.workerJoined && value.resourceReclaimed,
        );
        expect(closed.status, ChannelStatus.revoked);
        expect(closed.cleanupProof, ChannelCleanupProof.joined);
        expect(closed.taskState, isNot(ChannelTaskState.success));
        expect(closed.acceptedSequence, BigInt.zero);
        expect(closed.lastAcked, BigInt.zero);
        await fixture.record('fault-result', {
          'status': 'passed',
          'before': snapshotRecord(running),
          'after': snapshotRecord(closed),
          'actual_native_wait': true,
          'business_gate_stopped': true,
          'foreign_host_key_rejected_without_stopping_original': true,
          'automatic_replay': false,
        });
      } finally {
        await fixture.close();
      }
    },
    skip: !enabled,
    timeout: const Timeout(Duration(minutes: 2)),
  );

  test(
    'active disable stops the original control before the explicit durable catalog retry',
    () async {
      final fixture = await ChannelProductFixture.open(
        'disable',
        original.package,
      );
      try {
        final running = await stalledSink(fixture);
        await expectLater(
          fixture.backend.configureExternal(
            fixture.plugin,
            fixture.registryRevision,
            [],
            false,
          ),
          throwsStateError,
        );
        final closed = await waitJob(
          fixture,
          running,
          (value) => value.workerJoined && value.resourceReclaimed,
        );
        expect(closed.status, ChannelStatus.revoked);
        expect(closed.cleanupProof, ChannelCleanupProof.joined);
        expect(closed.taskState, isNot(ChannelTaskState.success));
        final disconnected = await fixture.backend.closeChannel(running.key);
        expect(disconnected.phase, ChannelJobPhase.closed);
        expect(disconnected.workerJoined, isTrue);
        expect(disconnected.resourceReclaimed, isTrue);
        expect(disconnected.cleanupProof, ChannelCleanupProof.joined);
        expect(disconnected.taskState, ChannelTaskState.unknown);
        expect(disconnected.output, isEmpty);
        await fixture.refresh();
        expect(
          fixture.plugin.enabled,
          isTrue,
          reason: 'Initial Busy refusal did not claim a durable catalog change',
        );
        final fencedRequest = prepareRequest(
          fixture,
          kind: ChannelSourceKind.byteStream,
          duplex: true,
          frames: 0,
          bytes: 0,
        );
        expect(fencedRequest.packageDigest, fixture.plugin.digest);
        expect(fencedRequest.registryRevision, fixture.registryRevision);
        String? pendingStopError;
        try {
          await fixture.backend.prepareChannel(fencedRequest);
        } on StateError catch (error) {
          pendingStopError = error.toString();
        }
        expect(
          pendingStopError,
          contains(
            'apply the pending catalog stop or explicitly reapprove before a new source grant',
          ),
          reason: 'The original stop fence must survive join and disconnect',
        );
        await fixture.backend.configureExternal(
          fixture.plugin,
          fixture.registryRevision,
          [],
          false,
        );
        await fixture.refresh();
        expect(fixture.plugin.enabled, isFalse);
        await expectLater(
          fixture.backend.prepareChannel(
            prepareRequest(
              fixture,
              kind: ChannelSourceKind.byteStream,
              duplex: true,
              frames: 0,
              bytes: 0,
            ),
          ),
          throwsStateError,
        );
        await fixture.record('fault-result', {
          'status': 'passed',
          'after': snapshotRecord(closed),
          'after_explicit_disconnect': snapshotRecord(disconnected),
          'pending_stop_fence_refused_fresh_prepare': true,
          'pending_stop_fence_error': pendingStopError,
          'fenced_package_digest': hex(fencedRequest.packageDigest),
          'fenced_registry_revision': fencedRequest.registryRevision.toString(),
          'durable_enabled': fixture.plugin.enabled,
          'catalog_retry_was_explicit_after_join': true,
          'guest_replay': false,
        });
      } finally {
        await fixture.close();
      }
    },
    skip: !enabled,
    timeout: const Timeout(Duration(minutes: 2)),
  );

  test(
    'real monotonic expiry stops the native wait without extending the approved deadline',
    () async {
      final fixture = await ChannelProductFixture.open(
        'expiry',
        original.package,
      );
      try {
        final job = await prepareSink(fixture, lifetimeMs: 1500);
        final watch = Stopwatch()..start();
        await fixture.backend.runChannel(job.key, exerciseInput(job, 0));
        final running = await waitJob(
          fixture,
          job,
          (value) => value.phase == ChannelJobPhase.running,
        );
        final expired = await waitJob(
          fixture,
          job,
          (value) => value.workerJoined && value.resourceReclaimed,
        );
        expect(expired.status, ChannelStatus.expired);
        expect(expired.cleanupProof, ChannelCleanupProof.joined);
        expect(expired.taskState, isNot(ChannelTaskState.success));
        expect(expired.lastAcked, BigInt.zero);
        expect(expired.acceptedSequence, BigInt.zero);
        expect(watch.elapsed, lessThan(const Duration(seconds: 6)));
        await expectLater(
          fixture.backend.runChannel(job.key, exerciseInput(job, 1)),
          throwsStateError,
        );
        await fixture.record('fault-result', {
          'status': 'passed',
          'running': snapshotRecord(running),
          'after': snapshotRecord(expired),
          'actual_elapsed_ms': watch.elapsedMilliseconds,
          'original_lifetime_ms': 1500,
          'clock_mocked': false,
          'deadline_extended': false,
          'automatic_replay': false,
        });
      } finally {
        await fixture.close();
      }
    },
    skip: !enabled,
    timeout: const Timeout(Duration(minutes: 2)),
  );

  test(
    'fresh grant has a new epoch and old SDK endpoint never consumes its source',
    () async {
      final fixture = await ChannelProductFixture.open(
        'old-endpoint',
        original.package,
      );
      try {
        final old = await prepareSink(fixture);
        final oldClosed = await fixture.backend.closeChannel(old.key);
        expect(oldClosed.cleanupProof, ChannelCleanupProof.noProducer);
        expect(oldClosed.resourceReclaimed, isTrue);
        await expectLater(
          fixture.backend.runChannel(old.key, exerciseInput(old, 1)),
          throwsStateError,
        );
        final fresh = await fixture.backend.prepareChannel(
          prepareRequest(
            fixture,
            kind: ChannelSourceKind.byteStream,
            duplex: false,
            frames: 1,
            bytes: 32768,
          ),
        );
        expect(hex(fresh.key), isNot(hex(old.key)));
        expect(hex(fresh.reference), isNot(hex(old.reference)));
        expect(hex(fresh.sourceEpoch), isNot(hex(old.sourceEpoch)));
        await fixture.backend.appendChannel(
          fresh.key,
          ChannelSourceFrame(sequence: BigInt.one, bytes: framePayload(1)),
        );
        // bytes65 is application input: only the real broker validates the old
        // ref/epoch. The source is allowed to have started; it must still join.
        await fixture.backend.runChannel(fresh.key, exerciseInput(old, 0));
        final completed = await waitJob(
          fixture,
          fresh,
          (value) => value.workerJoined,
        );
        expect(completed.lastAcked, BigInt.zero);
        expect(completed.acceptedSequence, BigInt.zero);
        if (completed.output.isNotEmpty) {
          expect(completed.output.length, 64);
          final summary = ByteData.sublistView(completed.output);
          expect(summary.getUint64(16, Endian.little), 0);
          expect(summary.getUint64(24, Endian.little), 0);
          expect(summary.getUint32(8, Endian.little), isNot(anyOf(5, 6)));
        } else {
          expect(completed.taskState, isNot(ChannelTaskState.success));
        }
        await fixture.backend.closeChannel(fresh.key);
        final joined = await waitJob(
          fixture,
          fresh,
          (value) => value.workerJoined && value.resourceReclaimed,
        );
        expect(joined.cleanupProof, ChannelCleanupProof.joined);
        expect(joined.lastAcked, BigInt.zero);
        await expectLater(
          fixture.backend.runChannel(fresh.key, exerciseInput(fresh, 0)),
          throwsStateError,
        );
        await fixture.record('fault-result', {
          'status': 'passed',
          'old': snapshotRecord(oldClosed),
          'fresh': snapshotRecord(joined),
          'source_may_have_started': true,
          'zero_consumption_ack': true,
          'new_grant_is_not_old_endpoint_authority': true,
          'automatic_replay': false,
        });
      } finally {
        await fixture.close();
      }
    },
    skip: !enabled,
    timeout: const Timeout(Duration(minutes: 2)),
  );

  test(
    'actual owned host exit keeps Unknown and retained supervision without automatic replay',
    () async {
      final fixture = await ChannelProductFixture.open(
        'host-crash',
        original.package,
      );
      HeldChannelHost? host;
      HeldChannelHost? supervisor;
      var faultInjected = false;
      try {
        final running = await stalledSink(fixture);
        final owner = Map<String, dynamic>.of(
          fixture.transport.supervision.value!.record,
        );
        host = HeldChannelHost(
          owner['child_pid'] as int,
          owner['child_creation_filetime'] as int,
        );
        supervisor = HeldChannelHost(
          owner['supervisor_pid'] as int,
          owner['supervisor_creation_filetime'] as int,
        );
        host.crash();
        faultInjected = true;
        await expectLater(
          fixture.backend.statusChannel(running.key),
          throwsA(
            anyOf(isA<StateError>(), isA<WorkbenchSupervisionUnconfirmed>()),
          ),
        );
        await expectLater(
          fixture.backend.close(),
          throwsA(isA<WorkbenchSupervisionUnconfirmed>()),
        );
        final hostExit = await host.exited();
        final supervisorExit = await supervisor.exited();
        final retained = fixture.transport.supervision.value!;
        expect(retained.released, isFalse);
        expect(retained.record['owner_phase'], 'Retained');
        expect(retained.record['business_outcome'], 'Unknown');
        expect(retained.record['generation'], owner['generation']);
        await expectLater(
          RustWorkbench.open(
            executable: Platform.environment['MORROW_WORKBENCH_HOST']!,
            supervisorExecutable:
                Platform.environment['MORROW_WORKBENCH_SUPERVISOR']!,
            package: Platform.environment['MORROW_WORKBENCH_PACKAGE']!,
            directory: fixture.directory,
            managed: true,
          ),
          throwsA(isA<WorkbenchSupervisionUnconfirmed>()),
        );
        await fixture.record('fault-result', {
          'status': 'passed',
          'before': snapshotRecord(running),
          'initial_owner': owner,
          'final_owner': retained.record,
          'host_exit': hostExit,
          'supervisor_exit': supervisorExit,
          'automatic_replay': false,
          'source_join_after_host_death': 'unavailable; never invented',
          'originals': await original.verify(),
        });
      } finally {
        if (faultInjected) {
          try {
            await fixture.backend.close();
          } catch (_) {}
        } else {
          await fixture.close();
        }
        host?.dispose();
        supervisor?.dispose();
        // The original uncertain owner/evidence remains on disk. Normal-close
        // release is intentionally not asserted after this real host fault.
      }
    },
    skip: !enabled,
    timeout: const Timeout(Duration(minutes: 2)),
  );
}
