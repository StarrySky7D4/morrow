import 'dart:convert';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'generated/host.capnp.dart' as host;
import 'generated/channel.capnp.dart' as wire;
import 'generated/identity.dart' as contract;
import 'channel_task_models.dart';

abstract final class ChannelTaskCodec {
  static T _enum<T>(List<T> values, int code) {
    if (code < 0 || code >= values.length) {
      throw const FormatException('Unknown channel state');
    }
    return values[code];
  }

  static void _text(String value) {
    if (utf8.encode(value).length > 256 || value.contains('\u0000')) {
      throw const FormatException('Channel text exceeds limit');
    }
  }

  static ChannelBudget hostBudget(host.ChannelBudgetReader row) =>
      ChannelBudget(
        maxChannels: row.maxChannels,
        maxFrameBytes: row.maxFrameBytes,
        maxBytes: row.maxBytesBigInt,
        maxMessages: row.maxMessagesBigInt,
        maxRequests: row.maxRequestsBigInt,
        maxDurationMs: row.maxDurationMsBigInt,
      );
  static void writeBudget(ChannelBudget b, host.ChannelBudgetBuilder row) {
    row.maxChannels = b.maxChannels;
    row.maxFrameBytes = b.maxFrameBytes;
    row.maxBytesBigInt = b.maxBytes;
    row.maxMessagesBigInt = b.maxMessages;
    row.maxRequestsBigInt = b.maxRequests;
    row.maxDurationMsBigInt = b.maxDurationMs;
  }

  static void writePrepare(
    ChannelPrepareRequest value,
    host.ChannelPrepareBuilder row,
  ) {
    _text(value.packageId);
    _text(value.handler);
    row.submission = value.submission;
    row.packageId = value.packageId;
    row.packageDigest = value.packageDigest;
    row.registryRevisionBigInt = value.registryRevision;
    row.handler = value.handler;
    row.kind = value.kind.index + 1;
    row.duplex = value.duplex;
    writeBudget(value.budget, row.initBudget());
    row.lifetimeMs = value.lifetimeMs;
    row.frameCount = value.frameCount;
    row.totalBytesBigInt = value.totalBytes;
  }

  static ChannelDirectory directory(Uint8List bytes) {
    if (bytes.length < 8 || bytes.length > 128 * 1024) {
      throw const FormatException('Invalid channel directory size');
    }
    final header = ByteData.sublistView(bytes);
    if (header.getUint32(0, Endian.little) != 0 ||
        header.getUint32(4, Endian.little) * 8 + 8 != bytes.length) {
      throw const FormatException(
        'Channel directory requires exact single-segment framing',
      );
    }
    final message = MessageReader.deserialize(
      bytes,
      MessageReaderOptions(
        traversalLimitInWords: 128 * 1024 ~/ 8,
        nestingLimit: 12,
        maxSegments: 1,
      ),
    );
    final root = message.getRoot(wire.directoryFactory);
    if (root.version != 1 ||
        !ChannelValidation.same(
          root.schemaSha256 ?? [],
          contract.channelDigest,
        )) {
      throw const FormatException(
        'Channel directory version or schema mismatch',
      );
    }
    final entries = root.channels;
    if (entries == null || entries.isEmpty || entries.length > 8) {
      throw const FormatException('Invalid channel directory endpoints');
    }
    final channels = <ChannelEndpoint>[];
    final seen = <String>{};
    for (final e in entries) {
      final budget = e.budget;
      if (budget == null || e.kind == null) {
        throw const FormatException('Missing channel endpoint metadata');
      }
      final endpoint = ChannelEndpoint(
        reference: e.reference ?? [],
        sourceEpoch: e.sourceEpoch ?? [],
        kind: _enum(ChannelSourceKind.values, e.kind!.index),
        budget: ChannelBudget(
          maxChannels: budget.maxChannels,
          maxFrameBytes: budget.maxFrameBytes,
          maxBytes: budget.maxBytesBigInt,
          maxMessages: budget.maxMessagesBigInt,
          maxRequests: budget.maxRequestsBigInt,
          maxDurationMs: budget.maxDurationMsBigInt,
        ),
      );
      if (!seen.add(base64Encode(endpoint.reference))) {
        throw const FormatException('Duplicate channel reference');
      }
      channels.add(endpoint);
    }
    final result = ChannelDirectory(
      scopeSha256: root.scopeSha256 ?? [],
      wire: bytes,
      channels: channels,
    );
    // Re-encode every known field in the public contract's exact allocation order.
    // Equality rejects unknown fields, alternate pointers, padding and trailing data.
    final builder = MessageBuilder();
    final out = builder.initRoot(wire.directoryFactory);
    out.version = 1;
    out.schemaSha256 = Uint8List.fromList(contract.channelDigest);
    out.scopeSha256 = result.scopeSha256;
    final items = out.initChannels(channels.length);
    for (var i = 0; i < channels.length; i++) {
      final e = channels[i];
      final item = items[i];
      item.reference = e.reference;
      item.sourceEpoch = e.sourceEpoch;
      item.kind = wire.Kind.values[e.kind.index];
      final b = item.initBudget();
      b.maxChannels = e.budget.maxChannels;
      b.maxFrameBytes = e.budget.maxFrameBytes;
      b.maxBytesBigInt = e.budget.maxBytes;
      b.maxMessagesBigInt = e.budget.maxMessages;
      b.maxRequestsBigInt = e.budget.maxRequests;
      b.maxDurationMsBigInt = e.budget.maxDurationMs;
    }
    if (!ChannelValidation.same(builder.serialize(), bytes)) {
      throw const FormatException('Noncanonical channel directory');
    }
    return result;
  }

  static ChannelTaskSnapshot snapshot(host.ChannelStateReader? row) {
    if (row == null) {
      throw const FormatException('Missing local channel state');
    }
    final dir = directory(Uint8List.fromList(row.directory ?? []));
    final reference = ChannelValidation.identity(row.reference ?? []);
    final epoch = ChannelValidation.identity(row.sourceEpoch ?? []);
    if (dir.channels.length != 1 ||
        !ChannelValidation.same(dir.channels.single.reference, reference) ||
        !ChannelValidation.same(dir.channels.single.sourceEpoch, epoch)) {
      throw const FormatException('Channel directory binding changed');
    }
    final phase = _enum(ChannelJobPhase.values, row.phase);
    final status = _enum(ChannelStatus.values, row.status);
    final proof = _enum(ChannelCleanupProof.values, row.cleanupProof);
    final taskState = _enum(ChannelTaskState.values, row.taskState);
    final output = row.output ?? Uint8List(0);
    final inputSha = row.inputSha256 ?? Uint8List(0);
    final observedSha = row.observedSha256 ?? Uint8List(0);
    final error = row.taskError ?? '';
    final outputType = row.outputType ?? '';
    _text(error);
    _text(outputType);
    if (output.length > 65536 ||
        (inputSha.isNotEmpty && inputSha.length != 32) ||
        (observedSha.isNotEmpty && observedSha.length != 32) ||
        row.uploadedFrames > 64 ||
        row.sourceFrames > 64 ||
        row.uploadedBytesBigInt > BigInt.from(1024 * 1024) ||
        row.sourceBytesBigInt > BigInt.from(1024 * 1024) ||
        row.observedBytesBigInt > BigInt.from(1024 * 1024) ||
        row.observedSequenceBigInt > row.acceptedSequenceBigInt ||
        (row.resourceReclaimed && proof == ChannelCleanupProof.pending) ||
        (!row.resourceReclaimed && proof != ChannelCleanupProof.pending) ||
        (phase == ChannelJobPhase.completed &&
            taskState == ChannelTaskState.pending) ||
        (taskState == ChannelTaskState.success &&
            (inputSha.length != 32 ||
                outputType.isEmpty ||
                error.isNotEmpty))) {
      throw const FormatException('Inconsistent bounded local channel state');
    }
    return ChannelTaskSnapshot(
      key: row.key ?? [],
      submission: row.submission ?? [],
      directory: dir,
      reference: reference,
      sourceEpoch: epoch,
      phase: phase,
      status: status,
      lastAcked: row.lastAckedBigInt,
      acceptedSequence: row.acceptedSequenceBigInt,
      observedSequence: row.observedSequenceBigInt,
      cleanupProof: proof,
      producerOutcome: _enum(
        ChannelProducerOutcome.values,
        row.producerOutcome,
      ),
      taskState: taskState,
      taskError: error,
      outputType: outputType,
      output: output,
      inputSha256: inputSha,
      uploadedFrames: row.uploadedFrames,
      uploadedBytes: row.uploadedBytesBigInt,
      sourceFrames: row.sourceFrames,
      sourceBytes: row.sourceBytesBigInt,
      resourceReclaimed: row.resourceReclaimed,
      closeRequested: row.closeRequested,
      observedBytes: row.observedBytesBigInt,
      observedSha256: observedSha,
      workerJoined: row.workerJoined,
      snapshotPending: row.snapshotPending,
    );
  }
}
