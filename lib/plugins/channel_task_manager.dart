import 'dart:async';
import 'dart:convert';
import 'dart:math';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'channel_task_models.dart';
import 'channel_task_session.dart';
import 'plugin_library.dart';

/// Caller supplies finite original bytes. This UI reads no path or remote source.
final class ChannelTaskManager extends StatefulWidget {
  const ChannelTaskManager({
    super.key,
    required this.backend,
    required this.entry,
    required this.registryRevision,
  });
  final ChannelTaskBackend backend;
  final PluginLibraryEntry entry;
  final BigInt registryRevision;
  @override
  State<ChannelTaskManager> createState() => _ChannelTaskManagerState();
}

final class _ChannelTaskManagerState extends State<ChannelTaskManager> {
  late final session = ChannelTaskSession.forBackend(widget.backend);
  final source = TextEditingController();
  String? handler;
  ChannelSourceKind? kind;
  bool duplex = false;
  Object? inputFailure;
  Object? peerFailure;
  ChannelSentFrame? peerReceipt;
  List<PluginTransformHandler> get handlers => widget.entry.handlers
      .where(
        (h) =>
            widget.entry.channelHandlers.contains(h.name) &&
            h.inputType == 'morrow.channel.directory.v1',
      )
      .toList();
  @override
  void initState() {
    super.initState();
    session.addListener(_changed);
    handler = handlers.firstOrNull?.name;
    kind = widget.entry.channelKinds.firstOrNull;
  }

  void _changed() {
    if (mounted) {
      setState(() {});
    }
  }

  @override
  void dispose() {
    session.removeListener(_changed);
    source.dispose();
    // The backend-retained session and its key survive dismissal. Closing the
    // dialog never asserts release or starts a replacement invocation.
    super.dispose();
  }

  Uint8List _hex(String value) {
    if (value.length > 3 * 65536) {
      throw const FormatException('Source frame text exceeds limit');
    }
    final compact = value.replaceAll(RegExp(r'\s'), '');
    if (compact.length.isOdd || !RegExp(r'^[0-9a-fA-F]*$').hasMatch(compact)) {
      throw const FormatException('Enter complete hexadecimal bytes');
    }
    return Uint8List.fromList([
      for (var i = 0; i < compact.length; i += 2)
        int.parse(compact.substring(i, i + 2), radix: 16),
    ]);
  }

  Future<void> _run() async {
    try {
      if (source.text.length > 3 * 1024 * 1024) {
        throw const FormatException('Source text exceeds finite upload limit');
      }
      final selected = handlers.firstWhere((h) => h.name == handler);
      final declared = widget.entry.channelBudget!;
      final frames = <ChannelSourceFrame>[];
      for (final line
          in source.text.split('\n').where((s) => s.trim().isNotEmpty)) {
        if (frames.length >= 64) {
          throw const FormatException('At most 64 source frames');
        }
        final parts = line.split('|');
        if (parts.length > 2) {
          throw const FormatException(
            'Each line is bytes hex | optional cursor hex',
          );
        }
        frames.add(
          ChannelSourceFrame(
            sequence: BigInt.from(frames.length + 1),
            bytes: _hex(parts[0]),
            cursor: parts.length == 2 ? _hex(parts[1]) : const [],
          ),
        );
      }
      final duration = min(declared.maxDurationMs.toInt(), 30000);
      final budget = ChannelBudget(
        maxChannels: 1,
        maxFrameBytes: declared.maxFrameBytes,
        maxBytes: declared.maxBytes < BigInt.from(1024 * 1024)
            ? declared.maxBytes
            : BigInt.from(1024 * 1024),
        maxMessages: declared.maxMessages < BigInt.from(64)
            ? declared.maxMessages
            : BigInt.from(64),
        maxRequests: declared.maxRequests,
        maxDurationMs: BigInt.from(duration),
      );
      final random = Random.secure();
      final request = ChannelPrepareRequest(
        submission: List.generate(32, (_) => random.nextInt(256)),
        packageId: widget.entry.id,
        packageDigest: widget.entry.digest,
        registryRevision: widget.registryRevision,
        handler: selected.name,
        kind: kind!,
        duplex: duplex,
        budget: budget,
        lifetimeMs: duration,
        frameCount: frames.length,
        totalBytes: frames.fold(
          BigInt.zero,
          (sum, f) => sum + BigInt.from(f.bytes.length),
        ),
      );
      inputFailure = null;
      peerReceipt = null;
      peerFailure = null;
      await session.prepareAndRun(request, frames);
    } catch (error) {
      inputFailure = error;
    }
    if (mounted) {
      setState(() {});
    }
  }

  Future<void> _readSent() async {
    final generation = session.generation;
    final current = session.snapshot;
    if (current == null || current.observedSequence == BigInt.zero) {
      return;
    }
    try {
      final receipt = await widget.backend.readChannelSent(
        current.key,
        current.observedSequence,
      );
      if (!mounted ||
          generation != session.generation ||
          session.snapshot == null ||
          !ChannelValidation.same(current.key, session.snapshot!.key)) {
        return;
      }
      setState(() {
        peerReceipt = receipt;
        peerFailure = null;
      });
    } catch (error) {
      if (mounted &&
          generation == session.generation &&
          session.snapshot != null &&
          ChannelValidation.same(current.key, session.snapshot!.key)) {
        setState(() => peerFailure = error);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final current = session.snapshot;
    final activeHere = session.request?.packageId == widget.entry.id;
    return AlertDialog(
      title: Text('Local channels · ${widget.entry.name}'),
      content: SizedBox(
        width: 620,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              const Text(
                'Provide finite source frames as hexadecimal bytes. Each line is one frame; use | followed by optional opaque cursor hex. Maximum 64 frames and 1 MiB, subject to the package budget. Cursors are for Events; byte-stream cursors must be empty. A duplex peer requires no inbound frames.',
              ),
              if (handlers.isEmpty)
                const Text(
                  'This package has no handler declaring morrow.channel.directory.v1 input.',
                ),
              DropdownButton<String>(
                key: const ValueKey('channel-handler'),
                value: handler,
                items: [
                  for (final h in handlers)
                    DropdownMenuItem(value: h.name, child: Text(h.name)),
                ],
                onChanged: session.canPrepare
                    ? (value) => setState(() => handler = value)
                    : null,
              ),
              DropdownButton<ChannelSourceKind>(
                key: const ValueKey('channel-kind'),
                value: kind,
                items: [
                  for (final k in widget.entry.channelKinds)
                    DropdownMenuItem(value: k, child: Text(k.name)),
                ],
                onChanged: session.canPrepare
                    ? (value) => setState(() => kind = value)
                    : null,
              ),
              CheckboxListTile(
                title: const Text('Duplex peer (requires empty source)'),
                value: duplex,
                onChanged: session.canPrepare
                    ? (value) => setState(() => duplex = value ?? false)
                    : null,
              ),
              TextField(
                key: const ValueKey('channel-source-bytes'),
                controller: source,
                maxLines: 6,
                enabled: session.canPrepare,
                decoration: const InputDecoration(
                  labelText: 'Original bytes hex | cursor hex',
                ),
              ),
              if (inputFailure != null) Text(inputFailure.toString()),
              if (peerFailure != null)
                Text('Peer receipt unconfirmed: $peerFailure'),
              if (peerReceipt case final receipt?)
                SelectableText(
                  'Native peer original send ${receipt.sequence}: ${base64Encode(receipt.bytes)} (base64)',
                ),
              if (session.failure != null)
                Text('Unconfirmed: ${session.failure}'),
              if (session.uncertain)
                const Text(
                  'State is unconfirmed. Retain this original session and inspect or close it; no automatic replay occurs.',
                ),
              if (session.observedTaskResult case final received?)
                Text(
                  'Previously received task result: ${received.taskState.name}. Current delivery and cleanup are shown separately.',
                ),
              if (session.observedTaskResult case final received?
                  when received.output.isNotEmpty)
                SelectableText(
                  'Previously received ${received.outputType}: ${base64Encode(received.output)} (base64)',
                ),
              if (current != null) ...[
                Text(
                  '${activeHere ? "This package" : session.request?.packageId}: ${current.phase.name} / ${current.status.name}${current.snapshotPending ? " · snapshot pending" : ""}',
                ),
                Text(
                  'Uploaded ${current.uploadedFrames} frames / ${current.uploadedBytes} bytes; produced ${current.sourceFrames} frames / ${current.sourceBytes} bytes',
                ),
                Text(
                  'ACK ${current.lastAcked}; send accepted ${current.acceptedSequence}; native peer observed ${current.observedSequence} / ${current.observedBytes} bytes',
                ),
                Text(
                  'Task ${current.taskState.name}; producer ${current.producerOutcome.name}; cleanup ${current.cleanupProof.name}; resource reclaimed ${current.resourceReclaimed}; executor joined ${current.workerJoined}',
                ),
                if (current.taskError.isNotEmpty) Text(current.taskError),
                if (current.output.isNotEmpty)
                  SelectableText(
                    '${current.outputType}: ${base64Encode(current.output)} (base64)',
                  ),
              ],
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          key: const ValueKey('channel-read-sent'),
          onPressed: current == null || current.observedSequence == BigInt.zero
              ? null
              : _readSent,
          child: const Text('Read native peer bytes'),
        ),
        TextButton(
          key: const ValueKey('channel-refresh'),
          onPressed: current == null
              ? null
              : () => unawaited(session.refresh()),
          child: const Text('Inspect'),
        ),
        TextButton(
          key: const ValueKey('channel-close'),
          onPressed: current == null ? null : () => unawaited(session.close()),
          child: const Text('Close channel'),
        ),
        FilledButton(
          key: const ValueKey('channel-run'),
          onPressed:
              session.canPrepare &&
                  handler != null &&
                  kind != null &&
                  widget.backend.supportsLocalChannels
              ? _run
              : null,
          child: Text(session.busy ? 'Preparing…' : 'Prepare and run once'),
        ),
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Dismiss'),
        ),
      ],
    );
  }
}
