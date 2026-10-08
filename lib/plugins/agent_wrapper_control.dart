import 'agent_wrapper_models.dart';

abstract interface class AgentWrapperControl {
  bool get supportsAgentWrappers;
  bool get wrapperOutcomeUnknown;
  Future<AgentWrapperResult> wrapperState();
  Future<AgentWrapperResult> wrapperPage(
    AgentWrapperRevisions revisions, {
    String cursor = '',
  });
  Future<AgentWrapperResult> inspectWrapper(
    String path,
    AgentWrapperRevisions revisions,
  );
  Future<AgentWrapperResult> installWrapper(
    String path,
    AgentWrapperReview review,
    AgentWrapperRevisions revisions,
  );
  Future<AgentWrapperResult> selectWrapperBase(
    AgentWrapperReview review,
    AgentWrapperRevisions revisions,
  );
  Future<AgentWrapperResult> enableWrapperBase(
    AgentWrapperReview review,
    AgentWrapperRevisions revisions,
    bool enabled,
  );
  Future<AgentWrapperResult> selectWrapper(
    AgentWrapperReview review,
    AgentWrapperRevisions revisions,
  );
  Future<AgentWrapperResult> approveWrapper(
    AgentWrapperReview review,
    AgentWrapperRevisions revisions,
    AgentWrapperApproval approval,
  );
  Future<AgentWrapperResult> enableWrapper(
    AgentWrapperReview review,
    AgentWrapperRevisions revisions,
    bool enabled,
  );
  Future<AgentWrapperResult> removeWrapper(
    AgentWrapperReview review,
    AgentWrapperRevisions revisions,
  );
}

enum AgentWrapperFailureKind {
  busy,
  lost,
  unknown,
  conflict,
  denied,
  unavailable,
  limit,
  invalid,
}

final class AgentWrapperFailure implements Exception {
  const AgentWrapperFailure(this.kind, [this.message = '']);
  final AgentWrapperFailureKind kind;
  final String message;
  @override
  String toString() => message.isEmpty ? kind.name : message;
}
