import 'dart:math' show Random;
import 'dart:typed_data';

import 'generated/host.capnp.dart' as host;
import 'plugin_library.dart' show PluginLibraryEntry;

abstract interface class WorkbenchOAuthControl {
  bool get supportsOAuthAccounts;
  Future<OAuthResult> accountControl(
    PluginLibraryEntry entry,
    BigInt revision,
    OAuthCommand command,
  );
}

enum OAuthAction { services, accounts, login, refresh, poll, cancel, logout }

enum OAuthState { available, refreshing, signInRequired, signedOut }

enum OAuthPhase { waiting, done, cancelled, signInRequired, unconfirmed }

String accountHex(List<int> id) =>
    id.map((v) => v.toRadixString(16).padLeft(2, '0')).join();
Uint8List _id(Uint8List? bytes) {
  if (bytes == null || bytes.length != 32 || bytes.every((v) => v == 0))
    throw const FormatException('Invalid account identity');
  return Uint8List.fromList(bytes);
}

Uint8List newOAuthKey() {
  final random = Random.secure();
  return Uint8List.fromList(List.generate(32, (_) => random.nextInt(256)));
}

Uri _endpoint(String? text) {
  final uri = Uri.tryParse(text ?? '');
  if (text == null ||
      text.length > 2048 ||
      uri == null ||
      uri.scheme != 'https' ||
      uri.host.isEmpty ||
      uri.userInfo.isNotEmpty ||
      uri.hasQuery ||
      uri.hasFragment) {
    throw const FormatException('Invalid reviewed account endpoint');
  }
  return uri;
}

class OAuthService {
  const OAuthService(
    this.key,
    this.name,
    this.issuer,
    this.authorize,
    this.resource,
    this.scopes,
  );
  final Uint8List key;
  final String name, scopes;
  final Uri issuer, authorize, resource;
  Uri browser(String raw) {
    final value = Uri.tryParse(raw);
    if (raw.length > 8192 ||
        value == null ||
        value.scheme != 'https' ||
        value.userInfo.isNotEmpty ||
        value.hasFragment ||
        value.origin != authorize.origin ||
        value.path != authorize.path)
      throw const FormatException('Unexpected sign-in destination');
    final p = value.queryParametersAll;
    final redirect = Uri.tryParse(p['redirect_uri']?.singleOrNull ?? '');
    if (p.values.any((items) => items.length != 1) ||
        p['response_type']?.singleOrNull != 'code' ||
        p['code_challenge_method']?.singleOrNull != 'S256' ||
        !(p['client_id']?.singleOrNull?.isNotEmpty ?? false) ||
        !RegExp(r'^[A-Za-z0-9_-]{43}$')
            .hasMatch(p['code_challenge']?.singleOrNull ?? '') ||
        !RegExp(r'^[A-Za-z0-9_-]{43}$')
            .hasMatch(p['state']?.singleOrNull ?? '') ||
        p['scope']?.singleOrNull != scopes ||
        redirect == null ||
        redirect.scheme != 'http' ||
        redirect.host != '127.0.0.1' ||
        !redirect.hasPort ||
        redirect.port == 0 ||
        redirect.userInfo.isNotEmpty ||
        redirect.hasQuery ||
        redirect.hasFragment ||
        redirect.path != '/morrow/oauth/callback') {
      throw const FormatException('Invalid Windows sign-in request');
    }
    return value;
  }
}

class OAuthEntry {
  const OAuthEntry(
    this.reference,
    this.revision,
    this.state,
    this.accessExpiresMs,
  );
  final Uint8List reference;
  final BigInt revision;
  final OAuthState state;
  final int accessExpiresMs;
}

class OAuthResult {
  const OAuthResult({
    this.services = const [],
    this.accounts = const [],
    this.key,
    this.account,
    this.phase,
    this.browser = '',
    this.next,
    this.snapshot,
  });
  final List<OAuthService> services;
  final List<OAuthEntry> accounts;
  final Uint8List? key, account, next, snapshot;
  final OAuthPhase? phase;
  final String browser;
}

Future<List<OAuthEntry>> readOAuthAccounts(
  WorkbenchOAuthControl backend,
  PluginLibraryEntry plugin,
  BigInt revision,
  OAuthService s,
  bool Function() current,
) async {
  final rows = <OAuthEntry>[];
  Uint8List? after, snapshot;
  for (int page = 0; page < 512; page++) {
    final result = await backend.accountControl(
      plugin,
      revision,
      OAuthCommand(
        OAuthAction.accounts,
        service: s.key,
        cursor: after,
        snapshot: snapshot,
      ),
    );
    if (!current()) return const [];
    final seen = result.snapshot;
    if (seen == null ||
        (snapshot != null && accountHex(snapshot) != accountHex(seen)))
      throw const FormatException('Changed account snapshot');
    snapshot = seen;
    var prev = after == null ? '' : accountHex(after);
    for (final row in result.accounts) {
      final id = accountHex(row.reference);
      if (id.compareTo(prev) <= 0 || rows.length >= 512)
        throw const FormatException('Invalid account page order');
      rows.add(row);
      prev = id;
    }
    final next = result.next;
    if (next == null) return rows;
    if ((after != null && accountHex(next).compareTo(accountHex(after)) <= 0) ||
        accountHex(next).compareTo(prev) < 0)
      throw const FormatException('Invalid account page cursor');
    after = next;
  }
  throw const FormatException('Account page limit');
}

class OAuthCommand {
  const OAuthCommand(
    this.action, {
    this.service,
    this.account,
    this.key,
    this.revision,
    this.days = 0,
    this.cursor,
    this.snapshot,
  });
  final OAuthAction action;
  final Uint8List? service, account, key, cursor, snapshot;
  final BigInt? revision;
  final int days;
  void encode(host.OAuthControlBuilder out) {
    out.action = host.OAuthAction.values.byName(action.name);
    if (service != null) out.service = _id(service);
    if (account != null) out.account = _id(account);
    if (key != null) out.key = _id(key);
    if (cursor != null) out.cursor = _id(cursor);
    if (snapshot != null) out.snapshot = _id(snapshot);
    if (revision != null) out.expectedRevisionBigInt = revision!;
    out.days = days;
  }

  OAuthResult decode(host.OAuthViewReader r) {
    final services = <OAuthService>[], entries = <OAuthEntry>[];
    final serviceRows = r.services, rows = r.accounts;
    if ((serviceRows?.length ?? 0) > 8 || (rows?.length ?? 0) > 16)
      throw const FormatException('Invalid Windows account page size');
    for (final s in serviceRows ?? <host.OAuthServiceReader>[]) {
      final name = s.name ?? '', scopes = s.scopes ?? '';
      if (name.isEmpty ||
          name.length > 100 ||
          scopes.isEmpty ||
          scopes.length > 512)
        throw const FormatException('Invalid reviewed service');
      services.add(
        OAuthService(
          _id(s.key),
          name,
          _endpoint(s.issuer),
          _endpoint(s.authorize),
          _endpoint(s.resource),
          scopes,
        ),
      );
    }
    for (final s in rows ?? <host.OAuthAccountReader>[]) {
      if (s.state < 1 || s.state > 4 || s.revisionBigInt <= BigInt.zero)
        throw const FormatException('Invalid Windows account state');
      entries.add(
        OAuthEntry(
          _id(s.reference),
          s.revisionBigInt,
          OAuthState.values[s.state - 1],
          s.accessExpiresMs,
        ),
      );
    }
    if (action == OAuthAction.services) return OAuthResult(services: services);
    if (action == OAuthAction.accounts)
      return OAuthResult(
        accounts: entries,
        snapshot: _id(r.snapshot),
        next: (r.next?.isEmpty ?? true) ? null : _id(r.next),
      );
    if (action == OAuthAction.logout) {
      if (entries.length != 1 ||
          entries.single.state != OAuthState.signedOut ||
          accountHex(entries.single.reference) != accountHex(account!))
        throw const FormatException('Missing sign-out receipt');
      return OAuthResult(accounts: entries);
    }
    if (r.phase < 1 ||
        r.phase > 5 ||
        accountHex(_id(r.key)) != accountHex(key!))
      throw const FormatException('Invalid Windows account operation');
    final url = r.browserUrl ?? '';
    if (r.phase==5 && (r.account?.isEmpty??true) && (action==OAuthAction.poll || action==OAuthAction.cancel) && url.isEmpty && entries.isEmpty) {
      return OAuthResult(key:_id(r.key),phase:OAuthPhase.unconfirmed);
    }
    final ref = _id(r.account);
    if ((account != null && accountHex(account!) != accountHex(ref)) ||
        (action == OAuthAction.login && (url.isEmpty || r.phase != 1)) ||
        (action != OAuthAction.login && url.isNotEmpty) ||
        (r.phase == 2 &&
            (entries.length != 1 ||
                accountHex(entries.single.reference) != accountHex(ref)))) {
      throw const FormatException('Unexpected Windows account result');
    }
    return OAuthResult(
      accounts: entries,
      key: _id(r.key),
      account: ref,
      phase: OAuthPhase.values[r.phase - 1],
      browser: url,
    );
  }
}
