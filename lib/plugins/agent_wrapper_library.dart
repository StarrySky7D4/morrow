import 'dart:async';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'agent_wrapper_control.dart';
import 'agent_wrapper_models.dart';

/// Trusted management only. No task execution or automatic activation here.
final class AgentWrapperLibrary extends StatefulWidget {
  const AgentWrapperLibrary({
    super.key,
    required this.backend,
    required this.onChanged,
    required this.ink,
    required this.muted,
    required this.line,
    required this.radius,
    this.pickPackage,
  });
  final AgentWrapperControl backend;
  final VoidCallback onChanged;
  final Color ink, muted, line;
  final BorderRadius radius;
  final Future<String?> Function()? pickPackage;
  @override
  State<AgentWrapperLibrary> createState() => _AgentWrapperLibraryState();
}

final class _AgentWrapperLibraryState extends State<AgentWrapperLibrary> {
  AgentWrapperRevisions? _revisions;
  AgentWrapperReview? _preview;
  String? _path, _message;
  final _entries = <AgentWrapperEntry>[];
  final _drafts = <String, AgentWrapperApproval>{};
  String _cursor = '';
  int _epoch = 0;
  bool _busy = false, _selecting = false, _confirmed = false, _unknown = false;
  String tr(String zh, String en) =>
      Localizations.localeOf(context).languageCode == 'zh' ? zh : en;
  bool get _canSubmit =>
      !_busy &&
      _revisions != null &&
      !_unknown &&
      !widget.backend.wrapperOutcomeUnknown;
  bool get _writeable => _canSubmit && _confirmed;
  bool _current(AgentWrapperControl backend, int epoch) =>
      mounted && identical(backend, widget.backend) && epoch == _epoch;
  @override
  void initState() {
    super.initState();
    unawaited(_state());
  }

  @override
  void didUpdateWidget(covariant AgentWrapperLibrary oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.backend, widget.backend)) {
      _epoch++;
      _busy = false;
      _selecting = false;
      _confirmed = false;
      _unknown = false;
      _revisions = null;
      _preview = null;
      _path = null;
      _cursor = '';
      _message = null;
      _entries.clear();
      _drafts.clear();
      unawaited(_state());
    }
  }

  @override
  void dispose() {
    _epoch++;
    super.dispose();
  }

  String _failure(Object error) {
    final kind = error is AgentWrapperFailure
        ? error.kind
        : widget.backend.wrapperOutcomeUnknown
        ? AgentWrapperFailureKind.unknown
        : AgentWrapperFailureKind.invalid;
    return switch (kind) {
      AgentWrapperFailureKind.busy => tr(
        '当前宿主忙碌。操作没有自动重试。',
        'The host is busy. This operation was not retried.',
      ),
      AgentWrapperFailureKind.lost => tr(
        '原拥有者已失效。请关闭并重新打开工作台，再重新审查。',
        'The original owner is no longer available. Close and reopen the workbench, then review again.',
      ),
      AgentWrapperFailureKind.unknown => tr(
        '结果未确认。只可查询事实；请由原拥有者恢复或重新打开工作台后重新审查，不能重复提交。',
        'The outcome is unconfirmed. Read current facts only; recover the original owner or reopen the workbench and review again before submitting.',
      ),
      AgentWrapperFailureKind.conflict => tr(
        '目录已变化。请重新查询并审查。',
        'The catalog changed. Read and review it again.',
      ),
      AgentWrapperFailureKind.denied => tr(
        '此范围未获允许。',
        'This scope is not allowed.',
      ),
      AgentWrapperFailureKind.limit => tr(
        '已达到包或目录上限。',
        'The package or catalog limit was reached.',
      ),
      AgentWrapperFailureKind.unavailable => tr(
        '此入口不支持完整会话插件管理。',
        'This entry does not support session plugin management.',
      ),
      AgentWrapperFailureKind.invalid => tr(
        '无法核验包或回执。',
        'The package or receipt could not be verified.',
      ),
    };
  }

  Future<void> _guard(
    Future<AgentWrapperResult> Function(AgentWrapperControl) action, {
    bool mutation = false,
    bool previewMutation = false,
    void Function(AgentWrapperResult)? received,
  }) async {
    if (_busy ||
        mutation &&
            !(previewMutation
                ? _canSubmit && _preview != null && _path != null
                : _writeable)) {
      return;
    }
    final backend = widget.backend, epoch = _epoch;
    setState(() {
      _busy = true;
      _message = null;
    });
    try {
      final result = await action(backend);
      if (!_current(backend, epoch)) return;
      setState(() {
        if (_revisions == null || !_revisions!.same(result.revisions)) {
          _drafts.clear();
          _confirmed = false;
          _preview = null;
          _path = null;
        }
        _revisions = result.revisions;
        if (mutation) {
          _confirmed = false;
          _drafts.clear();
          _preview = null;
          _path = null;
          _message = tr(
            '操作已确认。请查询列表后再进行下一次操作。',
            'The operation is confirmed. Read the list before taking the next action.',
          );
        }
        received?.call(result);
      });
      if (mutation) widget.onChanged();
    } catch (error) {
      if (!_current(backend, epoch)) return;
      setState(() {
        _confirmed = false;
        _drafts.clear();
        _preview = null;
        _path = null;
        if (backend.wrapperOutcomeUnknown ||
            error is AgentWrapperFailure &&
                (error.kind == AgentWrapperFailureKind.unknown ||
                    error.kind == AgentWrapperFailureKind.lost)) {
          _unknown = true;
        }
        _message = _failure(error);
      });
    } finally {
      if (_current(backend, epoch)) setState(() => _busy = false);
    }
  }

  Future<void> _state() => _guard(
    (backend) => backend.wrapperState(),
    received: (_) {
      _confirmed = false;
      _cursor = '';
      _message = tr(
        '已读取目录状态。查询列表以核对当前包。',
        'Catalog state received. Read the list to check current packages.',
      );
    },
  );
  Future<void> _page({bool next = false}) async {
    final revisions = _revisions;
    if (revisions == null) return;
    final cursor = next ? _cursor : '';
    await _guard(
      (backend) => backend.wrapperPage(revisions, cursor: cursor),
      received: (result) {
        final page = result.page;
        if (page == null || !page.revisions.same(revisions)) {
          throw const FormatException('Missing wrapper page');
        }
        if (!next) _entries.clear();
        if (_entries.length + page.entries.length > 64 ||
            page.entries.any(
              (entry) =>
                  _entries.any((old) => old.review.key == entry.review.key),
            )) {
          throw const FormatException('Duplicate or oversized catalog');
        }
        _entries.addAll(page.entries);
        _cursor = page.cursor;
        _confirmed = true;
        _drafts.clear();
      },
    );
  }

  Future<void> _inspect() async {
    if (_busy || _selecting || _revisions == null) return;
    final backend = widget.backend, epoch = _epoch;
    String? path;
    setState(() => _selecting = true);
    try {
      path =
          await (widget.pickPackage?.call() ??
              () async {
                final file = await openFile(
                  acceptedTypeGroups: [
                    const XTypeGroup(
                      label: 'Session plugin',
                      extensions: ['magent', 'morrowagent', 'mrowasp'],
                    ),
                  ],
                );
                return file?.path;
              }());
    } catch (error) {
      if (_current(backend, epoch)) setState(() => _message = _failure(error));
      return;
    } finally {
      if (_current(backend, epoch)) setState(() => _selecting = false);
    }
    if (path == null || !_current(backend, epoch)) return;
    final selectedPath = path;
    await _guard(
      (b) => b.inspectWrapper(selectedPath, _revisions!),
      received: (result) {
        if (result.review == null) {
          throw const FormatException('Missing wrapper preview');
        }
        _preview = result.review;
        _path = selectedPath;
      },
    );
  }

  AgentWrapperApproval _draft(AgentWrapperEntry entry) =>
      _drafts[entry.review.key] ??
      entry.approved ??
      AgentWrapperApproval(
        sessionBits: 1,
        processBits: 0,
        sessions: entry.review.declaration.sessions,
        executionDomain: entry.review.declaration.executionDomain,
      );
  void _setDraft(
    AgentWrapperEntry entry, {
    int? session,
    int? process,
    List<String>? sessions,
  }) {
    final old = _draft(entry);
    setState(
      () => _drafts[entry.review.key] = AgentWrapperApproval(
        sessionBits: session ?? old.sessionBits,
        processBits: process ?? old.processBits,
        sessions: sessions ?? old.sessions,
        executionDomain: old.executionDomain,
      ),
    );
  }

  Widget _note(String text) => Padding(
    padding: const EdgeInsets.only(top: 6),
    child: Text(
      text,
      style: TextStyle(color: widget.muted, fontSize: 12, height: 1.5),
    ),
  );
  Widget _button(String text, String key, VoidCallback? action) =>
      OutlinedButton(
        key: ValueKey(key),
        onPressed: _busy || _selecting ? null : action,
        style: OutlinedButton.styleFrom(
          foregroundColor: widget.ink,
          side: BorderSide(color: widget.line),
          shape: RoundedRectangleBorder(borderRadius: widget.radius),
        ),
        child: Text(text),
      );
  List<String> get _sessionLabels => [
    tr('读取会话（必需）', 'Read sessions (required)'),
    tr('写入会话', 'Write sessions'),
    tr('提交执行提案', 'Propose execution'),
    tr('使用批准的执行', 'Use approved execution'),
    tr('退役执行', 'Retire execution'),
  ];
  List<String> get _processLabels => [
    tr('读取输出', 'Read output'),
    tr('读取进程事件', 'Read process events'),
    tr('写入输入', 'Write input'),
    tr('关闭输入', 'Close input'),
    tr('中断', 'Interrupt'),
    tr('终止', 'Terminate'),
    tr('调整终端大小', 'Resize terminal'),
  ];
  String _declaredNames(int bits, List<String> names) => [
    for (var i = 0; i < names.length; i++)
      if (bits & (1 << i) != 0) names[i],
  ].join(', ');
  Widget _review(AgentWrapperReview review) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Text(
        '${review.id} · ${review.version}',
        style: TextStyle(color: widget.ink, fontWeight: FontWeight.w600),
      ),
      _note(
        tr('执行范围：', 'Execution domain: ') + review.declaration.executionDomain,
      ),
      _note(
        tr('声明会话：', 'Declared sessions: ') +
            review.declaration.sessions.join(', '),
      ),
      _note(
        tr('声明的会话能力（只读）：', 'Declared session capabilities (read only): ') +
            _declaredNames(review.declaration.sessionBits, _sessionLabels),
      ),
      _note(
        tr('声明的进程能力（只读）：', 'Declared process capabilities (read only): ') +
            (review.declaration.processBits == 0
                ? tr('无', 'None')
                : _declaredNames(
                    review.declaration.processBits,
                    _processLabels,
                  )),
      ),
      ExpansionTile(
        tilePadding: EdgeInsets.zero,
        title: Text(
          tr('包摘要与协议', 'Package digests and protocols'),
          style: TextStyle(color: widget.muted, fontSize: 12),
        ),
        children: [
          for (final pair in [
            (tr('完整包 SHA-256', 'Full package SHA-256'), review.wrapperSha256),
            (tr('基础包 SHA-256', 'Base package SHA-256'), review.baseSha256),
            (
              tr('会话 schema SHA-256', 'Session schema SHA-256'),
              review.sessionSchema,
            ),
            (
              tr('进程 schema SHA-256', 'Process schema SHA-256'),
              review.processSchema,
            ),
          ])
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: SelectableText(
                '${pair.$1}\n${AgentWrapperValidation.hex(pair.$2)}',
                style: TextStyle(color: widget.muted, fontSize: 11),
              ),
            ),
        ],
      ),
    ],
  );
  Widget _entry(AgentWrapperEntry entry) {
    final r = entry.review, declaration = r.declaration, draft = _draft(entry);
    final editable = _writeable && entry.selected && entry.baseSelected;
    final sc = _sessionLabels, pc = _processLabels;
    return ExpansionTile(
      key: ValueKey('agent-wrapper-${r.key}'),
      tilePadding: EdgeInsets.zero,
      title: Text(r.id, style: TextStyle(color: widget.ink, fontSize: 13)),
      subtitle: Text(
        entry.enabled
            ? tr('完整包已启用', 'Wrapper enabled')
            : entry.approved != null
            ? tr('已批准，尚未启用', 'Approved, disabled')
            : tr('尚未批准', 'Not approved'),
        style: TextStyle(color: widget.muted, fontSize: 11),
      ),
      children: [
        _review(r),
        _note(
          entry.baseSelected
              ? entry.baseEnabled
                    ? tr('基础包已选择并启用。', 'Base package selected and enabled.')
                    : tr('基础包已选择，但已停用。', 'Base package selected, disabled.')
              : tr('此基础包尚未选择。', 'This base package is not selected.'),
        ),
        _note(
          entry.selected
              ? tr('已选择此完整包版本。', 'This wrapper version is selected.')
              : tr('此完整包尚未选择。', 'This wrapper is not selected.'),
        ),
        if (entry.approved != null)
          _note(
            tr('已批准会话：', 'Approved sessions: ') +
                entry.approved!.sessions.join(', '),
          ),
        _note(
          tr(
            '勾选的是待批准子集；批准后仍需单独启用。',
            'Checkboxes edit a proposed subset. Approval still requires separate activation.',
          ),
        ),
        for (var i = 0; i < sc.length; i++)
          if (declaration.sessionBits & (1 << i) != 0)
            CheckboxListTile(
              key: ValueKey('agent-session-${r.key}-$i'),
              dense: true,
              contentPadding: EdgeInsets.zero,
              controlAffinity: ListTileControlAffinity.leading,
              title: Text(
                sc[i],
                style: TextStyle(color: widget.ink, fontSize: 12),
              ),
              value: draft.sessionBits & (1 << i) != 0,
              onChanged: !editable || i == 0
                  ? null
                  : (v) => _setDraft(
                      entry,
                      session: v == true
                          ? draft.sessionBits | (1 << i)
                          : draft.sessionBits & ~(1 << i),
                    ),
            ),
        for (var i = 0; i < pc.length; i++)
          if (declaration.processBits & (1 << i) != 0)
            CheckboxListTile(
              key: ValueKey('agent-process-${r.key}-$i'),
              dense: true,
              contentPadding: EdgeInsets.zero,
              controlAffinity: ListTileControlAffinity.leading,
              title: Text(
                pc[i],
                style: TextStyle(color: widget.ink, fontSize: 12),
              ),
              value: draft.processBits & (1 << i) != 0,
              onChanged: !editable
                  ? null
                  : (v) => _setDraft(
                      entry,
                      process: v == true
                          ? draft.processBits | (1 << i)
                          : draft.processBits & ~(1 << i),
                    ),
            ),
        for (final scope in declaration.sessions)
          CheckboxListTile(
            key: ValueKey('agent-scope-${r.key}-$scope'),
            dense: true,
            contentPadding: EdgeInsets.zero,
            controlAffinity: ListTileControlAffinity.leading,
            title: Text(
              scope,
              style: TextStyle(color: widget.ink, fontSize: 12),
            ),
            value: draft.sessions.contains(scope),
            onChanged:
                !editable ||
                    draft.sessions.length == 1 && draft.sessions.contains(scope)
                ? null
                : (v) {
                    final values = draft.sessions.toSet();
                    v == true ? values.add(scope) : values.remove(scope);
                    _setDraft(entry, sessions: values.toList()..sort());
                  },
          ),
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            if (!entry.baseSelected)
              _button(
                tr('选择基础包', 'Select base package'),
                'agent-base-select-${r.key}',
                !_writeable
                    ? null
                    : () => unawaited(
                        _guard(
                          (b) => b.selectWrapperBase(r, _revisions!),
                          mutation: true,
                        ),
                      ),
              ),
            if (entry.baseSelected)
              _button(
                entry.baseEnabled
                    ? tr('停用基础包', 'Disable base')
                    : tr('启用基础包', 'Enable base'),
                'agent-base-enable-${r.key}',
                !_writeable
                    ? null
                    : () => unawaited(
                        _guard(
                          (b) => b.enableWrapperBase(
                            r,
                            _revisions!,
                            !entry.baseEnabled,
                          ),
                          mutation: true,
                        ),
                      ),
              ),
            if (!entry.selected)
              _button(
                tr('选择完整包', 'Select wrapper'),
                'agent-select-${r.key}',
                !_writeable || !entry.baseSelected
                    ? null
                    : () => unawaited(
                        _guard(
                          (b) => b.selectWrapper(r, _revisions!),
                          mutation: true,
                        ),
                      ),
              ),
            _button(
              tr('批准所选权限', 'Approve selected permissions'),
              'agent-approve-${r.key}',
              !editable
                  ? null
                  : () => unawaited(
                      _guard(
                        (b) => b.approveWrapper(r, _revisions!, draft),
                        mutation: true,
                      ),
                    ),
            ),
            _button(
              entry.enabled
                  ? tr('停用完整包', 'Disable wrapper')
                  : tr('启用完整包', 'Enable wrapper'),
              'agent-enable-${r.key}',
              !_writeable ||
                      !entry.selected ||
                      !entry.baseSelected ||
                      !entry.enabled &&
                          (entry.approved == null || !entry.baseEnabled)
                  ? null
                  : () => unawaited(
                      _guard(
                        (b) => b.enableWrapper(r, _revisions!, !entry.enabled),
                        mutation: true,
                      ),
                    ),
            ),
            _button(
              tr('移除选择', 'Remove selection'),
              'agent-remove-${r.key}',
              !_writeable || !entry.selected || !entry.baseSelected
                  ? null
                  : () => unawaited(
                      _guard(
                        (b) => b.removeWrapper(r, _revisions!),
                        mutation: true,
                      ),
                    ),
            ),
          ],
        ),
        _note(
          tr(
            '能力声明不保证底层提供者支持每项操作；启用不会运行任务。',
            'The provider may not support every declared capability. Activation does not run a task.',
          ),
        ),
      ],
    );
  }

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      Text(
        tr('会话与安全执行插件', 'Session and safe execution plugins'),
        style: TextStyle(
          color: widget.ink,
          fontSize: 13,
          fontWeight: FontWeight.w600,
        ),
      ),
      _note(
        tr(
          '完整包的权限需要独立审查。安装、选择、批准和启用是分开的操作。',
          'Review permissions for the complete package. Installation, selection, approval and activation are separate actions.',
        ),
      ),
      Wrap(
        spacing: 8,
        runSpacing: 8,
        children: [
          _button(
            tr('查询状态', 'Read state'),
            'agent-state',
            () => unawaited(_state()),
          ),
          _button(
            tr('查询列表', 'Read list'),
            'agent-page',
            _revisions == null ? null : () => unawaited(_page()),
          ),
          _button(
            tr('选择完整包文件', 'Choose full package'),
            'agent-pick',
            _revisions == null ? null : () => unawaited(_inspect()),
          ),
          if (_cursor.isNotEmpty)
            _button(
              tr('下一页', 'Next page'),
              'agent-next',
              !_confirmed ? null : () => unawaited(_page(next: true)),
            ),
        ],
      ),
      if (_busy)
        const Padding(
          padding: EdgeInsets.symmetric(vertical: 8),
          child: LinearProgressIndicator(),
        ),
      if (_message != null) _note(_message!),
      if (_unknown || widget.backend.wrapperOutcomeUnknown)
        _note(
          tr(
            '查询不会解除禁止提交状态。请恢复原拥有者或重新打开工作台后重新审查。',
            'Reading facts does not unlock submissions. Recover the original owner or reopen the workbench and review again.',
          ),
        ),
      if (_preview != null)
        Container(
          key: const ValueKey('agent-preview'),
          margin: const EdgeInsets.symmetric(vertical: 12),
          padding: const EdgeInsets.all(12),
          decoration: BoxDecoration(
            border: Border.all(color: widget.line),
            borderRadius: widget.radius,
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              _review(_preview!),
              _note(
                tr(
                  '仅完成审查，尚未安装或批准。',
                  'Reviewed only; not installed or approved.',
                ),
              ),
              Wrap(
                spacing: 8,
                children: [
                  _button(
                    tr('安装完整包', 'Install wrapper'),
                    'agent-install',
                    !_canSubmit
                        ? null
                        : () {
                            final path = _path!,
                                review = _preview!,
                                revisions = _revisions!;
                            unawaited(
                              _guard(
                                (b) =>
                                    b.installWrapper(path, review, revisions),
                                mutation: true,
                                previewMutation: true,
                              ),
                            );
                          },
                  ),
                  _button(
                    tr('取消', 'Cancel'),
                    'agent-cancel',
                    () => setState(() {
                      _preview = null;
                      _path = null;
                    }),
                  ),
                ],
              ),
            ],
          ),
        ),
      if (!_confirmed && _entries.isNotEmpty)
        _note(
          tr(
            '下列为上次观察结果；请查询列表后再操作。',
            'These are earlier observations. Read the list before taking an action.',
          ),
        ),
      for (final entry in _entries) _entry(entry),
    ],
  );
}
