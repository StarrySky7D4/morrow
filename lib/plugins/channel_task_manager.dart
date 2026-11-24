import 'dart:async';
import 'dart:convert';
import 'dart:math';
import 'dart:typed_data';

import 'package:material_ui/material_ui.dart';

import 'channel_task_models.dart';
import 'channel_task_session.dart';
import 'plugin_library.dart';

/// Starts a finite caller-provided source or an explicitly reviewed card history.
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
  final resume = TextEditingController();
  final pageSize = TextEditingController(text: '16');
  int get maxPageSize =>
      min(64, widget.entry.channelBudget?.maxMessages.toInt() ?? 1);
  bool get changes =>
      widget.entry.changesMetadataSupported && handler == 'changes.metadata';
  String? handler;
  ChannelSourceKind? kind;
  bool duplex = false;
  bool confirmingBudget = false;
  Object? inputFailure;
  Object? peerFailure;
  ChannelSentFrame? peerReceipt;
  late int receiptGeneration;
  List<PluginTransformHandler> get handlers =>
      widget.entry.directoryChannelHandlers;
  @override
  void initState() {
    super.initState();
    receiptGeneration = session.generation;
    session.addListener(_changed);
    handler = handlers.firstOrNull?.name;
    kind = widget.entry.channelKinds.firstOrNull;
    pageSize.text = min(16, maxPageSize).toString();
  }

  void _changed() {
    if (mounted) {
      setState(() {
        if (receiptGeneration != session.generation) {
          receiptGeneration = session.generation;
          peerReceipt = null;
          peerFailure = null;
        }
      });
    }
  }

  @override
  void dispose() {
    session.removeListener(_changed);
    source.dispose();
    resume.dispose();
    pageSize.dispose();
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

  bool get _canRun =>
      !confirmingBudget &&
      session.canPrepare &&
      widget.entry.hasEligibleDirectoryChannels &&
      handler != null &&
      kind != null &&
      widget.backend.supportsLocalChannels;

  Future<bool> _approveRun(ChannelPrepareRequest request) async {
    final generation = session.generation;
    setState(() => confirmingBudget = true);
    try {
      final confirmed = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
          title: Text(
            request.changes == null
                ? 'Approve compatibility budget'
                : 'Approve card changes',
          ),
          content: SingleChildScrollView(
            child: Text(
              [
                if (request.changes case final review?) ...[
                  'Allow ${request.packageId} to read up to ${review.pageSize} change records for these cards once.',
                  review.cards.join('\n'),
                  review.previousCursor == null
                      ? 'Start at the beginning.'
                      : 'Resume after the last acknowledged record.',
                  'Each record includes the card ID, operation ID, revision and content digest.',
                ],
                if (request.compatibilityBudget)
                  'Run ${request.handler} from ${request.packageId} once with up to '
                      '100 million fuel units and 256 host calls. The package’s limits also apply.',
              ].join('\n\n'),
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: const Text('Cancel'),
            ),
            FilledButton(
              key: const ValueKey('channel-approve-compatibility-budget'),
              onPressed: () => Navigator.pop(context, true),
              child: const Text('Approve once'),
            ),
          ],
        ),
      );
      return confirmed == true &&
          mounted &&
          session.generation == generation &&
          session.canPrepare &&
          widget.registryRevision == request.registryRevision &&
          widget.entry.id == request.packageId &&
          ChannelValidation.same(widget.entry.digest, request.packageDigest);
    } finally {
      if (mounted) {
        setState(() => confirmingBudget = false);
      }
    }
  }

  Future<void> _run({bool compatibilityBudget = false}) async {
    if (!_canRun) return;
    try {
      if (source.text.length > 3 * 1024 * 1024) {
        throw const FormatException('Source text exceeds finite upload limit');
      }
      final selected = handlers.firstWhere((h) => h.name == handler);
      final declared = widget.entry.channelBudget!;
      final frames = <ChannelSourceFrame>[];
      final lines = source.text.split('\n').where((s) => s.trim().isNotEmpty);
      final review = changes
          ? ChangesWindowReview(
              cards: lines.map((s) => s.trim()).toList(),
              pageSize: int.parse(pageSize.text.trim()),
              previousCursor: resume.text.trim().isEmpty
                  ? null
                  : _hex(resume.text),
            )
          : null;
      for (final line in changes ? const <String>[] : lines) {
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
        kind: changes ? ChannelSourceKind.events : kind!,
        duplex: changes ? false : duplex,
        budget: budget,
        lifetimeMs: duration,
        frameCount: frames.length,
        totalBytes: frames.fold(
          BigInt.zero,
          (sum, f) => sum + BigInt.from(f.bytes.length),
        ),
        compatibilityBudget: compatibilityBudget,
        changes: review,
      );
      if ((compatibilityBudget || review != null) &&
          !await _approveRun(request)) {
        return;
      }
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
              if (!changes)
                const Text(
                  'Provide finite source frames as hexadecimal bytes. Each line is one frame; use | followed by optional opaque cursor hex. Maximum 64 frames and 1 MiB, subject to the package budget. Cursors are for Events; byte-stream cursors must be empty. A duplex peer requires no inbound frames.',
                ),
              if (handlers.isEmpty)
                const Text(
                  'This package is not eligible for a bounded local Directory invocation.',
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
              if (!changes)
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
              if (!changes)
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
                decoration: InputDecoration(
                  labelText: changes
                      ? 'Card IDs, one per line'
                      : 'Original bytes hex | cursor hex',
                ),
              ),
              if (changes) ...[
                TextField(
                  controller: pageSize,
                  enabled: session.canPrepare,
                  decoration: InputDecoration(
                    labelText: 'Records per approval (1–$maxPageSize)',
                  ),
                ),
                TextField(
                  controller: resume,
                  enabled: session.canPrepare,
                  key: const ValueKey('channel-changes-resume'),
                  decoration: const InputDecoration(
                    labelText: 'Resume cursor (optional)',
                  ),
                ),
              ],
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
                if (activeHere &&
                    session.request?.changes != null &&
                    current.changesCursor.isNotEmpty)
                  SelectableText(
                    'Resume cursor\n${current.changesCursor.map((b) => b.toRadixString(16).padLeft(2, '0')).join()}',
                  ),
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
        TextButton(
          key: const ValueKey('channel-run-compatibility-budget'),
          onPressed: _canRun
              ? () => unawaited(_run(compatibilityBudget: true))
              : null,
          child: const Text('Run with compatibility budget…'),
        ),
        FilledButton(
          key: const ValueKey('channel-run'),
          onPressed: _canRun ? _run : null,
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
