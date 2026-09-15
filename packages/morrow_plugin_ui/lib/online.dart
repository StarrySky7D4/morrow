/// Online host-owned plugin view. UI replies never assert content was saved.
library;

import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';
import 'package:flutter/material.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'package:morrow_core_client/ui.dart';
import 'morrow_plugin_ui.dart';
export 'morrow_plugin_ui.dart';

/// Each instance must own one host session. Implementations may submit/poll internally.
/// Future completion must carry the authoritative post-operation host counters.
/// A thrown error means admission/outcome is unknown; callers must not retry an event.
abstract interface class PluginUiTransport {
  Future<PluginUiReply> open(String seed);
  Future<PluginUiReply> event(Uint8List bytes);
  Future<void> close();
}

enum PluginUiFailureKind { busy, rejected, plugin, execution, unavailable }

enum PluginUiHostMessage {
  inputTooLong,
  connectionLost,
  rejected,
  execution,
  unavailable,
}

@immutable
final class PluginUiFailure {
  const PluginUiFailure(this.kind, this.message, {this.hostMessage});
  final PluginUiHostMessage? hostMessage;
  final PluginUiFailureKind kind;
  final String message;
}

@immutable
final class PluginUiReply {
  PluginUiReply({
    required this.view,
    required this.generation,
    required this.revision,
    required this.serial,
    Uint8List? documentBytes,
    this.failure,
  }) : documentBytes = documentBytes == null
           ? null
           : _copyDocument(documentBytes);
  final String view;
  final BigInt generation, revision, serial;
  final Uint8List? documentBytes;
  final PluginUiFailure? failure;
}

Uint8List _copyDocument(Uint8List bytes) {
  if (bytes.length > maxUiBytes) {
    throw const FormatException("UI document too large");
  }
  return Uint8List.fromList(bytes).asUnmodifiableView();
}

enum PluginUiPhase { idle, opening, ready, busy, interrupted, closed }

enum PluginUiAdmission { sent, queued, busy, rejected, closed }

/// Serial/revision proposals are routing data; only the host admits them.
/// One async operation at a time, with at most one pending text intent per node.
/// Discrete actions never queue. Failures retain local drafts but clear automatic delivery.
class PluginUiController extends ChangeNotifier {
  PluginUiController(this.transport);
  final PluginUiTransport transport;
  PluginUiPhase _phase = PluginUiPhase.idle;
  PluginUiPhase get phase => _phase;
  PluginUiFailure? _failure;
  PluginUiFailure? get failure => _failure;
  UiDocumentModel? _document, _renderDocument;
  UiDocumentModel? get document => _renderDocument;
  String? _view;
  String? get view => _view;
  BigInt? _generation;
  BigInt? get generation => _generation;
  BigInt _revision = BigInt.zero, _serial = BigInt.zero;
  BigInt get revision => _revision;
  BigInt get serial => _serial;
  final _drafts = <String, String>{};
  final _queue = <String, UiIntent>{};
  bool _disposed = false;
  int _epoch = 0;
  Future<void>? _closing;
  bool get canEdit =>
      _phase == PluginUiPhase.ready || _phase == PluginUiPhase.busy;
  bool get actionsEnabled => _phase == PluginUiPhase.ready;
  Object get viewIdentity => (this, _view, _generation);

  Future<void> open(String seed) async {
    if (_disposed || _phase != PluginUiPhase.idle) {
      throw StateError('View already opened');
    }
    if (utf8.encode(seed).length > 32) {
      throw const FormatException('UI seed too long');
    }
    final epoch = _epoch;
    _phase = PluginUiPhase.opening;
    notifyListeners();
    try {
      final reply = await transport.open(seed);
      if (!_current(epoch)) return;
      _apply(reply, null);
    } catch (_) {
      if (_current(epoch)) _interrupt();
    }
  }

  PluginUiAdmission submit(UiIntent intent, {BigInt? expectedRevision}) {
    if (_disposed || !canEdit) return PluginUiAdmission.closed;
    if (expectedRevision != null && expectedRevision != _revision) {
      return PluginUiAdmission.rejected;
    }
    final nodes = _document?.nodes.where((n) => n.id == intent.node);
    final node = nodes == null || nodes.isEmpty ? null : nodes.first;
    if (node == null ||
        !node.enabled ||
        node.action != intent.action ||
        !_matches(node, intent)) {
      return PluginUiAdmission.rejected;
    }
    if (_phase == PluginUiPhase.busy && intent.kind != EventKind.editText) {
      return PluginUiAdmission.busy;
    }
    if (intent.kind == EventKind.editText) {
      final previous = _drafts[intent.node];
      _drafts[intent.node] = intent.text;
      try {
        _rebuildDocument();
      } catch (_) {
        if (previous == null) {
          _drafts.remove(intent.node);
        } else {
          _drafts[intent.node] = previous;
        }
        _rebuildDocument();
        _failure = const PluginUiFailure(
          PluginUiFailureKind.rejected,
          '输入内容已超过此界面的容量，请缩短后重试。',
          hostMessage: PluginUiHostMessage.inputTooLong,
        );
        notifyListeners();
        return PluginUiAdmission.rejected;
      }
    }
    if (_phase == PluginUiPhase.busy) {
      _queue[intent.node] = intent;
      notifyListeners();
      return PluginUiAdmission.queued;
    }
    _send(intent);
    return PluginUiAdmission.sent;
  }

  static bool _matches(UiNode node, UiIntent intent) {
    if ((node.kind, intent.kind) case (Kind.textInput, EventKind.editText)) {
      return !intent.checked &&
          utf8.encode(intent.text).length <= node.maxBytes &&
          !RegExp(r'[\x00-\x08\x0b-\x1f\x7f-\x9f]').hasMatch(intent.text);
    }
    return intent.text.isEmpty &&
        switch ((node.kind, intent.kind)) {
          (Kind.button, EventKind.activate) => !intent.checked,
          (Kind.toggle, EventKind.setToggle) => true,
          _ => false,
        };
  }

  Future<void> _send(UiIntent intent) async {
    final epoch = _epoch;
    late final UiEvent event;
    try {
      event = UiEvent(
        view: _view!,
        generation: _generation!,
        revision: _revision,
        serial: _serial + BigInt.one,
        node: intent.node,
        action: intent.action,
        kind: intent.kind,
        text: intent.text,
        checked: intent.checked,
      );
    } catch (_) {
      _interrupt();
      return;
    }
    _phase = PluginUiPhase.busy;
    _failure = null;
    notifyListeners();
    try {
      final reply = await transport.event(event.encode());
      if (!_current(epoch)) return;
      _apply(reply, event);
      if (_phase == PluginUiPhase.ready && _failure == null) _sendQueued();
    } catch (_) {
      if (_current(epoch)) _interrupt();
    }
  }

  void _apply(PluginUiReply reply, UiEvent? event) {
    final max = (BigInt.one << 64) - BigInt.one;
    if (reply.view.isEmpty ||
        utf8.encode(reply.view).length > 256 ||
        RegExp(r'[\x00-\x1f\x7f-\x9f/\\:]').hasMatch(reply.view) ||
        reply.generation <= BigInt.zero ||
        reply.generation > max ||
        reply.revision < BigInt.zero ||
        reply.revision > max ||
        reply.serial < BigInt.zero ||
        reply.serial > max ||
        (reply.documentBytes == null) == (reply.failure == null)) {
      throw const FormatException('Invalid host UI reply');
    }
    if (event != null &&
        (reply.view != _view || reply.generation != _generation)) {
      throw const FormatException('Changed host UI identity');
    }
    final expectedSerial = event?.serial ?? BigInt.zero;
    if (reply.failure == null) {
      if (reply.serial != expectedSerial ||
          reply.revision != _revision + BigInt.one) {
        throw const FormatException('Invalid host UI counters');
      }
      final doc = UiDocument.decode(reply.documentBytes!);
      _document = doc;
      if (event?.kind == EventKind.editText &&
          _drafts[event!.node] == event.text) {
        _drafts.remove(event.node);
      }
      _drafts.removeWhere(
        (id, text) => !doc.nodes.any(
          (n) =>
              n.id == id &&
              n.kind == Kind.textInput &&
              utf8.encode(text).length <= n.maxBytes,
        ),
      );
    } else {
      if (reply.revision != _revision ||
          (reply.serial != _serial && reply.serial != expectedSerial)) {
        throw const FormatException('Invalid host failure counters');
      }
      _queue.clear();
    }
    _view = reply.view;
    _generation = reply.generation;
    _revision = reply.revision;
    _serial = reply.serial;
    _failure = reply.failure;
    _phase = _document == null
        ? PluginUiPhase.interrupted
        : PluginUiPhase.ready;
    _rebuildDocument();
    notifyListeners();
  }

  void _sendQueued() {
    while (_queue.isNotEmpty) {
      final key = _queue.keys.first;
      final intent = _queue.remove(key)!;
      if (submit(intent) == PluginUiAdmission.sent) break;
    }
  }

  void _rebuildDocument() {
    final doc = _document;
    _renderDocument = doc == null
        ? null
        : UiDocumentModel([
            for (final n in doc.nodes)
              if (_drafts[n.id] case final String draft)
                UiNode(
                  id: n.id,
                  parent: n.parent,
                  kind: n.kind,
                  label: n.label,
                  text: draft,
                  action: n.action,
                  enabled: n.enabled,
                  checked: n.checked,
                  maxBytes: n.maxBytes,
                  tone: n.tone,
                )
              else
                n,
          ]);
  }

  bool _current(int epoch) =>
      !_disposed && _epoch == epoch && _phase != PluginUiPhase.closed;
  void _interrupt() {
    _queue.clear();
    _failure = const PluginUiFailure(
      PluginUiFailureKind.unavailable,
      '连接已中断，请重新打开此插件界面。',
      hostMessage: PluginUiHostMessage.connectionLost,
    );
    _phase = PluginUiPhase.interrupted;
    if (!_disposed) notifyListeners();
  }

  Future<void> close() {
    if (_closing != null) return _closing!;
    _epoch++;
    _phase = PluginUiPhase.closed;
    _queue.clear();
    if (!_disposed) notifyListeners();
    return _closing = Future<void>.sync(
      transport.close,
    ).catchError((Object _) {});
  }

  @override
  void dispose() {
    _disposed = true;
    unawaited(close());
    super.dispose();
  }
}

/// Own the controller in the calling screen, open it once, and dispose it on teardown.
/// Replacing the controller invalidates old widget callbacks and resets local editing state.
class ManagedPluginForm extends StatefulWidget {
  const ManagedPluginForm({
    super.key,
    required this.controller,
    this.documentBuilder,
  });
  final PluginUiController controller;

  /// Optional trusted first-party presentation adapter. It must preserve node
  /// identities, actions, input values and limits; it never changes controller data.
  final UiDocumentModel Function(BuildContext, UiDocumentModel)?
  documentBuilder;
  @override
  State<ManagedPluginForm> createState() => _ManagedPluginFormState();
}

class _ManagedPluginFormState extends State<ManagedPluginForm> {
  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: widget.controller,
    builder: (context, _) {
      final controller = widget.controller;
      final revision = controller.revision;
      final identity = controller.viewIdentity;
      final document = controller.document;
      return Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (document == null)
            Padding(
              padding: const EdgeInsets.all(16),
              child: Text(
                controller.phase == PluginUiPhase.opening
                    ? L10n.of(context).pluginsOpeningView
                    : L10n.of(context).pluginsUnavailableView,
                style: Theme.of(context).textTheme.bodyMedium,
              ),
            ),
          if (document != null)
            Flexible(
              child: AbsorbPointer(
                absorbing: !controller.canEdit,
                child: PluginForm(
                  document:
                      widget.documentBuilder?.call(context, document) ??
                      document,
                  viewIdentity: identity,
                  actionsEnabled: controller.actionsEnabled,
                  onIntent: (intent) {
                    if (!mounted ||
                        widget.controller != controller ||
                        controller.viewIdentity != identity) {
                      return;
                    }
                    controller.submit(intent, expectedRevision: revision);
                  },
                ),
              ),
            ),
          if (controller.phase == PluginUiPhase.busy)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 4),
              child: Text(
                L10n.of(context).pluginsUpdatingView,
                style: Theme.of(context).textTheme.bodySmall,
              ),
            ),
          if (controller.failure case final failure?)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 8),
              child: Text(
                pluginUiFailureMessage(L10n.of(context), failure),
                style: Theme.of(context).textTheme.bodySmall?.copyWith(
                  color: Theme.of(context).colorScheme.error,
                ),
              ),
            ),
        ],
      );
    },
  );
}

/// Translate host status codes only; plugin-authored literals remain verbatim.
String pluginUiFailureMessage(AppLocalizations l, PluginUiFailure failure) =>
    switch (failure.hostMessage) {
      PluginUiHostMessage.inputTooLong => l.pluginsInputTooLong,
      PluginUiHostMessage.connectionLost => l.pluginsConnectionLost,
      PluginUiHostMessage.rejected => l.pluginsUiRejected,
      PluginUiHostMessage.execution => l.pluginsUiExecution,
      PluginUiHostMessage.unavailable => l.pluginsUiUnavailable,
      null => failure.message,
    };
