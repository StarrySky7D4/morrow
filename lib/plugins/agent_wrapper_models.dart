import 'dart:convert';
import 'dart:typed_data';

/// Metadata and persisted ceilings only; these objects never grant execution.
abstract final class AgentWrapperValidation {
  static final maxU64 = (BigInt.one << 64) - BigInt.one;
  static BigInt u64(BigInt value) {
    if (value < BigInt.zero || value > maxU64) {
      throw const FormatException('Invalid wrapper revision');
    }
    return value;
  }

  static Uint8List digest(List<int> bytes) {
    if (bytes.length != 32 || bytes.any((v) => v < 0 || v > 255)) {
      throw const FormatException('Invalid wrapper digest');
    }
    return Uint8List.fromList(bytes).asUnmodifiableView();
  }

  static bool same(List<int> a, List<int> b) {
    if (a.length != b.length) return false;
    for (var i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }

  static String text(String value, {int max = 256, bool empty = false}) {
    if ((!empty && value.isEmpty) ||
        value.contains('\u0000') ||
        utf8.encode(value).length > max) {
      throw const FormatException('Invalid wrapper text');
    }
    return value;
  }

  static List<String> scopes(List<String> values) {
    if (values.isEmpty || values.length > 16) {
      throw const FormatException('Invalid wrapper session scopes');
    }
    for (var i = 0; i < values.length; i++) {
      text(values[i], max: 128);
      if (!RegExp(r'^[a-zA-Z0-9._:-]+$').hasMatch(values[i])) {
        throw const FormatException('Invalid wrapper scope');
      }
      if (i > 0 && values[i - 1].compareTo(values[i]) >= 0) {
        throw const FormatException('Wrapper scopes must be sorted and unique');
      }
    }
    return List.unmodifiable(values);
  }

  static String hex(List<int> bytes) =>
      bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();
}

final class AgentWrapperRevisions {
  AgentWrapperRevisions({required BigInt catalog, required BigInt manager})
    : catalog = AgentWrapperValidation.u64(catalog),
      manager = AgentWrapperValidation.u64(manager);
  final BigInt catalog, manager;
  bool same(AgentWrapperRevisions other) =>
      catalog == other.catalog && manager == other.manager;
}

final class AgentWrapperApproval {
  AgentWrapperApproval({
    required this.sessionBits,
    required this.processBits,
    required List<String> sessions,
    required this.executionDomain,
  }) : sessions = AgentWrapperValidation.scopes(sessions) {
    if (sessionBits < 1 ||
        sessionBits & ~31 != 0 ||
        sessionBits & 1 == 0 ||
        processBits < 0 ||
        processBits & ~127 != 0) {
      throw const FormatException('Unknown wrapper capabilities');
    }
    AgentWrapperValidation.text(executionDomain, max: 128);
    if (!RegExp(r'^[a-zA-Z0-9._:-]+$').hasMatch(executionDomain)) {
      throw const FormatException('Invalid wrapper domain');
    }
  }
  final int sessionBits, processBits;
  final List<String> sessions;
  final String executionDomain;
  bool subsetOf(AgentWrapperApproval declaration) =>
      sessionBits & ~declaration.sessionBits == 0 &&
      processBits & ~declaration.processBits == 0 &&
      executionDomain == declaration.executionDomain &&
      sessions.every(declaration.sessions.contains);
}

final class AgentWrapperReview {
  AgentWrapperReview({
    required this.id,
    required this.version,
    required List<int> wrapperSha256,
    required List<int> baseSha256,
    required List<int> sessionSchema,
    required List<int> processSchema,
    required this.declaration,
  }) : wrapperSha256 = AgentWrapperValidation.digest(wrapperSha256),
       baseSha256 = AgentWrapperValidation.digest(baseSha256),
       sessionSchema = AgentWrapperValidation.digest(sessionSchema),
       processSchema = AgentWrapperValidation.digest(processSchema) {
    AgentWrapperValidation.text(id);
    AgentWrapperValidation.text(version, max: 128);
  }
  final String id, version;
  final Uint8List wrapperSha256, baseSha256, sessionSchema, processSchema;
  final AgentWrapperApproval declaration;
  String get key => AgentWrapperValidation.hex(wrapperSha256);
}

enum AgentWrapperAction {
  state,
  inspect,
  install,
  baseSelect,
  baseEnable,
  wrapperSelect,
  approve,
  wrapperEnable,
  remove,
  page,
}

enum AgentWrapperStatus {
  ok,
  invalid,
  conflict,
  denied,
  notFound,
  limit,
  storage,
  unknown,
  busy,
  recoveryRequired,
  ownerUnavailable,
  unsupported,
}

final class AgentWrapperResult {
  AgentWrapperResult({required this.revisions, this.review, this.page});
  final AgentWrapperRevisions revisions;
  final AgentWrapperReview? review;
  final AgentWrapperPage? page;
}

final class AgentWrapperEntry {
  AgentWrapperEntry({
    required this.review,
    required this.selected,
    required this.enabled,
    required this.baseSelected,
    required this.baseEnabled,
    this.approved,
  }) {
    if (enabled && (!selected || approved == null) ||
        baseEnabled && !baseSelected ||
        approved != null && !selected ||
        approved != null && !approved!.subsetOf(review.declaration)) {
      throw const FormatException('Inconsistent wrapper approval');
    }
  }
  final AgentWrapperReview review;
  final bool selected, enabled, baseSelected, baseEnabled;
  final AgentWrapperApproval? approved;
}

final class AgentWrapperPage {
  AgentWrapperPage({
    required this.revisions,
    required List<AgentWrapperEntry> entries,
    this.cursor = '',
  }) : entries = List.unmodifiable(entries) {
    if (entries.length > 16 ||
        entries.map((e) => e.review.key).toSet().length != entries.length) {
      throw const FormatException('Invalid wrapper page');
    }
    if (cursor.isNotEmpty &&
        (cursor.length != 64 || !RegExp(r'^[0-9a-f]+$').hasMatch(cursor))) {
      throw const FormatException('Invalid wrapper cursor');
    }
  }
  final AgentWrapperRevisions revisions;
  final List<AgentWrapperEntry> entries;
  final String cursor;
}
