import 'package:morrow_studio/plugins/agent_wrapper_models.dart';

AgentWrapperRevisions revisions({BigInt? catalog, BigInt? manager}) =>
    AgentWrapperRevisions(
      catalog: catalog ?? BigInt.one,
      manager: manager ?? BigInt.one,
    );
AgentWrapperReview review({int seed = 1}) => AgentWrapperReview(
  id: 'org.example.agent',
  version: '1.0.0',
  wrapperSha256: List.filled(32, seed),
  baseSha256: List.filled(32, 7),
  sessionSchema: List.filled(32, 8),
  processSchema: List.filled(32, 9),
  declaration: AgentWrapperApproval(
    sessionBits: 3,
    processBits: 3,
    sessions: ['main'],
    executionDomain: 'synthetic:test',
  ),
);
AgentWrapperEntry entry({
  AgentWrapperReview? package,
  AgentWrapperApproval? approved,
  bool enabled = false,
}) => AgentWrapperEntry(
  review: package ?? review(),
  selected: true,
  enabled: enabled,
  baseSelected: true,
  baseEnabled: true,
  approved: approved,
);
