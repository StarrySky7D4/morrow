import 'dart:math';
import 'dart:typed_data';
import 'agent_session_control.dart';
import 'agent_session_models.dart';

/// Typed input to the private PS30 codec. The Rust bridge constructs R2 with
/// current digests/generation and validates its canonical reply. No R2 encoder
/// or caller-provided canonical request exists in this API.
final class AgentSessionWireRequest {
  AgentSessionWireRequest({
    required this.operation,
    required List<int> id,
    required List<int> taskKey,
    required BigInt attachment,
    BigInt? submission,
    BigInt? generation,
    BigInt? writerSlot,
    this.sessionId = '',
    this.command,
    this.pendingCommand,
    this.pendingOperation,
    this.expectedGeneration,
    List<int>? pendingRequestId,
  }) : id = AgentSessionValidation.bytes(id, 32, nonzero32: true),
       taskKey = AgentSessionValidation.bytes(taskKey, 32, nonzero32: true),
       attachment = AgentSessionValidation.u64(attachment),
       submission = AgentSessionValidation.u64(submission ?? BigInt.zero),
       generation = AgentSessionValidation.u64(generation ?? BigInt.zero),
       writerSlot = AgentSessionValidation.u64(writerSlot ?? BigInt.zero),
       pendingRequestId = pendingRequestId == null
           ? null
           : AgentSessionValidation.bytes(
               pendingRequestId,
               32,
               nonzero32: true,
             ) {
    if ((operation == AgentSessionOperation.attach) !=
            (attachment == BigInt.zero) ||
        (operation == AgentSessionOperation.submit) != (command != null) ||
        command is AgentSessionAdmitWriter ||
        command is AgentSessionCloseWriter ||
        (operation == AgentSessionOperation.read) !=
            (this.submission != BigInt.zero) ||
        (operation == AgentSessionOperation.admitWriter) !=
            sessionId.isNotEmpty ||
        (operation == AgentSessionOperation.closeWriter) !=
            (this.writerSlot != BigInt.zero) ||
        operation != AgentSessionOperation.submit &&
            this.generation != BigInt.zero ||
        operation != AgentSessionOperation.read &&
            (pendingCommand != null ||
                pendingOperation != null ||
                expectedGeneration != null ||
                pendingRequestId != null) ||
        operation == AgentSessionOperation.read &&
            (pendingOperation == null || pendingRequestId == null)) {
      throw const FormatException('Unexpected private session request fields');
    }
    if (sessionId.isNotEmpty) AgentSessionValidation.identity(sessionId);
  }
  final AgentSessionOperation operation;
  final Uint8List id, taskKey;
  final BigInt attachment, submission, generation, writerSlot;
  final String sessionId;
  final AgentSessionCommand? command;
  // Decoder-only context retained from the original correlated submission.
  // These fields MUST NOT be serialized as new request fields or authority.
  final AgentSessionCommand? pendingCommand;
  final AgentSessionOperation? pendingOperation;
  final BigInt? expectedGeneration;
  final Uint8List? pendingRequestId;
}

/// Supply only the codec verified against real generated Rust layouts/vectors.
/// Its decoder must correlate schema/version/id/task/op/request hash, enforce
/// exact canonical framing and fully validate the typed R2 projection. A fake
/// implementation is suitable only for transport tests, never activation.
abstract interface class AgentSessionWireCodec {
  Uint8List encode(AgentSessionWireRequest request);
  AgentSessionResult decode(AgentSessionWireRequest request, Uint8List reply);
}

typedef AgentSessionExchange =
    Future<T> Function<T>(Uint8List frame, T Function(Uint8List reply) decode);

final class NativeAgentSessionClient implements AgentSessionControl {
  NativeAgentSessionClient(
    this._exchange,
    this._supported,
    this._codec, {
    List<int>? nonce,
  }) : _nonce = AgentSessionValidation.bytes(
         nonce ?? List.generate(24, (_) => Random.secure().nextInt(256)),
         24,
       ) {
    if (_nonce.length != 24)
      throw ArgumentError('Session nonce must have 24 bytes');
  }
  static const maxRequestBytes = 128 * 1024;
  static const maxReplyBytes = 384 * 1024;
  final AgentSessionExchange _exchange;
  final bool Function() _supported;
  final AgentSessionWireCodec _codec;
  final Uint8List _nonce;
  var _sequence = 0;
  bool _unknown = false, _busy = false;
  bool _readUncertain = false;
  BigInt _openedGeneration = BigInt.zero;
  _PendingSession? _pending;
  @override
  bool get supportsAgentSessions => _supported();
  @override
  bool get sessionOutcomeUnknown => _unknown;
  Uint8List _id() {
    // Unique correlation IDs remain available for drain/revoke after many
    // explicit observations. Only the host bounds effect-admission IDs.
    // Checked low-32 counter, no truncation/reset/automatic nonce rotation.
    if (_sequence >= 0xffffffff)
      throw const AgentSessionFailure(AgentSessionFailureKind.invalid);
    _sequence++;
    final value = Uint8List(32)..setRange(0, 24, _nonce);
    ByteData.sublistView(value).setUint32(24, _sequence, Endian.little);
    return value;
  }

  Future<AgentSessionResult> _call(
    AgentSessionOperation operation,
    Uint8List task,
    BigInt attachment, {
    BigInt? submission,
    BigInt? generation,
    BigInt? writerSlot,
    String sessionId = '',
    AgentSessionCommand? command,
  }) async {
    if (!supportsAgentSessions)
      throw const AgentSessionFailure(AgentSessionFailureKind.unavailable);
    if (_busy) throw const AgentSessionFailure(AgentSessionFailureKind.busy);
    final effect =
        operation == AgentSessionOperation.attach ||
        operation == AgentSessionOperation.open ||
        operation == AgentSessionOperation.submit ||
        operation == AgentSessionOperation.admitWriter;
    if (_unknown && effect)
      throw const AgentSessionFailure(AgentSessionFailureKind.unknown);
    if (effect && _pending != null)
      throw const AgentSessionFailure(AgentSessionFailureKind.busy);
    final pending = _pending;
    if (operation == AgentSessionOperation.read &&
        (pending == null ||
            _readUncertain ||
            pending.token != submission ||
            pending.attachment != attachment ||
            !AgentSessionValidation.same(pending.task, task))) {
      throw const AgentSessionFailure(AgentSessionFailureKind.unknown);
    }
    final request = AgentSessionWireRequest(
      operation: operation,
      id: _id(),
      taskKey: task,
      attachment: attachment,
      submission: submission,
      generation: generation,
      writerSlot: writerSlot,
      sessionId: sessionId,
      command: command,
      pendingCommand: operation == AgentSessionOperation.read
          ? pending!.command
          : null,
      pendingOperation: operation == AgentSessionOperation.read
          ? pending!.operation
          : null,
      expectedGeneration: operation == AgentSessionOperation.read
          ? pending!.generation
          : null,
      pendingRequestId: operation == AgentSessionOperation.read
          ? pending!.requestId
          : null,
    );
    final frame = _codec.encode(request);
    if (frame.isEmpty || frame.length > maxRequestBytes)
      throw const FormatException('Session frame limit');
    final previousUnknown = _unknown;
    _busy = true;
    if (operation == AgentSessionOperation.read) _readUncertain = true;
    if (effect) _unknown = true;
    try {
      final result = await _exchange(frame, (bytes) {
        if (bytes.isEmpty || bytes.length > maxReplyBytes)
          throw const FormatException('Session reply limit');
        return _codec.decode(request, bytes);
      });
      if (result.operation != operation ||
          !AgentSessionValidation.same(result.taskKey, task) ||
          operation != AgentSessionOperation.attach &&
              result.attachment != attachment) {
        throw const FormatException('Session control binding mismatch');
      }
      if (result.status == AgentSessionStatus.submitted) {
        if (!effect || result.submission == BigInt.zero || _pending != null) {
          throw const FormatException('Invalid pending session identity');
        }
        _pending = _PendingSession(
          operation,
          result.submission,
          attachment,
          task,
          _openedGeneration,
          request.id,
          operation == AgentSessionOperation.admitWriter
              ? AgentSessionAdmitWriter(sessionId)
              : command,
        );
        _readUncertain = false;
      } else if (operation == AgentSessionOperation.read) {
        if (result.submission != pending!.token)
          throw const FormatException('Read token changed');
        if (result.status == AgentSessionStatus.pending) {
          _readUncertain = false;
        } else if (result.status == AgentSessionStatus.opened ||
            result.status == AgentSessionStatus.reply ||
            result.status == AgentSessionStatus.writerAdmitted) {
          if (result.status == AgentSessionStatus.opened) {
            if (pending.operation != AgentSessionOperation.open ||
                result.endpointGeneration == BigInt.zero) {
              throw const FormatException('Invalid opening generation');
            }
            _openedGeneration = result.endpointGeneration;
          } else if (result.endpointGeneration != pending.generation) {
            throw const FormatException('Pending endpoint generation changed');
          }
          if (result.status == AgentSessionStatus.writerAdmitted &&
              (pending.operation != AgentSessionOperation.admitWriter ||
                  result.data is! AgentSessionWriterAdmitted ||
                  (result.data as AgentSessionWriterAdmitted).sessionId !=
                      (pending.command as AgentSessionAdmitWriter).sessionId)) {
            throw const FormatException('Writer admission context changed');
          }
          _pending =
              null; // Correlated terminal read consumes this context once.
        }
      }
      _unknown =
          previousUnknown ||
          result.deliveryUnknown ||
          operation == AgentSessionOperation.markLost ||
          result.status == AgentSessionStatus.unknown ||
          result.data is AgentSessionRejected &&
              (result.data as AgentSessionRejected).uncertain;
      return result;
    } catch (_) {
      _unknown =
          true; // Includes a missing/malformed receipt after Read or revoke.
      rethrow;
    } finally {
      _busy = false;
    }
  }

  @override
  Future<AgentSessionResult> attachSession(Uint8List key) =>
      _call(AgentSessionOperation.attach, key, BigInt.zero);
  @override
  Future<AgentSessionResult> openSession(Uint8List key, BigInt a) =>
      _call(AgentSessionOperation.open, key, a);
  @override
  Future<AgentSessionResult> submitSession(
    Uint8List key,
    BigInt a,
    BigInt generation,
    AgentSessionCommand command,
  ) => _call(
    AgentSessionOperation.submit,
    key,
    a,
    generation: generation,
    command: command,
  );
  @override
  Future<AgentSessionResult> sessionStatus(Uint8List key, BigInt a) =>
      _call(AgentSessionOperation.status, key, a);
  @override
  Future<AgentSessionResult> readSession(
    Uint8List key,
    BigInt a,
    BigInt token,
  ) => _call(AgentSessionOperation.read, key, a, submission: token);
  @override
  Future<AgentSessionResult> markSessionLost(Uint8List key, BigInt a) =>
      _call(AgentSessionOperation.markLost, key, a);
  @override
  Future<AgentSessionResult> detachSession(Uint8List key, BigInt a) =>
      _call(AgentSessionOperation.detach, key, a);
  @override
  Future<AgentSessionResult> admitSessionWriter(
    Uint8List key,
    BigInt a,
    String id,
  ) => _call(AgentSessionOperation.admitWriter, key, a, sessionId: id);
  @override
  Future<AgentSessionResult> closeSessionWriter(
    Uint8List key,
    BigInt a,
    BigInt slot,
  ) => _call(AgentSessionOperation.closeWriter, key, a, writerSlot: slot);
}

final class _PendingSession {
  _PendingSession(
    this.operation,
    this.token,
    this.attachment,
    List<int> task,
    this.generation,
    List<int> requestId,
    this.command,
  ) : task = AgentSessionValidation.bytes(task, 32, nonzero32: true),
      requestId = AgentSessionValidation.bytes(requestId, 32, nonzero32: true);
  final AgentSessionOperation operation;
  final BigInt token, attachment, generation;
  final Uint8List task;
  final Uint8List requestId;
  final AgentSessionCommand? command;
}
