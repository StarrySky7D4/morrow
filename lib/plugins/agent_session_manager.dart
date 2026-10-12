import 'dart:async';
import 'dart:convert';
import 'package:flutter/material.dart';
import 'agent_session_controller.dart';
import 'agent_session_models.dart';
import 'agent_session_text.dart';
import 'agent_session_workspace.dart';
import 'agent_task_discovery.dart';

/// A temporary view of one Workbench-owned workspace. Mounting, navigating
/// away, and rebuilding never discover, attach, submit, read or release it.
final class AgentSessionManager extends StatefulWidget {
  const AgentSessionManager({
    super.key,
    required this.workspace,
    this.ink,
    this.muted,
    this.line,
    this.radius,
  });
  final AgentSessionWorkspace workspace;
  final Color? ink, muted, line;
  final BorderRadius? radius;
  @override
  State<AgentSessionManager> createState() => _AgentSessionManagerState();
}

final class _AgentSessionManagerState extends State<AgentSessionManager> {
  final _editors = <AgentSessionDraft, TextEditingController>{};
  StreamSubscription<AgentSessionWorkspaceView>? _subscription;
  String? _notice;
  int _revision = 0;

  @override
  void initState() {
    super.initState();
    _bind();
  }

  void _bind() {
    final owner = widget.workspace;
    for (final field in AgentSessionDraft.values) {
      _editors[field] = TextEditingController(
        text: widget.workspace.view.drafts[field],
      );
    }
    _subscription = widget.workspace.changes.listen((view) {
      if (!mounted || !identical(owner, widget.workspace)) return;
      for (final field in AgentSessionDraft.values) {
        final text = view.drafts[field]!;
        if (_editors[field]!.text != text) _editors[field]!.text = text;
      }
      setState(() {});
    });
  }

  void _unbind() {
    final subscription = _subscription;
    if (subscription != null) unawaited(subscription.cancel());
    for (final editor in _editors.values) {
      editor.dispose();
    }
    _editors.clear();
  }

  @override
  void didUpdateWidget(AgentSessionManager oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.workspace, widget.workspace)) {
      _revision++;
      _unbind();
      _notice = null;
      _bind();
    }
  }

  @override
  void dispose() {
    _revision++;
    _unbind();
    // The original Workbench alone disposes the workspace. No backend call.
    super.dispose();
  }

  Future<void> _act(Future<Object?> Function() action) async {
    final owner = widget.workspace, revision = _revision;
    setState(() => _notice = null);
    try {
      await action();
    } on FormatException {
      if (mounted &&
          revision == _revision &&
          identical(owner, widget.workspace)) {
        setState(() => _notice = 'invalidInput');
      }
    } catch (_) {
      if (mounted &&
          revision == _revision &&
          identical(owner, widget.workspace)) {
        setState(() => _notice = 'failed');
      }
    }
  }

  String _hex(List<int> value) =>
      value.map((b) => b.toRadixString(16).padLeft(2, '0')).join();
  String _phase(AgentSessionPhase phase) => switch (phase) {
    AgentSessionPhase.detached => 'detached',
    AgentSessionPhase.attached => 'attached',
    AgentSessionPhase.pending => 'pending',
    AgentSessionPhase.opened => 'opened',
    AgentSessionPhase.unknown => 'unknown',
    AgentSessionPhase.staleBackend => 'unavailable',
    AgentSessionPhase.disposed => 'closed',
  };
  String _observation(AgentTaskKind? kind) => switch (kind) {
    null => 'notObserved',
    AgentTaskKind.none => 'observationNone',
    AgentTaskKind.runningAgent => 'observationRunning',
    AgentTaskKind.retainedAgentExit => 'observationRetainedExit',
    AgentTaskKind.otherTask => 'observationOther',
    AgentTaskKind.unavailable => 'unavailable',
  };

  @override
  Widget build(BuildContext context) {
    final text = AgentSessionText.of(context), workspace = widget.workspace;
    final view = workspace.view, session = view.session;
    Widget value(String key, Object? value) =>
        SelectableText('${text.text(key)}: ${value ?? '—'}');
    Widget button(
      String key,
      bool enabled,
      Future<Object?> Function() action,
    ) => OutlinedButton(
      key: ValueKey('agent-session-$key'),
      style: OutlinedButton.styleFrom(
        foregroundColor: widget.ink,
        side: widget.line == null ? null : BorderSide(color: widget.line!),
        shape: widget.radius == null
            ? null
            : RoundedRectangleBorder(borderRadius: widget.radius!),
      ),
      onPressed: enabled ? () => _act(action) : null,
      child: Text(text.text(key)),
    );
    Widget field(AgentSessionDraft draft, String key, {int lines = 1}) =>
        SizedBox(
          width: lines > 1 ? 560 : 260,
          child: TextField(
            key: ValueKey('agent-session-draft-${draft.name}'),
            controller: _editors[draft],
            enabled: !view.closed,
            minLines: lines,
            maxLines: lines > 1 ? 6 : 1,
            maxLength: draft == AgentSessionDraft.eventBody ? 32768 : 256,
            decoration: InputDecoration(labelText: text.text(key)),
            onChanged: (input) {
              try {
                workspace.setDraft(draft, input);
              } catch (_) {
                setState(() => _notice = 'invalidInput');
              }
            },
          ),
        );
    Widget metadata(AgentSessionInfo info) => Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        value('sessionId', info.sessionId),
        value('parent', info.parent),
        value('parentTail', info.parentTail),
        value('epoch', info.epoch),
        value('tail', info.tail),
        value('floor', info.floor),
        value('revision', info.revision),
        value('archived', info.archived),
        value(
          'checkpoint',
          '${info.checkpointTail} / ${info.checkpointSealed} / ${_hex(info.checkpointSha256)}',
        ),
        if (info.parentCheckpointSha256.isNotEmpty)
          value('parent', _hex(info.parentCheckpointSha256)),
      ],
    );
    final snapshot = view.snapshot;
    return Padding(
      padding: const EdgeInsets.all(19),
      child: DefaultTextStyle.merge(
        style: TextStyle(color: widget.ink),
        child: Column(
          key: const ValueKey('agent-session-manager'),
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              text.text('title'),
              style: Theme.of(
                context,
              ).textTheme.titleLarge?.copyWith(color: widget.ink),
            ),
            Text(text.text('summary'), style: TextStyle(color: widget.muted)),
            Text(
              text.text('readOnlyNotice'),
              style: TextStyle(color: widget.muted),
            ),
            Text(text.text(_observation(view.observation?.kind))),
            if (view.observation?.taskKey != null)
              SelectableText(_hex(view.observation!.taskKey!)),
            if (view.discoveryBusy) Text(text.text('observing')),
            if (session != null) ...[
              Text(text.text(_phase(session.phase))),
              value('submission', session.submission),
              if (view.attachedTaskKey != null)
                value('attached', _hex(view.attachedTaskKey!)),
              value('writer', session.writerSessionId),
            ],
            if (view.outcomeUnknown)
              Text(
                text.text('unknown'),
                key: const ValueKey('agent-session-unknown'),
              ),
            if (view.closed) Text(text.text('closed')),
            if (view.actionBusy) Text(text.text('busy')),
            if (_notice != null) Text(text.text(_notice!)),
            if (_notice == null && view.failure != null)
              Text(text.text('failed')),
            if (view.lastResult?.data is AgentSessionRejected)
              Text(text.text('rejected')),
            const SizedBox(height: 12),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                button('observe', view.canObserve, workspace.observe),
                button('attach', view.canAttach, workspace.attach),
                button('open', view.canOpen, workspace.open),
                button('refreshStatus', view.canStatus, workspace.status),
                button('read', view.canRead, workspace.read),
                button('markLost', view.canStatus, workspace.markLost),
                button('detach', view.canDetach, workspace.detach),
              ],
            ),
            Divider(color: widget.line),
            Wrap(
              spacing: 16,
              runSpacing: 8,
              children: [
                field(AgentSessionDraft.sessionId, 'sessionId'),
                field(AgentSessionDraft.parent, 'parentSession'),
                field(AgentSessionDraft.parentTail, 'parentTail'),
              ],
            ),
            const SizedBox(height: 8),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                button('listSessions', view.canSubmit, workspace.listSessions),
                button(
                  'createSession',
                  view.canSubmit,
                  workspace.createSession,
                ),
                button(
                  'admitWriter',
                  view.canAdmitWriter,
                  workspace.admitWriter,
                ),
                button('openWriter', view.canOpenWriter, workspace.openWriter),
                button(
                  'closeWriter',
                  view.canCloseWriter,
                  workspace.closeWriter,
                ),
              ],
            ),
            Text(text.text('sessionList')),
            for (final info in view.sessions)
              TextButton(
                key: ValueKey('agent-session-select-${info.sessionId}'),
                onPressed: view.closed
                    ? null
                    : () => workspace.selectSession(info.sessionId),
                child: Text(info.sessionId),
              ),
            Wrap(
              spacing: 16,
              runSpacing: 8,
              children: [
                field(AgentSessionDraft.after, 'after'),
                field(AgentSessionDraft.limit, 'limit'),
              ],
            ),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                button('snapshot', view.canSubmit, workspace.snapshotSession),
                // A separate key distinguishes the writer-bound Snapshot. Its label
                // remains the existing localized Snapshot and Writer words.
                OutlinedButton(
                  key: const ValueKey('agent-session-snapshotWriter'),
                  onPressed: view.canSubmit && session?.writerSessionId != null
                      ? () => _act(workspace.snapshotWriter)
                      : null,
                  child: Text(
                    '${text.text('writer')} · ${text.text('snapshot')}',
                  ),
                ),
              ],
            ),
            if (view.selectedInfo != null) ...[
              Text(text.text('selectedSession')),
              metadata(view.selectedInfo!),
            ],
            if (snapshot != null) ...[
              value('gap', snapshot.gap),
              Text(text.text('events')),
              for (final event in snapshot.events)
                SelectableText(
                  '${event.sequence} · ${event.event.id}\n${utf8.decode(event.event.body, allowMalformed: true)}',
                ),
              if (snapshot.checkpoint.isNotEmpty)
                value('checkpoint', _hex(snapshot.checkpoint)),
            ],
            Divider(color: widget.line),
            field(AgentSessionDraft.eventId, 'eventId'),
            field(AgentSessionDraft.eventBody, 'eventBody', lines: 3),
            const SizedBox(height: 8),
            button('append', view.canAppend, workspace.append),
            const SizedBox(height: 8),
            Text(text.text('releaseNotice')),
          ],
        ),
      ),
    );
  }
}
