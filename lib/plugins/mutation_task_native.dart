import 'dart:typed_data';
import 'generated/host.capnp.dart' as host;
import 'mutation_task_codec_native.dart';
import 'mutation_task_models.dart';

typedef MutationTaskExchange =
    Future<T> Function<T>(
      host.Action action, {
      void Function(host.RequestBuilder)? configure,
      required T Function(host.ResponseReader) decode,
      required bool clearReply,
    });

/// Typed native-only scheduler. Every call is a single explicit wire exchange.
final class NativeMutationTaskClient implements MutationTaskBackend {
  NativeMutationTaskClient(this._exchange);
  final MutationTaskExchange _exchange;

  static Uint8List _identity(Uint8List value) =>
      MutationTaskValidation.identity(value);

  @override
  Future<MutationTaskReply> startSelected(MutationStartRequest request) {
    MutationTaskValidation.validateStart(request);
    return _exchange(
      host.Action.mutationStart,
      configure: (r) =>
          MutationTaskCodec.writeStart(request, r.initMutationStart()),
      clearReply: true,
      decode: (r) => MutationTaskCodec.reply(
        r,
        submission: request.submission,
        startedKind: MutationCommandKind.select,
      ),
    );
  }

  @override
  Future<MutationTaskReply> startReconciliation(
    MutationReconcileRequest request,
  ) {
    MutationTaskValidation.validateReconciliation(request);
    return _exchange(
      host.Action.mutationReconcile,
      configure: (r) => MutationTaskCodec.writeReconciliation(
        request,
        r.initMutationReconcile(),
      ),
      clearReply: true,
      decode: (r) => MutationTaskCodec.reply(
        r,
        submission: request.submission,
        startedKind: MutationCommandKind.reconcile,
      ),
    );
  }

  @override
  Future<MutationTaskReply> startDiscovery(MutationDiscoverRequest request) {
    MutationTaskValidation.validateDiscovery(request);
    return _exchange(
      host.Action.mutationDiscover,
      configure: (r) =>
          MutationTaskCodec.writeDiscovery(request, r.initMutationDiscover()),
      clearReply: true,
      decode: (r) => MutationTaskCodec.reply(
        r,
        submission: request.submission,
        startedKind: MutationCommandKind.discover,
      ),
    );
  }

  Future<MutationTaskReply> _submit(
    Uint8List key,
    Uint8List submission,
    MutationCommandKind kind, {
    Uint8List? plan,
    BigInt? offset,
    Uint8List? bytes,
    String? operationId,
    BigInt? contentLength,
    Uint8List? contentSha256,
    int? scanLimit,
  }) {
    final safeKey = _identity(key), safeSubmission = _identity(submission);
    if (kind == MutationCommandKind.prepare) {
      if (plan == null) throw const FormatException('Missing mutation plan');
      MutationTaskValidation.plan(plan);
    } else if (plan != null) {
      throw const FormatException('Unexpected mutation plan');
    }
    if (kind == MutationCommandKind.chunk) {
      if (offset == null || bytes == null) {
        throw const FormatException('Missing mutation chunk');
      }
      MutationTaskValidation.chunk(offset, bytes);
    } else if (offset != null || bytes != null) {
      throw const FormatException('Unexpected mutation chunk');
    }
    if (kind == MutationCommandKind.buildPlan) {
      if (operationId == null || contentLength == null) {
        throw const FormatException('Missing mutation plan specification');
      }
      MutationTaskValidation.buildPlan(
        operationId,
        contentLength,
        contentSha256,
      );
    } else if (operationId != null ||
        contentLength != null ||
        contentSha256 != null) {
      throw const FormatException('Unexpected mutation plan specification');
    }
    if (kind == MutationCommandKind.discover) {
      if (scanLimit == null) {
        throw const FormatException('Missing mutation plan scan limit');
      }
      MutationTaskValidation.scanLimit(scanLimit);
    } else if (scanLimit != null) {
      throw const FormatException('Unexpected mutation plan scan limit');
    }
    final safePlan = plan == null ? null : Uint8List.fromList(plan);
    final safeBytes = bytes == null ? null : Uint8List.fromList(bytes);
    final safeContentSha256 = contentSha256 == null
        ? null
        : Uint8List.fromList(contentSha256);
    return _exchange(
      host.Action.mutationSubmit,
      configure: (r) {
        r.ioKey = safeKey;
        MutationTaskCodec.writeCommand(
          submission: safeSubmission,
          kind: kind,
          plan: safePlan,
          offset: offset,
          bytes: safeBytes,
          operationId: operationId,
          contentLength: contentLength,
          contentSha256: safeContentSha256,
          scanLimit: scanLimit,
          out: r.initMutationCommand(),
        );
      },
      clearReply: true,
      decode: (r) =>
          MutationTaskCodec.reply(r, key: safeKey, submittedKind: kind),
    );
  }

  @override
  Future<MutationTaskReply> submitPrepare(
    Uint8List key,
    Uint8List submission,
    Uint8List plan,
  ) => _submit(key, submission, MutationCommandKind.prepare, plan: plan);
  @override
  Future<MutationTaskReply> submitBuildPlan(
    Uint8List key,
    Uint8List submission, {
    required String operationId,
    required BigInt contentLength,
    Uint8List? contentSha256,
  }) => _submit(
    key,
    submission,
    MutationCommandKind.buildPlan,
    operationId: operationId,
    contentLength: contentLength,
    contentSha256: contentSha256,
  );
  @override
  Future<MutationTaskReply> submitNextPlans(
    Uint8List key,
    Uint8List submission,
    int scanLimit,
  ) => _submit(
    key,
    submission,
    MutationCommandKind.discover,
    scanLimit: scanLimit,
  );
  @override
  Future<MutationTaskReply> submitChunk(
    Uint8List key,
    Uint8List submission,
    BigInt offset,
    Uint8List bytes,
  ) => _submit(
    key,
    submission,
    MutationCommandKind.chunk,
    offset: offset,
    bytes: bytes,
  );
  @override
  Future<MutationTaskReply> submitCommitContent(
    Uint8List key,
    Uint8List submission,
  ) => _submit(key, submission, MutationCommandKind.commitContent);
  @override
  Future<MutationTaskReply> submitExecute(
    Uint8List key,
    Uint8List submission,
  ) => _submit(key, submission, MutationCommandKind.execute);
  @override
  Future<MutationTaskReply> submitQuery(Uint8List key, Uint8List submission) =>
      _submit(key, submission, MutationCommandKind.query);
  @override
  Future<MutationTaskReply> submitCancelPlan(
    Uint8List key,
    Uint8List submission,
  ) => _submit(key, submission, MutationCommandKind.cancelPlan);
  @override
  Future<MutationTaskReply> submitRelease(
    Uint8List key,
    Uint8List submission,
  ) => _submit(key, submission, MutationCommandKind.release);

  @override
  Future<MutationTaskReply> status(Uint8List key) {
    final safeKey = _identity(key);
    return _exchange(
      host.Action.mutationStatus,
      configure: (r) => r.ioKey = safeKey,
      clearReply: true,
      decode: (r) => MutationTaskCodec.reply(r, key: safeKey, status: true),
    );
  }

  @override
  Future<MutationTaskRead> read(Uint8List key, BigInt commandId) {
    final safeKey = _identity(key);
    MutationTaskValidation.commandId(commandId);
    return _exchange(
      host.Action.mutationRead,
      configure: (r) {
        r.ioKey = safeKey;
        r.mutationCommandIdBigInt = commandId;
      },
      clearReply: true,
      decode: (r) {
        final reply = MutationTaskCodec.reply(
          r,
          key: safeKey,
          commandId: commandId,
        );
        final result = MutationTaskCodec.result(r.mutationResult);
        if (result == null &&
                reply.state.delivery == MutationDelivery.consumed ||
            result != null &&
                reply.state.delivery != MutationDelivery.consumed) {
          throw const FormatException('Mutation result delivery changed');
        }
        if (result != null) {
          final expected = switch (reply.state.kind) {
            MutationCommandKind.select => {MutationResultKind.selected},
            MutationCommandKind.prepare => {MutationResultKind.prepared},
            MutationCommandKind.chunk ||
            MutationCommandKind.commitContent => {MutationResultKind.staged},
            MutationCommandKind.execute => {
              MutationResultKind.created,
              MutationResultKind.deleted,
            },
            MutationCommandKind.query => {MutationResultKind.history},
            MutationCommandKind.cancelPlan => {
              MutationResultKind.planCancelled,
            },
            MutationCommandKind.release => {MutationResultKind.released},
            MutationCommandKind.reconcile => {MutationResultKind.reconciled},
            MutationCommandKind.buildPlan => {MutationResultKind.planned},
            MutationCommandKind.discover => {MutationResultKind.plans},
          };
          if (result.kind != MutationResultKind.failure &&
              !expected.contains(result.kind)) {
            throw const FormatException(
              'Mutation result belongs to another command',
            );
          }
          if (result.kind == MutationResultKind.selected) {
            final a = result.reference, b = reply.state.reference;
            if (a == null ||
                b == null ||
                a.length != b.length ||
                !Iterable<int>.generate(a.length).every((i) => a[i] == b[i])) {
              throw const FormatException(
                'Mutation selection identity changed',
              );
            }
            final expected = result.expectedIdentity;
            final stateExpected = reply.state.expectedIdentity;
            if ((expected == null) != (stateExpected == null) ||
                (expected != null &&
                    stateExpected != null &&
                    (expected.length != stateExpected.length ||
                        !Iterable<int>.generate(
                          expected.length,
                        ).every((i) => expected[i] == stateExpected[i])))) {
              throw const FormatException('Mutation expected identity changed');
            }
          }
          if (reply.state.kind == MutationCommandKind.discover &&
              ((result.kind == MutationResultKind.plans &&
                      result.done != reply.state.terminal) ||
                  (result.kind == MutationResultKind.failure &&
                      !reply.state.terminal))) {
            throw const FormatException(
              'Inconsistent mutation discovery state',
            );
          }
        }
        return MutationTaskRead(reply: reply, result: result);
      },
    );
  }

  @override
  Future<MutationTaskReply> cancelCommand(Uint8List key, BigInt commandId) {
    final safeKey = _identity(key);
    MutationTaskValidation.commandId(commandId);
    return _exchange(
      host.Action.mutationCancelCommand,
      configure: (r) {
        r.ioKey = safeKey;
        r.mutationCommandIdBigInt = commandId;
      },
      clearReply: true,
      decode: (r) =>
          MutationTaskCodec.reply(r, key: safeKey, commandId: commandId),
    );
  }
}
