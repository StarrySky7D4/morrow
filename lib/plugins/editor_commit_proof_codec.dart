import 'editor_commit_proof.dart';
import 'editor_draft_codec.dart';
import 'generated/host.capnp.dart' as host;
import 'versioned_content_codec.dart';

abstract final class EditorCommitProofCodec {
  static void validateIdentity(String id, String operation) =>
      EditorDraftCodec.validateHandoffProposalIdentity(
        id,
        'commit-proof',
        operation,
      );

  /// The caller has already checked transport version, digest and host error.
  /// Copy all pointer-backed values before releasing the response frame.
  static EditorDraftCommitEvidence decode(
    host.ResponseReader response, {
    required String id,
    required String operation,
  }) {
    validateIdentity(id, operation);
    final value = response.editorCommitProof;
    if (value == null ||
        value.id != id ||
        value.operation != operation ||
        value.digest == null ||
        value.digest!.length != 32) {
      throw const FormatException('Editor commit proof identity changed');
    }
    final source = value.sourceRevisionBigInt;
    final committed = value.committedRevisionBigInt;
    if (source == VersionedContentCodec.maxU64 ||
        committed != source + BigInt.one ||
        response.revisionBigInt != committed ||
        (response.payload?.isNotEmpty ?? false)) {
      throw const FormatException('Editor commit proof revision changed');
    }
    return EditorDraftCommitEvidence(
      id: id,
      operation: operation,
      digest: value.digest!,
      sourceRevision: source,
      committedRevision: committed,
    );
  }
}
