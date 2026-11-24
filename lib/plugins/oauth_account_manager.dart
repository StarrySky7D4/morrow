import 'dart:typed_data';

import 'package:material_ui/material_ui.dart';
import 'package:url_launcher/url_launcher.dart';

import 'oauth_accounts.dart';
import 'plugin_library.dart' show PluginLibraryEntry;

class OAuthAccountManager extends StatefulWidget {
  const OAuthAccountManager({
    super.key,
    required this.backend,
    required this.entries,
    required this.revision,
    this.openBrowser,
  });
  final WorkbenchOAuthControl backend;
  final List<PluginLibraryEntry> entries;
  final BigInt revision;
  final Future<bool> Function(Uri)? openBrowser;
  @override
  State<OAuthAccountManager> createState() => _OAuthManagerState();
}

class _OAuthManagerState extends State<OAuthAccountManager> {
  PluginLibraryEntry? _plugin;
  List<OAuthService> _services = [];
  OAuthService? _service;
  List<OAuthEntry> _accounts = [];
  Uint8List? _key;
  int _generation = 0;
  bool _busy = false;
  String _message = '';
  String t(String en, String zh) =>
      Localizations.localeOf(context).languageCode == 'zh' ? zh : en;
  bool current(int g) => mounted && g == _generation;
  bool get active =>
      _plugin != null &&
      _plugin!.enabled &&
      ['http-request', 'credential-use'].every(_plugin!.approvedIo.contains);
  Future<OAuthResult> call(OAuthCommand command) =>
      widget.backend.accountControl(_plugin!, widget.revision, command);
  @override
  void didUpdateWidget(covariant OAuthAccountManager old) {
    super.didUpdateWidget(old);
    if (old.backend != widget.backend || old.revision != widget.revision) {
      _generation++;
      _plugin = null;
      _service = null;
      _key = null;
      _accounts = [];
      _services = [];
      _busy = false;
      _message = '';
    }
  }

  Future<void> selectPlugin(PluginLibraryEntry row) async {
    final g = ++_generation;
    setState(() {
      _plugin = row;
      _services = [];
      _service = null;
      _accounts = [];
      _busy = true;
      _message = '';
    });
    try {
      final result = await call(const OAuthCommand(OAuthAction.services));
      if (current(g))
        setState(() {
          _services = result.services;
          _message = result.services.isEmpty
              ? t('No approved services for this plugin.', '此插件尚无已批准的服务。')
              : '';
        });
    } catch (_) {
      if (current(g))
        setState(
          () =>
              _message = t('Could not load approved services.', '未能读取已批准的服务。'),
        );
    } finally {
      if (current(g)) setState(() => _busy = false);
    }
  }

  Future<void> loadAccounts(int g, OAuthService s) async {
    final rows = await readOAuthAccounts(
      widget.backend,
      _plugin!,
      widget.revision,
      s,
      () => current(g),
    );
    if (current(g)) setState(() => _accounts = rows);
  }

  Future<void> selectService(OAuthService s) async {
    final g = ++_generation;
    setState(() {
      _service = s;
      _accounts = [];
      _busy = true;
      _message = '';
    });
    try {
      await loadAccounts(g, s);
    } catch (_) {
      if (current(g))
        setState(() => _message = t('Could not load accounts.', '未能读取账户。'));
    } finally {
      if (current(g)) setState(() => _busy = false);
    }
  }

  Future<bool> review(String title, String body, String action) async =>
      await showDialog<bool>(
        context: context,
        builder: (ctx) => AlertDialog(
          title: Text(title),
          content: SingleChildScrollView(child: Text(body)),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(ctx, false),
              child: Text(t('Cancel', '取消')),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(ctx, true),
              child: Text(action),
            ),
          ],
        ),
      ) ??
      false;
  Future<void> perform(OAuthAction op, [OAuthEntry? row]) async {
    final g = _generation, s = _service;
    if (s == null || _busy) return;
    final name = _plugin!.name;
    setState(() => _busy = true);
    try {
      if (op == OAuthAction.login) {
        final body = [
          name,
          s.name,
          t('Issuer', '签发方') + ' ' + s.issuer.toString(),
          t('Browser destination', '浏览器地址') + ' ' + s.authorize.toString(),
          t('Resource', '访问目标') + ' ' + s.resource.toString(),
          t('Permissions', '权限') + ' ' + s.scopes,
          t('Local approval lasts 7 days.', '本机批准有效期为 7 天。'),
        ].join('\n');
        if (!await review(
              t('Review account sign-in', '确认账户登录'),
              body,
              t('Open browser', '打开浏览器'),
            ) ||
            !current(g))
          return;
      }
      if (op == OAuthAction.logout &&
          (!await review(
                t('Sign out this account?', '退出此账户？'),
                [name, s.name, accountHex(row!.reference)].join('\n'),
                t('Sign out', '退出账户'),
              ) ||
              !current(g)))
        return;
      if (op == OAuthAction.login || op == OAuthAction.refresh) {
        setState(() => _key = newOAuthKey());
      }
      final cmd = OAuthCommand(
        op,
        service: s.key,
        account: row?.reference,
        revision: row?.revision,
        key: _key,
        days: op == OAuthAction.login ? 7 : 0,
      );
      final view = await call(cmd);
      if (!current(g)) return;
      if (op == OAuthAction.login) {
        final uri = s.browser(view.browser);
        final opened =
            await (widget.openBrowser?.call(uri) ??
                launchUrl(uri, mode: LaunchMode.externalApplication));
        if (!current(g)) return;
        if (!opened) {
          setState(
            () => _message = t(
              'Browser did not open. Cancel this sign-in before starting again.',
              '浏览器未打开。请先取消此次登录。',
            ),
          );
          return;
        }
      }
      if (view.phase == OAuthPhase.waiting) {
        setState(
          () => _message = t(
            'Waiting for the original request. Check status when ready.',
            '正在等待本次请求。完成后请查看状态。',
          ),
        );
      } else {
        final message = switch (view.phase) {
          OAuthPhase.unconfirmed => t(
            'Outcome unconfirmed. Review the account before signing in again.',
            '结果待核对。请查看账户状态后再登录。',
          ),
          OAuthPhase.signInRequired => t(
            'Sign in again to authorize this account.',
            '此账户需要重新登录授权。',
          ),
          OAuthPhase.cancelled => t('Cancelled.', '已取消。'),
          _ => t('Completed.', '已完成。'),
        };
        setState(() {
          _key = null;
          _message = message;
        });
        await loadAccounts(g, s);
      }
    } catch (_) {
      if (current(g))
        setState(
          () => _message = t(
            'Outcome unconfirmed. Check the original request or reload accounts.',
            '结果待核对。请查看本次请求或重新读取账户。',
          ),
        );
    } finally {
      if (current(g)) setState(() => _busy = false);
    }
  }

  String state(OAuthEntry row) => switch (row.state) {
    OAuthState.available =>
      row.accessExpiresMs > DateTime.now().millisecondsSinceEpoch
          ? t('Access available', '可访问')
          : t('Access expired', '访问已到期'),
    OAuthState.refreshing => t(
      'Refresh outcome pending. Check the original request or sign out.',
      '刷新结果待核对。请查看原请求或退出账户。',
    ),
    OAuthState.signInRequired => t('Sign-in required', '需要登录'),
    OAuthState.signedOut => t('Signed out', '已退出'),
  };
  @override
  Widget build(BuildContext context) {
    final plugins = widget.entries
        .where(
          (e) =>
              e.declaredIo.contains('http-request') &&
              e.declaredIo.contains('credential-use'),
        )
        .toList();
    if (plugins.isEmpty) return const SizedBox.shrink();
    final locked = _busy || _key != null;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          t('Service accounts', '服务账户'),
          style: Theme.of(context).textTheme.titleMedium,
        ),
        Wrap(
          spacing: 8,
          children: [
            for (final e in plugins)
              TextButton(
                onPressed: locked ? null : () => selectPlugin(e),
                child: Text(e.name),
              ),
          ],
        ),
        if (_plugin != null)
          Wrap(
            spacing: 8,
            children: [
              for (final s in _services)
                OutlinedButton(
                  onPressed: locked ? null : () => selectService(s),
                  child: Text(s.name),
                ),
            ],
          ),
        if (_service != null) ...[
          Text(_service!.issuer.toString()),
          if (_key == null)
            Wrap(
              spacing: 8,
              children: [
                FilledButton(
                  onPressed: !active || _busy
                      ? null
                      : () => perform(OAuthAction.login),
                  child: Text(t('Sign in', '登录账户')),
                ),
                TextButton(
                  onPressed: _busy ? null : () => selectService(_service!),
                  child: Text(t('Reload accounts', '重新读取账户')),
                ),
              ],
            ),
          if (_key != null)
            Wrap(
              spacing: 8,
              children: [
                TextButton(
                  onPressed: _busy ? null : () => perform(OAuthAction.poll),
                  child: Text(t('Check status', '查看状态')),
                ),
                TextButton(
                  onPressed: _busy ? null : () => perform(OAuthAction.cancel),
                  child: Text(t('Cancel request', '取消请求')),
                ),
              ],
            ),
          for (final row in _accounts)
            Padding(
              padding: const EdgeInsets.only(top: 8),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  SelectableText(accountHex(row.reference)),
                  Text(state(row)),
                  Wrap(
                    spacing: 8,
                    children: [
                      if (row.state == OAuthState.available)
                        TextButton(
                          onPressed: locked || !active
                              ? null
                              : () => perform(OAuthAction.refresh, row),
                          child: Text(t('Refresh access', '刷新授权')),
                        ),
                      if (row.state != OAuthState.signedOut)
                        TextButton(
                          onPressed: locked
                              ? null
                              : () => perform(OAuthAction.logout, row),
                          child: Text(t('Sign out', '退出账户')),
                        ),
                    ],
                  ),
                ],
              ),
            ),
        ],
        if (_message.isNotEmpty) Text(_message),
      ],
    );
  }
}
