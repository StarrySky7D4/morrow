import 'dart:typed_data';
import 'agent_session_models.dart';

enum AgentSessionOperation {
  attach,
  open,
  submit,
  status,
  read,
  markLost,
  detach,
  admitWriter,
  closeWriter,
}

enum AgentSessionStatus {
  unsupported,
  unavailable,
  busy,
  invalid,
  unknown,
  limit,
  attached,
  submitted,
  pending,
  opened,
  reply,
  observed,
  detached,
  writerAdmitted,
}

final class AgentSessionResult {
  AgentSessionResult({
    required this.operation,
    required this.status,
    required List<int> taskKey,
    required BigInt submission,
    required BigInt attachment,
    required BigInt endpointGeneration,
    required this.leaseLive,
    required this.deliveryUnknown,
    this.data,
  }) : taskKey = AgentSessionValidation.bytes(taskKey, 32, nonzero32: true),
       submission = AgentSessionValidation.u64(submission),
       attachment = AgentSessionValidation.u64(attachment),
       endpointGeneration = AgentSessionValidation.u64(endpointGeneration);
  final AgentSessionOperation operation;
  final AgentSessionStatus status;
  final Uint8List taskKey;
  final BigInt submission, attachment, endpointGeneration;
  final bool leaseLive, deliveryUnknown;
  final AgentSessionData? data;
}

/// One already running exact task on the original trusted host. Implementations
/// return only correlated, canonical replies. Short submissions/status/read must
/// not await an entire session workflow in the shared native pipe slot.
abstract interface class AgentSessionControl {
  bool get supportsAgentSessions;
  bool get sessionOutcomeUnknown;
  Future<AgentSessionResult> attachSession(Uint8List taskKey);
  Future<AgentSessionResult> openSession(Uint8List taskKey, BigInt attachment);
  Future<AgentSessionResult> submitSession(
    Uint8List taskKey,
    BigInt attachment,
    BigInt generation,
    AgentSessionCommand command,
  );
  Future<AgentSessionResult> sessionStatus(
    Uint8List taskKey,
    BigInt attachment,
  );
  Future<AgentSessionResult> readSession(
    Uint8List taskKey,
    BigInt attachment,
    BigInt submission,
  );
  Future<AgentSessionResult> markSessionLost(
    Uint8List taskKey,
    BigInt attachment,
  );
  Future<AgentSessionResult> detachSession(
    Uint8List taskKey,
    BigInt attachment,
  );
  Future<AgentSessionResult> admitSessionWriter(
    Uint8List taskKey,
    BigInt attachment,
    String sessionId,
  );
  Future<AgentSessionResult> closeSessionWriter(
    Uint8List taskKey,
    BigInt attachment,
    BigInt writerSlot,
  );
}

enum AgentSessionFailureKind {
  busy,
  unknown,
  staleBackend,
  unavailable,
  invalid,
  disposed,
}

final class AgentSessionFailure implements Exception {
  const AgentSessionFailure(this.kind);
  final AgentSessionFailureKind kind;
  @override
  String toString() => 'Agent session ${kind.name}';
}
