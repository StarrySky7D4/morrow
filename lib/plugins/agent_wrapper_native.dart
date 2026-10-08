import 'dart:math';
import 'dart:typed_data';
import 'agent_wrapper_models.dart';
import 'agent_wrapper_control.dart';
import 'agent_wrapper_codec_native.dart';

typedef AgentWrapperExchange =
    Future<T> Function<T>(Uint8List frame, T Function(Uint8List reply) decode);

/// One fixed profile on the original ordered host channel. Never replays a write.
final class NativeAgentWrapperClient implements AgentWrapperControl {
  NativeAgentWrapperClient(this._exchange, this._supported, {List<int>? nonce})
    : _nonce = Uint8List.fromList(
        nonce ?? List.generate(8, (_) => Random.secure().nextInt(256)),
      ) {
    if (_nonce.length != 8) {
      throw ArgumentError('Catalog nonce must have 8 bytes');
    }
    if (_nonce.every((v) => v == 0)) _nonce[0] = 1;
  }
  final AgentWrapperExchange _exchange;
  final bool Function() _supported;
  final Uint8List _nonce;
  BigInt _sequence = BigInt.zero;
  bool _unknown = false, _busy = false;
  @override
  bool get supportsAgentWrappers => _supported();
  @override
  bool get wrapperOutcomeUnknown => _unknown;
  Uint8List _id() {
    _sequence += BigInt.one;
    AgentWrapperValidation.u64(_sequence);
    final result = Uint8List(16)..setRange(0, 8, _nonce);
    final out = ByteData.sublistView(result);
    out.setUint32(
      8,
      (_sequence & BigInt.from(0xffffffff)).toInt(),
      Endian.little,
    );
    out.setUint32(12, (_sequence >> 32).toInt(), Endian.little);
    return result;
  }

  Future<AgentWrapperResult> _call(
    AgentWrapperAction action, {
    AgentWrapperRevisions? revisions,
    String path = '',
    AgentWrapperReview? review,
    bool enabled = false,
    AgentWrapperApproval? approval,
    String cursor = '',
  }) async {
    if (!supportsAgentWrappers) {
      throw const AgentWrapperFailure(AgentWrapperFailureKind.unavailable);
    }
    if (_busy) throw const AgentWrapperFailure(AgentWrapperFailureKind.busy);
    final request = AgentWrapperRequest(
      action: action,
      id: _id(),
      revisions: revisions,
      path: path,
      packageId:
          action == AgentWrapperAction.baseEnable ||
              action == AgentWrapperAction.approve ||
              action == AgentWrapperAction.wrapperEnable ||
              action == AgentWrapperAction.remove
          ? review!.id
          : '',
      fullSha256: review?.wrapperSha256 ?? [],
      enabled: enabled,
      approval: approval,
      cursor: cursor,
      limit: action == AgentWrapperAction.page ? 16 : 0,
    );
    if (_unknown && request.mutates) {
      throw const AgentWrapperFailure(AgentWrapperFailureKind.unknown);
    }
    if (approval != null &&
        (review == null || !approval.subsetOf(review.declaration))) {
      throw const FormatException('Approval exceeds wrapper declaration');
    }
    final frame = AgentWrapperCodec.encodeRequest(request);
    _busy = true;
    // Set before transport, including exceptions or a lost receipt after an effect.
    if (request.mutates) _unknown = true;
    try {
      final reply = await _exchange(
        frame,
        (bytes) => AgentWrapperCodec.decodeReply(request, bytes),
      );
      if (reply.status == AgentWrapperStatus.ok) {
        if (request.mutates) _unknown = false;
        return reply.result;
      }
      final unsafeToContinue =
          reply.status == AgentWrapperStatus.unknown ||
          reply.status == AgentWrapperStatus.storage ||
          reply.status == AgentWrapperStatus.recoveryRequired ||
          reply.status == AgentWrapperStatus.ownerUnavailable;
      if (unsafeToContinue) {
        _unknown = true;
      } else if (request.mutates) {
        _unknown =
            false; // Correlated definite rejection proves this request had no accepted effect.
      }
      throw AgentWrapperFailure(switch (reply.status) {
        AgentWrapperStatus.busy => AgentWrapperFailureKind.busy,
        AgentWrapperStatus.unknown ||
        AgentWrapperStatus.storage => AgentWrapperFailureKind.unknown,
        AgentWrapperStatus.recoveryRequired ||
        AgentWrapperStatus.ownerUnavailable => AgentWrapperFailureKind.lost,
        AgentWrapperStatus.conflict => AgentWrapperFailureKind.conflict,
        AgentWrapperStatus.denied => AgentWrapperFailureKind.denied,
        AgentWrapperStatus.limit => AgentWrapperFailureKind.limit,
        AgentWrapperStatus.unsupported => AgentWrapperFailureKind.unavailable,
        _ => AgentWrapperFailureKind.invalid,
      });
    } catch (error) {
      if (error is! AgentWrapperFailure) _unknown = true;
      rethrow;
    } finally {
      _busy = false;
    }
  }

  @override
  Future<AgentWrapperResult> wrapperState() => _call(AgentWrapperAction.state);
  @override
  Future<AgentWrapperResult> wrapperPage(
    AgentWrapperRevisions revisions, {
    String cursor = '',
  }) => _call(AgentWrapperAction.page, revisions: revisions, cursor: cursor);
  @override
  Future<AgentWrapperResult> inspectWrapper(
    String path,
    AgentWrapperRevisions revisions,
  ) => _call(AgentWrapperAction.inspect, path: path);
  @override
  Future<AgentWrapperResult> installWrapper(
    String path,
    AgentWrapperReview review,
    AgentWrapperRevisions revisions,
  ) => _call(
    AgentWrapperAction.install,
    path: path,
    review: review,
    revisions: revisions,
  );
  @override
  Future<AgentWrapperResult> selectWrapperBase(
    AgentWrapperReview review,
    AgentWrapperRevisions revisions,
  ) => _call(
    AgentWrapperAction.baseSelect,
    review: review,
    revisions: revisions,
  );
  @override
  Future<AgentWrapperResult> enableWrapperBase(
    AgentWrapperReview review,
    AgentWrapperRevisions revisions,
    bool enabled,
  ) => _call(
    AgentWrapperAction.baseEnable,
    review: review,
    revisions: revisions,
    enabled: enabled,
  );
  @override
  Future<AgentWrapperResult> selectWrapper(
    AgentWrapperReview review,
    AgentWrapperRevisions revisions,
  ) => _call(
    AgentWrapperAction.wrapperSelect,
    review: review,
    revisions: revisions,
  );
  @override
  Future<AgentWrapperResult> approveWrapper(
    AgentWrapperReview review,
    AgentWrapperRevisions revisions,
    AgentWrapperApproval approval,
  ) => _call(
    AgentWrapperAction.approve,
    review: review,
    revisions: revisions,
    approval: approval,
  );
  @override
  Future<AgentWrapperResult> enableWrapper(
    AgentWrapperReview review,
    AgentWrapperRevisions revisions,
    bool enabled,
  ) => _call(
    AgentWrapperAction.wrapperEnable,
    review: review,
    revisions: revisions,
    enabled: enabled,
  );
  @override
  Future<AgentWrapperResult> removeWrapper(
    AgentWrapperReview review,
    AgentWrapperRevisions revisions,
  ) => _call(AgentWrapperAction.remove, review: review, revisions: revisions);
}
