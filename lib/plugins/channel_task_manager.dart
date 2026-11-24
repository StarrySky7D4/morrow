import 'dart:async';
import 'dart:convert';
import 'dart:math';
import 'dart:typed_data';

import 'package:material_ui/material_ui.dart';

import 'channel_task_models.dart';
import 'card_changes_watch.dart';
import 'channel_task_session.dart';
import 'changes_page_wait.dart';
import 'changes_watch_controls.dart';
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
  late ChannelTaskSession session;
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
  ReviewedChangesProgress? checkpoint;
  int lookupGeneration = 0;
  bool checkingCheckpoint = false;
  bool waitForChanges = false;
  ChangesPageWait? watcher;
  Object? peerFailure;
  ChannelSentFrame? peerReceipt;
  late int receiptGeneration;
  List<PluginTransformHandler> get handlers =>
      widget.entry.directoryChannelHandlers;
  @override
  void initState() {
    super.initState();
    session = ChannelTaskSession.forBackend(widget.backend);
    receiptGeneration = session.generation;
    session.addListener(_changed);
    handler = handlers.firstOrNull?.name;
    kind = widget.entry.channelKinds.firstOrNull;
    pageSize.text = min(16, maxPageSize).toString();
    if (widget.backend case final ChangesObservationBackend backend) {
      final watch = CardChangesWatch.forBackend(backend);
      if (watch.active &&
          watch.matches(
            widget.entry.id,
            widget.entry.digest,
            widget.registryRevision,
          )) {
        source.text = watch.selection!.cards.join('\n');
      }
    }
  }

  void _changed() {
    if (mounted) {
      setState(() {
        if (receiptGeneration != session.generation) {
          receiptGeneration = session.generation;
          clearCheckpoint();
          peerReceipt = null;
          peerFailure = null;
        }
      });
    }
  }

  @override
  void didUpdateWidget(covariant ChannelTaskManager oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.backend, widget.backend)) {
      session.removeListener(_changed);
      session = ChannelTaskSession.forBackend(widget.backend);
      receiptGeneration = session.generation;
      session.addListener(_changed);
      peerReceipt = null;
      peerFailure = null;
    }
    if (!identical(oldWidget.backend, widget.backend) ||
        oldWidget.entry.id != widget.entry.id ||
        oldWidget.registryRevision != widget.registryRevision ||
        !ChannelValidation.same(oldWidget.entry.digest, widget.entry.digest)) {
      clearCheckpoint();
      watcher?.removeListener(_changed);
      watcher?.dispose();
      watcher = null;
    }
  }

  void clearCheckpoint() {
    lookupGeneration++;
    checkpoint = null;
    checkingCheckpoint = false;
  }

  ChangesWindowReview reviewedCards(int count) => ChangesWindowReview(
    cards: source.text
        .split('\n')
        .map((s) => s.trim())
        .where((s) => s.isNotEmpty)
        .toList(),
    pageSize: count,
    previousCursor: resume.text.trim().isEmpty ? null : _hex(resume.text),
  );
  Future<void> inspectCheckpoint(ChangesCheckpointBackend backend) async {
    final generation = ++lookupGeneration;
    setState(() {
      checkingCheckpoint = true;
      checkpoint = null;
      inputFailure = null;
    });
    try {
      final result = await backend.inspectChangesCheckpoint(
        widget.entry.id,
        widget.entry.digest,
        widget.registryRevision,
        reviewedCards(1),
      );
      if (mounted && generation == lookupGeneration)
        setState(() => checkpoint = result);
    } catch (error) {
      if (mounted && generation == lookupGeneration)
        setState(() => inputFailure = error);
    } finally {
      if (mounted && generation == lookupGeneration)
        setState(() => checkingCheckpoint = false);
    }
  }

  String get checkpointMessage {
    final progress = checkpoint!;
    final zh = Localizations.localeOf(context).languageCode == 'zh';
    final text = switch (progress.bookmark) {
      ChangesBookmarkState.current =>
        zh ? '输入的游标是当前已确认的位置。' : 'The entered cursor is current.',
      ChangesBookmarkState.retained =>
        zh
            ? '这条回执仍可核对，资料库已有更近的确认位置。'
            : 'This receipt is retained. A newer position is confirmed.',
      ChangesBookmarkState.unavailable =>
        zh
            ? '此卡片集合无法核实输入的游标。'
            : 'This selection has no retained receipt for the entered cursor.',
      ChangesBookmarkState.notSupplied =>
        progress.latest == null
            ? (zh
                  ? '此卡片集合尚无已确认的游标。'
                  : 'No cursor has been confirmed for these cards.')
            : (zh
                  ? '已查到此卡片集合的确认位置。'
                  : 'A confirmed cursor is available for these cards.'),
    };
    return '$text ' +
        (progress.pending
            ? (zh ? '有待读取的变更。' : 'Changes are available to read.')
            : (zh ? '截至本次核对已读完。' : 'Caught up at the time of this check.'));
  }

  @override
  void dispose() {
    watcher?.removeListener(_changed);
    watcher?.dispose();
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

  bool get _canEdit =>
      !confirmingBudget &&
      watcher?.original == null &&
      watcher?.busy != true &&
      session.canPrepare;

  bool get _canRun =>
      _canEdit &&
      widget.entry.hasEligibleDirectoryChannels &&
      handler != null &&
      kind != null &&
      widget.backend.supportsLocalChannels;

  Future<bool> _approveRun(
    ChannelPrepareRequest request, {
    bool wait = false,
  }) async {
    final originalBackend = widget.backend;
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
                  if (wait)
                    'If these cards are caught up, wait up to ${request.lifetimeMs ~/ 1000} seconds for a change and run this page once. The wait and the plugin share the same deadline.',
                  review.cards.join('\n'),
                  review.previousCursor == null
                      ? 'Use the last confirmed position, or start at the beginning for a new selection.'
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
          identical(widget.backend, originalBackend) &&
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
          ? reviewedCards(int.parse(pageSize.text.trim()))
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
      final wait =
          review != null &&
          waitForChanges &&
          widget.backend is ChangesWaitBackend;
      if ((compatibilityBudget || review != null) &&
          !await _approveRun(request, wait: wait)) {
        return;
      }
      inputFailure = null;
      peerReceipt = null;
      peerFailure = null;
      if (wait) {
        watcher ??= ChangesPageWait(
          widget.backend as ChangesWaitBackend,
          (approved) =>
              session.prepareAndRun(approved, const [], fromApprovedWait: true),
        )..addListener(_changed);
        await watcher!.start(request);
      } else {
        await session.prepareAndRun(request, frames);
      }
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
                onChanged: _canEdit
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
                  onChanged: _canEdit
                      ? (value) => setState(() => kind = value)
                      : null,
                ),
              if (!changes)
                CheckboxListTile(
                  title: const Text('Duplex peer (requires empty source)'),
                  value: duplex,
                  onChanged: _canEdit
                      ? (value) => setState(() => duplex = value ?? false)
                      : null,
                ),
              TextField(
                key: const ValueKey('channel-source-bytes'),
                controller: source,
                onChanged: changes ? (_) => setState(clearCheckpoint) : null,
                maxLines: 6,
                enabled: _canEdit,
                decoration: InputDecoration(
                  labelText: changes
                      ? 'Card IDs, one per line'
                      : 'Original bytes hex | cursor hex',
                ),
              ),
              if (changes) ...[
                if (widget.backend is ChangesWaitBackend)
                  CheckboxListTile(
                    title: Text(
                      Localizations.localeOf(context).languageCode == 'zh'
                          ? '等待下一页变更并运行一次'
                          : 'Wait for changes and run one page',
                    ),
                    value: waitForChanges,
                    onChanged: _canRun
                        ? (value) =>
                              setState(() => waitForChanges = value ?? false)
                        : null,
                  ),
                TextField(
                  controller: pageSize,
                  enabled: _canEdit,
                  decoration: InputDecoration(
                    labelText: 'Records per approval (1–$maxPageSize)',
                  ),
                ),
                TextField(
                  controller: resume,
                  onChanged: (_) => setState(clearCheckpoint),
                  enabled: _canEdit,
                  key: const ValueKey('channel-changes-resume'),
                  decoration: const InputDecoration(
                    labelText: 'Resume cursor (optional)',
                  ),
                ),
                if (widget.backend case final ChangesCheckpointBackend backend)
                  TextButton(
                    onPressed: session.canPrepare && !checkingCheckpoint
                        ? () => inspectCheckpoint(backend)
                        : null,
                    child: Text(
                      Localizations.localeOf(context).languageCode == 'zh'
                          ? '核对已确认的游标'
                          : 'Check confirmed cursor',
                    ),
                  ),
                if (checkpoint case final result?) ...[
                  Text(checkpointMessage),
                  if (result.latest case final latest?)
                    SelectableText(
                      latest
                          .map((b) => b.toRadixString(16).padLeft(2, '0'))
                          .join(),
                    ),
                ],
                if (widget.backend
                    case final ChangesObservationBackend observer)
                  ChangesWatchControls(
                    backend: observer,
                    packageId: widget.entry.id,
                    digest: widget.entry.digest,
                    revision: widget.registryRevision,
                    readSelection: () => reviewedCards(1),
                    enabled: _canEdit,
                  ),
              ],
              if (inputFailure != null) Text(inputFailure.toString()),
              if (watcher case final w?) ...[
                if (w.original != null)
                  Wrap(
                    children: [
                      Text(
                        w.failure != null ||
                                w.phase == ChangesWaitPhase.unconfirmed
                            ? 'Original wait outcome unconfirmed.'
                            : 'Waiting on the reviewed cards.',
                      ),
                      TextButton(
                        onPressed: w.busy ? null : () => unawaited(w.check()),
                        child: const Text('Check original wait'),
                      ),
                      TextButton(
                        onPressed: () => unawaited(w.check(cancel: true)),
                        child: const Text('Cancel wait'),
                      ),
                    ],
                  ),
                if (w.phase == ChangesWaitPhase.expired)
                  const Text('The wait expired.'),
                if (w.phase == ChangesWaitPhase.changed)
                  const Text(
                    'The selection or confirmed cursor changed. Review the cards again.',
                  ),
                if (w.phase == ChangesWaitPhase.cancelled)
                  const Text('Wait cancelled.'),
              ],
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
          onPressed: current == null && !session.canRecoverPreparedWait
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
