import '../settings_surface.dart';
import 'package:morrow_i18n/morrow_i18n.dart';
import 'dart:async';
import '../appearance.dart';
import 'io_settings_page.dart';
import 'credential_manager.dart';
import 'endpoint_control.dart';
import 'endpoint_manager.dart';
import 'http_task_manager.dart';
import 'io_task_control.dart';
import 'service_control.dart';
import 'service_manager.dart';
import 'service_run_control.dart';
import 'service_run_manager.dart';
import 'service_run_session.dart';
import 'dart:convert';
import 'dart:typed_data';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'theme_package_import.dart';
import 'package:morrow_plugin_ui/online.dart';

class PluginLibraryPage {
  const PluginLibraryPage({
    required this.revision,
    required this.entries,
    required this.cursor,
  });
  final BigInt revision;
  final List<PluginLibraryEntry> entries;
  final String cursor;
}

class PluginLibraryEntry {
  const PluginLibraryEntry({
    required this.id,
    required this.name,
    required this.version,
    required this.digest,
    required this.enabled,
    required this.builtin,
    required this.available,
    required this.declared,
    required this.approved,
    required this.dependencies,
    required this.handlers,
    required this.issue,
    this.declaredIo = const [],
    this.approvedIo = const [],
    this.ioHandlers = const [],
  });
  final String id, name, version, issue;
  final Uint8List digest;
  final bool enabled, builtin, available;
  final List<String> declared, approved, dependencies, declaredIo, approvedIo;
  final List<PluginTransformHandler> handlers;
  final List<String> ioHandlers;
  bool get isTheme => handlers.any(
    (h) =>
        h.name == 'theme.describe' &&
        h.inputType == 'morrow.ui.theme.request.v1' &&
        h.outputType == 'morrow.ui.theme.v1',
  );
}

class PluginTransformHandler {
  const PluginTransformHandler({
    required this.name,
    required this.inputType,
    required this.outputType,
    required this.maxInputBytes,
    required this.maxOutputBytes,
  });
  final String name, inputType, outputType;
  final int maxInputBytes, maxOutputBytes;
}

abstract interface class ExternalPluginControl {
  Future<PluginLibraryPage> pluginPage({String cursor = '', BigInt? revision});
  Future<PluginLibraryPage> inspectPlugin(String path);
  Future<void> importPlugin(String path, Uint8List digest, BigInt revision);
  Future<void> configureExternal(
    PluginLibraryEntry entry,
    BigInt revision,
    List<String> approved,
    bool enable,
  );
  Future<void> configureExternalIo(
    PluginLibraryEntry entry,
    BigInt revision,
    List<String> approved,
  );
  Future<void> removeExternal(PluginLibraryEntry entry, BigInt revision);
  Future<Uint8List> transformExternal(
    PluginLibraryEntry entry,
    BigInt revision,
    PluginTransformHandler handler,
    Uint8List input,
  );
  PluginUiTransport createExternalPluginUi(
    PluginLibraryEntry entry,
    BigInt revision,
  );
}

// One bounded catalog snapshot per backend, not an authorization cache.
class _CatalogSnapshot {
  _CatalogSnapshot(this.revision, this.firstPage, this.entries);
  final BigInt revision;
  final String firstPage;
  final List<PluginLibraryEntry> entries;
}

var _catalogs = Expando<_CatalogSnapshot>('plugin directory snapshots');

void clearPluginCatalogSnapshots() {
  _catalogs = Expando<_CatalogSnapshot>('plugin directory snapshots');
}

final _releases = Expando<Set<_ObservedTransport>>('pending plugin releases');

String _pageFingerprint(PluginLibraryPage page) => jsonEncode([
  page.cursor,
  for (final e in page.entries)
    [
      e.id,
      e.name,
      e.version,
      e.digest,
      e.enabled,
      e.builtin,
      e.available,
      e.declared,
      e.approved,
      e.dependencies,
      e.issue,
      e.declaredIo,
      e.approvedIo,
      e.ioHandlers,
      for (final h in e.handlers)
        [h.name, h.inputType, h.outputType, h.maxInputBytes, h.maxOutputBytes],
    ],
]);

enum PluginLibraryMode { library, io }

class PluginLibrary extends StatefulWidget {
  const PluginLibrary({
    super.key,
    required this.backend,
    required this.onChanged,
    required this.ink,
    required this.muted,
    required this.line,
    required this.radius,
    this.pickPackage,
    this.pickThemePackage,
    this.openIo,
    this.mode = PluginLibraryMode.library,
  });
  final PluginLibraryMode mode;
  final ExternalPluginControl backend;
  final VoidCallback onChanged;
  final Color ink, muted, line;
  final BorderRadius radius;
  final Future<String?> Function()? pickPackage;
  final Future<XFile?> Function()? pickThemePackage;
  final Future<void> Function()? openIo;
  @override
  State<PluginLibrary> createState() => _PluginLibraryState();
}

// Controller.close intentionally hides transport errors. Preserve the underlying
// Future here so explicit library actions can report failure before changing policy.
class _ObservedTransport implements PluginUiTransport {
  _ObservedTransport(this.delegate);
  final PluginUiTransport delegate;
  Future<void>? _closing;
  bool _closeFailed = false;
  @override
  Future<PluginUiReply> open(String seed) => delegate.open(seed);
  @override
  Future<PluginUiReply> event(Uint8List bytes) => delegate.event(bytes);
  @override
  Future<void> close() => _closing ??= Future<void>.sync(delegate.close)
      .catchError((Object error, StackTrace stack) {
        _closeFailed = true;
        Error.throwWithStackTrace(error, stack);
      });
  Future<void> closeOrRetry() => _closeFailed ? retryClose() : close();
  Future<void> retryClose() {
    _closing = null;
    _closeFailed = false;
    return close();
  }
}

class _PluginLibraryState extends State<PluginLibrary> {
  List<PluginLibraryEntry> _entries = [];
  BigInt? _revision;
  bool _busy = false, _confirmed = false;
  int _epoch = 0;
  String? _candidatePath, _toolId, _fileName;
  ThemePackageSelection? _candidateTheme;
  bool get _themeImport =>
      widget.backend is ThemePackageImportControl &&
      (widget.backend as ThemePackageImportControl).supportsThemePackageImport;
  String Function(AppLocalizations)? _message, _result;
  PluginLibraryPage? _preview;
  final Map<String, Set<String>> _approvals = {};
  final Map<String, Set<String>> _ioApprovals = {};
  final _text = TextEditingController();
  PluginTransformHandler? _handler;
  Uint8List? _fileBytes;
  PluginUiController? _controller;
  _ObservedTransport? _transport, _unclosed;
  String? _formId;

  bool _current(ExternalPluginControl backend, int epoch) =>
      mounted && epoch == _epoch && identical(widget.backend, backend);
  @override
  void initState() {
    super.initState();
    unawaited(_refresh(reuseCatalog: true));
  }

  @override
  void didUpdateWidget(covariant PluginLibrary oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.backend, widget.backend)) {
      _epoch++;
      _releaseForm(oldWidget.backend);
      _busy = false;
      _confirmed = false;
      _entries = [];
      _revision = null;
      _preview = null;
      _clearTool();
      unawaited(_refresh(reuseCatalog: true));
    }
  }

  void _releaseForm([ExternalPluginControl? owner]) {
    final controller = _controller;
    final transport = _transport ?? _unclosed;
    _controller = null;
    _transport = null;
    _unclosed = null;
    _formId = null;
    if (transport == null) return;
    final pending = _releases[owner ?? widget.backend] ??=
        <_ObservedTransport>{};
    pending.add(transport);
    unawaited(() async {
      try {
        if (controller != null) {
          await controller.close();
          await transport.close();
        } else {
          await transport.closeOrRetry();
        }
        pending.remove(transport);
      } catch (error, stack) {
        FlutterError.reportError(
          FlutterErrorDetails(
            exception: error,
            stack: stack,
            library: 'plugin library',
            context: ErrorDescription('closing an external plugin view'),
          ),
        );
      } finally {
        controller?.dispose();
      }
    }());
  }

  @override
  void dispose() {
    _epoch++;
    _releaseForm();
    _text.dispose();
    super.dispose();
  }

  Future<void> _drainReleases(ExternalPluginControl backend) async {
    final pending = _releases[backend];
    if (pending == null) return;
    for (final transport in pending.toList()) {
      await transport.closeOrRetry();
      pending.remove(transport);
    }
  }

  Future<void> _closeForm(int epoch) async {
    final controller = _controller;
    final transport = _transport ?? _unclosed;
    if (transport == null) return;
    try {
      if (controller != null) {
        await controller.close();
        await transport.close();
      } else {
        // A new explicit user action may retry the idempotent close request.
        await transport.retryClose();
      }
      if (mounted && epoch == _epoch && identical(_unclosed, transport)) {
        _unclosed = null;
      }
    } catch (_) {
      if (mounted && epoch == _epoch) _unclosed = transport;
      rethrow;
    } finally {
      // On replacement/disposal, _releaseForm owns cleanup of the old controller.
      if (mounted && epoch == _epoch) controller?.dispose();
      if (mounted && epoch == _epoch && identical(_controller, controller)) {
        _controller = null;
        _transport = null;
        _formId = null;
        if (mounted) setState(() {});
      }
    }
  }

  Future<void> _loadPages(
    ExternalPluginControl backend,
    int epoch, {
    bool reuseCatalog = false,
  }) async {
    var cursor = '';
    var firstPage = '';
    BigInt? revision;
    final entries = <PluginLibraryEntry>[];
    final cursors = <String>{};
    final ids = <String>{};
    do {
      if (!cursors.add(cursor) || cursors.length > 4096) {
        throw const FormatException('插件分页未能结束');
      }
      final page = await backend.pluginPage(cursor: cursor, revision: revision);
      if (!_current(backend, epoch)) return;
      if (cursor.isEmpty) {
        firstPage = _pageFingerprint(page);
        final cached = reuseCatalog ? _catalogs[backend] : null;
        if (cached != null &&
            cached.revision == page.revision &&
            cached.firstPage == firstPage) {
          // Recheck the live catalog revision before exposing this snapshot.
          // All actions still pass revision/digest to the host for authorization.
          entries.addAll(cached.entries);
          revision = page.revision;
          break;
        }
      }
      revision ??= page.revision;
      if (page.revision != revision) throw const FormatException('插件列表已变化');
      for (final entry in page.entries) {
        if (!ids.add(entry.id)) throw const FormatException('插件列表包含重复条目');
        entries.add(entry);
      }
      cursor = page.cursor;
    } while (cursor.isNotEmpty);
    if (!_current(backend, epoch)) return;
    // Do not retain an unbounded directory or large strings between visits.
    final bytes = entries.fold<int>(
      0,
      (total, e) =>
          total +
          utf8
              .encode(
                _pageFingerprint(
                  PluginLibraryPage(
                    revision: revision!,
                    entries: [e],
                    cursor: '',
                  ),
                ),
              )
              .length,
    );
    _catalogs[backend] = entries.length <= 256 && bytes <= 2 * 1024 * 1024
        ? _CatalogSnapshot(revision, firstPage, List.unmodifiable(entries))
        : null;
    setState(() {
      _entries = entries;
      _revision = revision;
      _confirmed = true;
      _clearTool();
      _approvals.clear();
      _ioApprovals.clear();
      for (final entry in entries) {
        _approvals[entry.id] = entry.approved.toSet();
        _ioApprovals[entry.id] = entry.approvedIo.toSet();
      }
      _preview = null;
      _candidatePath = null;
      _candidateTheme = null;
    });
  }

  Future<void> _guard(
    Future<void> Function(ExternalPluginControl, int) action,
    String Function(AppLocalizations) failure,
  ) async {
    if (_busy) return;
    final backend = widget.backend;
    final epoch = _epoch;
    setState(() {
      _busy = true;
      _message = null;
    });
    try {
      await _drainReleases(backend);
      if (!_current(backend, epoch)) return;
      await action(backend, epoch);
    } catch (error) {
      if (!_current(backend, epoch)) return;
      setState(() {
        _confirmed = false;
        _message = (l) =>
            '${l.pluginsUnconfirmed(failure(l))}${_themeImport ? '\n$error' : ''}';
      });
      try {
        await _loadPages(backend, epoch);
      } catch (_) {
        if (_current(backend, epoch)) {
          setState(() => _message = (l) => l.pluginsRefreshFailed(failure(l)));
        }
      }
      if (_current(backend, epoch)) widget.onChanged();
    } finally {
      if (_current(backend, epoch)) setState(() => _busy = false);
    }
  }

  Future<void> _refresh({bool reuseCatalog = false}) =>
      _guard((backend, epoch) async {
        await _drainReleases(backend);
        await _closeForm(epoch);
        if (!_current(backend, epoch)) return;
        setState(() {
          _confirmed = false;
          _clearTool();
        });
        await _loadPages(backend, epoch, reuseCatalog: reuseCatalog);
      }, (l) => l.pluginsListUnknown);

  Future<String?> _pickPackage() async {
    final selected = await openFile(
      acceptedTypeGroups: [
        XTypeGroup(
          label: L10n.of(context).pluginsPackageFile,
          extensions: ['mplugin', 'morrowplugin'],
        ),
      ],
    );
    return selected?.path;
  }

  Future<void> _inspect() => _guard((backend, epoch) async {
    if (_themeImport) {
      final file =
          await (widget.pickThemePackage?.call() ??
              openFile(
                acceptedTypeGroups: [
                  XTypeGroup(
                    label: 'Theme plugin',
                    extensions: ['morrowplugin', 'mplugin'],
                  ),
                ],
              ));
      if (file == null || !_current(backend, epoch)) return;
      final selected = await ThemePackageSelection.read(file);
      final preview = await (backend as ThemePackageImportControl)
          .inspectThemePackage(selected);
      if (!_current(backend, epoch)) return;
      if (preview.entries.length != 1) {
        throw const FormatException('Incomplete theme preview');
      }
      setState(() {
        _candidateTheme = selected;
        _candidatePath = null;
        _preview = preview;
      });
      return;
    }
    final path = await (widget.pickPackage ?? _pickPackage)();
    if (path == null || !_current(backend, epoch)) return;
    final preview = await backend.inspectPlugin(path);
    if (!_current(backend, epoch)) return;
    if (preview.entries.length != 1) throw const FormatException('插件预览不完整');
    setState(() {
      _candidatePath = path;
      _preview = preview;
    });
  }, (l) => l.pluginsInspectFailed);
  Future<void> _import() => _guard((backend, epoch) async {
    final preview = _preview;
    final path = _candidatePath;
    final theme = _candidateTheme;
    if (preview == null || (path == null && theme == null)) return;
    await _closeForm(epoch);
    if (!_current(backend, epoch)) return;
    if (theme != null) {
      await (backend as ThemePackageImportControl).importThemePackage(
        theme,
        Uint8List.fromList(preview.entries.single.digest),
        preview.revision,
      );
    } else {
      await backend.importPlugin(
        path!,
        Uint8List.fromList(preview.entries.single.digest),
        preview.revision,
      );
    }
    if (!_current(backend, epoch)) return;
    _clearTool();
    await _loadPages(backend, epoch);
    if (!_current(backend, epoch)) return;
    final alreadyEnabled = _entries.any(
      (entry) => entry.id == preview.entries.single.id && entry.enabled,
    );
    setState(
      () => _message = (l) =>
          alreadyEnabled ? l.pluginsExistingVersion : l.pluginsImportedDisabled,
    );
    widget.onChanged();
  }, (l) => l.pluginsImportUnknown);
  Future<void> _configure(PluginLibraryEntry entry, bool enable) =>
      _guard((backend, epoch) async {
        final revision = _revision!;
        final approval = enable
            ? (_approvals[entry.id] ?? {}).toList()
            : entry.approved;
        await _closeForm(epoch);
        if (!_current(backend, epoch)) return;
        await backend.configureExternal(entry, revision, approval, enable);
        if (!_current(backend, epoch)) return;
        _clearTool();
        await _loadPages(backend, epoch);
        if (_current(backend, epoch)) widget.onChanged();
      }, (l) => l.pluginsApprovalUnknown);
  Future<void> _openIo() => _guard((backend, epoch) async {
    await _closeForm(epoch);
    if (!mounted || !_current(backend, epoch)) return;
    if (widget.openIo != null) {
      await widget.openIo!();
    } else {
      await Navigator.of(context).push<void>(
        CanvasSettingsRoute<void>(
          builder: (_) =>
              IoSettingsPage(backend: backend, onChanged: widget.onChanged),
        ),
      );
    }
    if (!_current(backend, epoch)) return;
    await _loadPages(backend, epoch, reuseCatalog: true);
    if (_current(backend, epoch)) widget.onChanged();
  }, (l) => l.pluginsApprovalUnknown);

  Future<void> _configureIo(PluginLibraryEntry entry, {bool revoke = false}) =>
      _guard((backend, epoch) async {
        final revision = _revision!;
        final approval = revoke
            ? <String>[]
            : (_ioApprovals[entry.id] ?? {}).toList();
        await _closeForm(epoch);
        if (!_current(backend, epoch)) return;
        await backend.configureExternalIo(entry, revision, approval);
        if (!_current(backend, epoch)) return;
        _clearTool();
        await _loadPages(backend, epoch);
        if (_current(backend, epoch)) widget.onChanged();
      }, (l) => l.pluginsApprovalUnknown);
  Future<void> _remove(PluginLibraryEntry entry) =>
      _guard((backend, epoch) async {
        final revision = _revision!;
        await _closeForm(epoch);
        if (!_current(backend, epoch)) return;
        await backend.removeExternal(entry, revision);
        if (!_current(backend, epoch)) return;
        _clearTool();
        await _loadPages(backend, epoch);
        if (!_current(backend, epoch)) return;
        setState(() => _message = (l) => l.pluginsUninstalled);
        widget.onChanged();
      }, (l) => l.pluginsUninstallUnknown);

  bool _standardUi(PluginLibraryEntry entry) =>
      entry.handlers.any(
        (h) =>
            h.name == 'ui.form' &&
            h.inputType == 'text.utf8' &&
            h.outputType == 'morrow.ui.document.v1' &&
            h.maxInputBytes >= 32 &&
            h.maxOutputBytes == 65536,
      ) &&
      entry.handlers.any(
        (h) =>
            h.name == 'ui.edit' &&
            h.inputType == 'morrow.ui.event.v1' &&
            h.outputType == 'morrow.ui.document.v1' &&
            h.maxInputBytes >= 65536 &&
            h.maxOutputBytes == 65536,
      );
  List<PluginTransformHandler> _transforms(PluginLibraryEntry entry) => entry
      .handlers
      .where((h) => h.name != 'ui.form' && h.name != 'ui.edit')
      .toList();
  Future<void> _openForm(PluginLibraryEntry entry) =>
      _guard((backend, epoch) async {
        await _drainReleases(backend);
        if (!_current(backend, epoch)) return;
        final revision = _revision!;
        await _closeForm(epoch);
        if (!_current(backend, epoch)) return;
        final transport = _ObservedTransport(
          backend.createExternalPluginUi(entry, revision),
        );
        final controller = PluginUiController(transport);
        setState(() {
          _transport = transport;
          _controller = controller;
          _formId = entry.id;
        });
        await controller.open('');
        if (_current(backend, epoch) && controller.failure != null) {
          final failure = controller.failure!;
          setState(() => _message = (l) => pluginUiFailureMessage(l, failure));
        }
      }, (l) => l.pluginsViewFailed);
  void _clearTool() {
    _toolId = null;
    _handler = null;
    _fileBytes = null;
    _fileName = null;
    _result = null;
  }

  void _selectTool(PluginLibraryEntry entry) {
    setState(() {
      _clearTool();
      _toolId = entry.id;
      _handler = _transforms(entry).first;
      _text.clear();
    });
  }

  Future<void> _pickInput() => _guard((backend, epoch) async {
    final handler = _handler;
    if (handler == null) return;
    final file = await openFile();
    if (file == null || !_current(backend, epoch)) return;
    final limit = handler.maxInputBytes.clamp(0, 65536).toInt();
    if (await file.length() > limit) {
      if (_current(backend, epoch)) {
        setState(() => _message = (l) => l.pluginsFileLimit(limit));
      }
      return;
    }
    final builder = BytesBuilder();
    await for (final chunk in file.openRead(0, limit + 1)) {
      if (builder.length + chunk.length > limit) {
        throw const FormatException('文件超出限制');
      }
      builder.add(chunk);
    }
    if (_current(backend, epoch)) {
      setState(() {
        _fileBytes = builder.takeBytes();
        _fileName = file.name;
        _result = null;
      });
    }
  }, (l) => l.pluginsInputFailed);
  Future<void> _transform(PluginLibraryEntry entry) => _guard((
    backend,
    epoch,
  ) async {
    final handler = _handler;
    final revision = _revision;
    if (handler == null || revision == null) return;
    final input = _fileBytes ?? Uint8List.fromList(utf8.encode(_text.text));
    if (input.length > handler.maxInputBytes || input.length > 65536) {
      setState(() => _message = (l) => l.pluginsInputTooLong);
      return;
    }
    setState(() => _result = null);
    final bytes = await backend.transformExternal(
      entry,
      revision,
      handler,
      Uint8List.fromList(input),
    );
    if (!_current(backend, epoch)) return;
    if (bytes.length > handler.maxOutputBytes || bytes.length > 65536) {
      throw const FormatException('输出超出限制');
    }
    // Only the host's framing is localized; plugin output bytes/text stay original.
    String Function(AppLocalizations) preview;
    try {
      final text = utf8.decode(bytes);
      if (text.runes.any(
        (value) => value < 32 && value != 9 && value != 10 && value != 13,
      )) {
        throw const FormatException('non-text result');
      }
      final shown = String.fromCharCodes(text.runes.take(4096));
      preview = (l) => text.isEmpty
          ? l.pluginsEmptyResult
          : text.runes.length > 4096
          ? '$shown\n${l.pluginsPreviewTruncated}'
          : shown;
    } on FormatException {
      final hex = bytes
          .take(64)
          .map((b) => b.toRadixString(16).padLeft(2, '0'))
          .join(' ');
      preview = (l) =>
          l.pluginsBinaryPreview('$hex${bytes.length > 64 ? ' …' : ''}');
    }
    setState(
      () => _result = (l) => l.pluginsResultBytes(bytes.length, preview(l)),
    );
  }, (l) => l.pluginsTransformUnknown);

  String _capability(String name) =>
      {
        'summary': L10n.of(context).pluginsSummary,
        'operation': L10n.of(context).pluginsOperation,
        'attachment': L10n.of(context).pluginsAttachment,
        'create-content': L10n.of(context).pluginsCreate,
        'edit-content': L10n.of(context).pluginsEdit,
        'read-content': L10n.of(context).pluginsRead,
        'createContent': L10n.of(context).pluginsCreate,
        'editContent': L10n.of(context).pluginsEdit,
        'readContent': L10n.of(context).pluginsRead,
        'rename': L10n.of(context).pluginsRename,
        'readSummary': L10n.of(context).pluginsSummary,
        'queryOperation': L10n.of(context).pluginsOperation,
        'readAttachment': L10n.of(context).pluginsAttachment,
        'CreateContent': L10n.of(context).pluginsCreate,
        'EditContent': L10n.of(context).pluginsEdit,
        'ReadContent': L10n.of(context).pluginsRead,
        'Rename': L10n.of(context).pluginsRename,
        'ReadSummary': L10n.of(context).pluginsSummary,
        'QueryOperation': L10n.of(context).pluginsOperation,
        'ReadAttachment': L10n.of(context).pluginsAttachment,
      }[name] ??
      L10n.of(context).pluginsOtherCapability(name);
  String _ioCapability(String name) {
    final l = L10n.of(context);
    return {
          'file-read': l.pluginsIoFileRead,
          'file-list': l.pluginsIoFileList,
          'file-create': l.pluginsIoFileCreate,
          'file-replace': l.pluginsIoFileReplace,
          'file-delete': l.pluginsIoFileDelete,
          'http-request': l.pluginsIoHttpRequest,
          'http-listen': l.pluginsIoHttpListen,
          'http-publish': l.pluginsIoHttpPublish,
          'credential-use': l.pluginsIoCredentialUse,
          'websocket-connect': l.pluginsIoWebSocketConnect,
        }[name] ??
        l.pluginsOtherCapability(name);
  }

  Widget _ioPermissions(PluginLibraryEntry entry) {
    final l = L10n.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        const SizedBox(height: 12),
        Text(
          l.pluginsIoTitle,
          style: TextStyle(
            color: widget.ink,
            fontSize: 12,
            fontWeight: FontWeight.w600,
          ),
        ),
        _note(l.pluginsIoScopeNotice),
        _note(
          entry.approvedIo.isEmpty
              ? l.pluginsIoNoneApproved
              : l.pluginsIoApproved(
                  entry.approvedIo.map(_ioCapability).join(', '),
                ),
        ),
        ...entry.declaredIo.map(
          (cap) => CheckboxListTile(
            key: ValueKey('plugin-io-cap-${entry.id}-$cap'),
            dense: true,
            contentPadding: EdgeInsets.zero,
            controlAffinity: ListTileControlAffinity.leading,
            title: Text(
              _ioCapability(cap),
              style: TextStyle(color: widget.ink, fontSize: 12),
            ),
            value: _ioApprovals[entry.id]?.contains(cap) ?? false,
            onChanged: _busy || !_confirmed || !entry.available
                ? null
                : (checked) => setState(() {
                    final selected = _ioApprovals.putIfAbsent(
                      entry.id,
                      () => {},
                    );
                    checked == true ? selected.add(cap) : selected.remove(cap);
                  }),
          ),
        ),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            _button(
              l.pluginsIoSave,
              'plugin-io-save-${entry.id}',
              !_confirmed || !entry.available
                  ? null
                  : () => _configureIo(entry),
              icon: Icons.security_outlined,
            ),
            if (entry.approvedIo.isNotEmpty)
              _button(
                l.pluginsIoRevoke,
                'plugin-io-revoke-${entry.id}',
                !_confirmed ? null : () => _configureIo(entry, revoke: true),
                icon: Icons.block_outlined,
              ),
          ],
        ),
      ],
    );
  }

  Widget _note(String text) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 5),
    child: Text(
      text,
      style: TextStyle(color: widget.muted, fontSize: 11, height: 1.6),
    ),
  );
  Widget _button(
    String label,
    String key,
    VoidCallback? action, {
    IconData icon = Icons.chevron_right,
  }) => OutlinedButton.icon(
    key: ValueKey(key),
    onPressed: _busy ? null : action,
    style: OutlinedButton.styleFrom(
      foregroundColor: widget.ink,
      minimumSize: const Size(0, 38),
      side: BorderSide(color: widget.line),
      shape: RoundedRectangleBorder(borderRadius: widget.radius),
    ),
    icon: Icon(icon, size: 16),
    label: Text(label, style: const TextStyle(fontSize: 11)),
  );
  Widget _facts(PluginLibraryEntry entry) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      _note('${entry.id} · ${entry.version}'),
      if (entry.isTheme)
        _note(
          Localizations.localeOf(context).languageCode == 'zh'
              ? '主题插件 · 可叠加界面风格；启用时会停用其他主题。'
              : 'Theme plugin · combines with styles; enabling disables other themes.',
        ),
      _note(
        entry.declared.isEmpty
            ? L10n.of(context).pluginsNoPermissions
            : L10n.of(
                context,
              ).pluginsDeclared(entry.declared.map(_capability).join(', ')),
      ),
      if (entry.declaredIo.isNotEmpty)
        _note(
          L10n.of(
            context,
          ).pluginsIoDeclared(entry.declaredIo.map(_ioCapability).join(', ')),
        ),
      if (entry.dependencies.isNotEmpty) ...[
        _note(L10n.of(context).pluginsDependenciesNotice),
        ...entry.dependencies.map(_note),
      ],
      if (entry.issue.isNotEmpty) _note(entry.issue),
    ],
  );
  Widget _entry(PluginLibraryEntry entry) {
    final usable = _confirmed && entry.available && entry.enabled;
    // Keep ephemeral expansion/form state out of the settings scroll bucket.
    // Unkeyed descendant text fields must not read an expansion boolean either.
    return PageStorage(
      bucket: PageStorageBucket(),
      child: ExpansionTile(
        key: ValueKey('plugin-entry-${entry.id}'),
        tilePadding: EdgeInsets.zero,
        childrenPadding: const EdgeInsets.only(bottom: 12),
        title: Text(
          entry.name,
          style: TextStyle(color: widget.ink, fontSize: 13),
        ),
        subtitle: Text(
          entry.builtin
              ? L10n.of(context).pluginsBuiltin
              : entry.enabled
              ? L10n.of(context).pluginsEnabled
              : L10n.of(context).pluginsDisabled,
          style: TextStyle(color: widget.muted, fontSize: 11),
        ),
        children: [
          _facts(entry),
          if (entry.builtin)
            _note(L10n.of(context).pluginsManageAbove)
          else ...[
            if (entry.declared.isNotEmpty) ...[
              const SizedBox(height: 12),
              _note(L10n.of(context).pluginsContentPermissions),
            ],
            ...entry.declared.map(
              (cap) => CheckboxListTile(
                key: ValueKey('plugin-cap-${entry.id}-$cap'),
                dense: true,
                contentPadding: EdgeInsets.zero,
                controlAffinity: ListTileControlAffinity.leading,
                title: Text(
                  _capability(cap),
                  style: TextStyle(color: widget.ink, fontSize: 12),
                ),
                value: _approvals[entry.id]?.contains(cap) ?? false,
                onChanged: _busy || !_confirmed
                    ? null
                    : (checked) => setState(() {
                        final selected = _approvals.putIfAbsent(
                          entry.id,
                          () => {},
                        );
                        checked == true
                            ? selected.add(cap)
                            : selected.remove(cap);
                      }),
              ),
            ),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                if (!entry.isTheme ||
                    !entry.enabled ||
                    entry.declared.isNotEmpty)
                  _button(
                    entry.enabled
                        ? L10n.of(context).pluginsSavePermissions
                        : L10n.of(context).pluginsApproveEnable,
                    'plugin-approve-${entry.id}',
                    !_confirmed || !entry.available
                        ? null
                        : () => _configure(entry, true),
                    icon: Icons.check_circle_outline,
                  ),
                if (entry.enabled)
                  _button(
                    L10n.of(context).pluginsDisable,
                    'plugin-disable-${entry.id}',
                    !_confirmed ? null : () => _configure(entry, false),
                    icon: Icons.pause_circle_outline,
                  ),
                _button(
                  L10n.of(context).pluginsUninstallKeepContent,
                  'plugin-remove-${entry.id}',
                  !_confirmed ? null : () => _remove(entry),
                  icon: Icons.remove_circle_outline,
                ),
                if (!entry.isTheme && _transforms(entry).isNotEmpty)
                  _button(
                    L10n.of(context).pluginsUseTransform,
                    'plugin-transform-${entry.id}',
                    !usable ? null : () => _selectTool(entry),
                    icon: Icons.auto_fix_high_outlined,
                  ),
                if (_standardUi(entry))
                  _button(
                    _formId == entry.id
                        ? L10n.of(context).pluginsCloseView
                        : L10n.of(context).pluginsOpenView,
                    'plugin-ui-${entry.id}',
                    !usable
                        ? null
                        : () {
                            if (_formId == entry.id) {
                              unawaited(
                                _guard(
                                  (_, epoch) => _closeForm(epoch),
                                  (l) => l.pluginsCloseUnknown,
                                ),
                              );
                            } else {
                              unawaited(_openForm(entry));
                            }
                          },
                    icon: Icons.view_quilt_outlined,
                  ),
              ],
            ),
            if (_toolId == entry.id) ...[
              const SizedBox(height: 12),
              DropdownButton<PluginTransformHandler>(
                key: const ValueKey('plugin-handler'),
                isExpanded: true,
                value: _handler,
                items: _transforms(entry)
                    .map(
                      (handler) => DropdownMenuItem(
                        value: handler,
                        child: Text(
                          handler.name,
                          overflow: TextOverflow.ellipsis,
                        ),
                      ),
                    )
                    .toList(),
                onChanged: _busy
                    ? null
                    : (handler) => setState(() {
                        _handler = handler;
                        _result = null;
                        _fileBytes = null;
                        _fileName = null;
                      }),
              ),
              TextField(
                key: const ValueKey('plugin-input'),
                controller: _text,
                enabled: !_busy && _fileBytes == null,
                minLines: 2,
                maxLines: 5,
                decoration: InputDecoration(
                  labelText: L10n.of(context).pluginsTextInput,
                ),
                onChanged: (_) => setState(() => _result = null),
              ),
              if (_fileName != null)
                _note(L10n.of(context).pluginsSelectedFile(_fileName!)),
              Wrap(
                spacing: 8,
                children: [
                  _button(
                    L10n.of(context).pluginsChooseSmallFile,
                    'plugin-input-file',
                    _pickInput,
                    icon: Icons.attach_file,
                  ),
                  if (_fileBytes != null)
                    _button(
                      L10n.of(context).pluginsUseText,
                      'plugin-input-text',
                      () => setState(() {
                        _fileBytes = null;
                        _fileName = null;
                        _result = null;
                      }),
                    ),
                  _button(
                    L10n.of(context).pluginsTransform,
                    'plugin-run',
                    () => _transform(entry),
                  ),
                ],
              ),
              if (_result != null)
                SelectableText(
                  _result!(L10n.of(context)),
                  key: const ValueKey('plugin-result'),
                  style: TextStyle(color: widget.ink, fontSize: 12),
                ),
              _note(L10n.of(context).pluginsPreviewOnly),
            ],
            if (_formId == entry.id && _controller != null)
              Padding(
                padding: const EdgeInsets.only(top: 12),
                child: ManagedPluginForm(controller: _controller!),
              ),
          ],
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) => widget.mode == PluginLibraryMode.io
      ? _ioSettings()
      : Padding(
          padding: const EdgeInsets.all(19),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Text(
                L10n.of(context).pluginsThirdParty,
                style: TextStyle(
                  color: widget.ink,
                  fontSize: 13,
                  fontWeight: FontWeight.w600,
                ),
              ),
              const SizedBox(height: 12),
              SettingsNavigationFeedback(
                child: ListTile(
                  key: const ValueKey('io-settings-open'),
                  contentPadding: const EdgeInsets.symmetric(
                    horizontal: 12,
                    vertical: 8,
                  ),
                  shape: RoundedRectangleBorder(
                    borderRadius: widget.radius,
                    side: BorderSide(color: widget.line),
                  ),
                  title: Text(
                    L10n.of(context).mainIoSettings,
                    style: TextStyle(color: widget.ink, fontSize: 13),
                  ),
                  subtitle: Text(
                    L10n.of(context).mainIoSettingsSummary,
                    style: TextStyle(
                      color: widget.muted,
                      fontSize: 11,
                      height: 1.5,
                    ),
                  ),
                  trailing: const Icon(Icons.chevron_right_rounded),
                  onTap: _busy ? null : _openIo,
                ),
              ),
              const SizedBox(height: 16),
              _note(L10n.of(context).pluginsImportDetails),
              if (_themeImport && _confirmed)
                _note(
                  Localizations.localeOf(context).languageCode == 'zh'
                      ? '已读取此设备的插件目录。当前 Web 入口仅支持主题插件；若上次导入中断，请核对下方记录，不会自动重复导入或启用。'
                      : 'Loaded this device’s plugin catalog. Web imports support theme plugins only. After an interrupted import, check the list below; imports and activation are never retried automatically.',
                ),
              Wrap(
                spacing: 8,
                runSpacing: 8,
                children: [
                  _button(
                    _themeImport
                        ? (Localizations.localeOf(context).languageCode == 'zh'
                              ? '选择主题插件'
                              : 'Choose theme plugin')
                        : L10n.of(context).pluginsChoosePackage,
                    'plugin-pick',
                    _inspect,
                    icon: Icons.add,
                  ),
                  _button(
                    L10n.of(context).pluginsRefreshList,
                    'plugin-refresh',
                    _refresh,
                    icon: Icons.refresh,
                  ),
                ],
              ),
              if (_busy)
                const Padding(
                  padding: EdgeInsets.symmetric(vertical: 8),
                  child: LinearProgressIndicator(),
                ),
              if (_message != null) _note(_message!(L10n.of(context))),
              if (_preview != null)
                Container(
                  key: const ValueKey('plugin-preview'),
                  margin: const EdgeInsets.symmetric(vertical: 12),
                  padding: const EdgeInsets.all(12),
                  decoration: BoxDecoration(
                    border: Border.all(color: widget.line),
                    borderRadius: widget.radius,
                  ),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        L10n.of(
                          context,
                        ).pluginsImportPreview(_preview!.entries.single.name),
                        style: TextStyle(color: widget.ink),
                      ),
                      _facts(_preview!.entries.single),
                      _note(L10n.of(context).pluginsInspectedOnly),
                      Wrap(
                        spacing: 8,
                        children: [
                          _button(
                            L10n.of(context).pluginsImport,
                            'plugin-import',
                            _import,
                          ),
                          _button(
                            L10n.of(context).pluginsCancel,
                            'plugin-cancel-import',
                            () => setState(() {
                              _preview = null;
                              _candidatePath = null;
                              _candidateTheme = null;
                            }),
                          ),
                        ],
                      ),
                    ],
                  ),
                ),
              if (_confirmed && _entries.isEmpty)
                _note(L10n.of(context).pluginsEmptyLibrary),
              ..._entries.map(_entry),
            ],
          ),
        );
  Widget _ioGroup(String id, Widget child) => Glass(
    key: ValueKey('io-group:$id'),
    componentId: 'io:$id',
    p: AppearanceScope.of(context),
    child: Padding(padding: const EdgeInsets.all(20), child: child),
  );

  Widget _ioSettings() => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      _note(L10n.of(context).mainIoSettingsGuide),
      const SizedBox(height: 12),
      Align(
        alignment: Alignment.centerRight,
        child: _button(
          L10n.of(context).pluginsRefreshList,
          'plugin-refresh',
          _refresh,
          icon: Icons.refresh,
        ),
      ),
      if (_busy) const LinearProgressIndicator(),
      if (_message != null) _note(_message!(L10n.of(context))),
      const SizedBox(height: 16),
      SettingsSections(
        children: [
          _ioGroup(
            'permissions',
            Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text(
                  L10n.of(context).pluginsIoTitle,
                  style: TextStyle(
                    color: widget.ink,
                    fontWeight: FontWeight.w600,
                  ),
                ),
                if (_confirmed &&
                    !_entries.any(
                      (e) => e.declaredIo.isNotEmpty || e.approvedIo.isNotEmpty,
                    ))
                  _note(L10n.of(context).mainIoNoDeclarations),
                for (final entry in _entries.where(
                  (e) => e.declaredIo.isNotEmpty || e.approvedIo.isNotEmpty,
                ))
                  PageStorage(
                    key: ValueKey('io-permission-storage-${entry.id}'),
                    bucket: PageStorageBucket(),
                    child: ExpansionTile(
                      key: ValueKey('plugin-io-entry-${entry.id}'),
                      tilePadding: EdgeInsets.zero,
                      title: Text(entry.name),
                      children: [_ioPermissions(entry)],
                    ),
                  ),
              ],
            ),
          ),
          if (widget.backend is WorkbenchServiceControl) ...[
            _ioGroup(
              'ServiceManager',
              ServiceManager(
                backend: widget.backend as WorkbenchServiceControl,
                plugins: _confirmed ? _entries : const [],
                registryRevision: _confirmed ? _revision : null,
                ink: widget.ink,
                muted: widget.muted,
                line: widget.line,
                radius: widget.radius,
              ),
            ),
          ],
          if (widget.backend is WorkbenchServiceRunControl &&
              widget.backend is WorkbenchServiceControl &&
              widget.backend is WorkbenchIoTaskControl) ...[
            _ioGroup(
              'ServiceRunManager',
              ServiceRunManager(
                backend: widget.backend as WorkbenchServiceRunControl,
                ioBackend: widget.backend as WorkbenchIoTaskControl,
                metadataBackend: widget.backend as WorkbenchServiceControl,
                endpointBackend: widget.backend is WorkbenchEndpointControl
                    ? widget.backend as WorkbenchEndpointControl
                    : null,
                plugins: _confirmed ? _entries : const [],
                registryRevision: _confirmed ? _revision : null,
                ink: widget.ink,
                muted: widget.muted,
                line: widget.line,
                radius: widget.radius,
                onChanged: widget.onChanged,
              ),
            ),
          ],
          if (widget.backend is WorkbenchIoTaskControl &&
              widget.backend is WorkbenchEndpointControl) ...[
            _ioGroup(
              'HttpTaskManager',
              HttpTaskManager(
                backend: widget.backend as WorkbenchIoTaskControl,
                serviceSession: widget.backend is WorkbenchServiceRunControl
                    ? ServiceRunSession.forBackend(
                        widget.backend as WorkbenchServiceRunControl,
                        widget.backend as WorkbenchIoTaskControl,
                      )
                    : null,
                endpointBackend: widget.backend as WorkbenchEndpointControl,
                plugins: _confirmed ? _entries : const [],
                registryRevision: _confirmed ? _revision : null,
                ink: widget.ink,
                muted: widget.muted,
                line: widget.line,
                radius: widget.radius,
                onChanged: widget.onChanged,
              ),
            ),
          ],
          if (widget.backend is WorkbenchCredentialControl) ...[
            _ioGroup(
              'CredentialManager',
              CredentialManager(
                backend: widget.backend as WorkbenchCredentialControl,
                ink: widget.ink,
                muted: widget.muted,
                line: widget.line,
                radius: widget.radius,
              ),
            ),
          ],
          if (widget.backend is WorkbenchEndpointControl &&
              _confirmed &&
              _revision != null) ...[
            _ioGroup(
              'EndpointManager',
              EndpointManager(
                backend: widget.backend as WorkbenchEndpointControl,
                plugins: _entries,
                registryRevision: _revision!,
                credentialBackend: widget.backend is WorkbenchCredentialControl
                    ? widget.backend as WorkbenchCredentialControl
                    : null,
                ink: widget.ink,
                muted: widget.muted,
                line: widget.line,
                radius: widget.radius,
              ),
            ),
          ],
        ],
      ),
    ],
  );
}
