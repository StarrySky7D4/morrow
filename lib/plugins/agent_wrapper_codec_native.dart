import 'dart:convert';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:crypto/crypto.dart';
import 'agent_wrapper_models.dart';

/// Layout pinned to extensions/agent-catalog-admin-v1/contracts/agent_catalog.capnp.
/// This profile is private to the original trusted native host.
final class AgentWrapperRequest {
  AgentWrapperRequest({
    required this.action,
    required List<int> id,
    this.revisions,
    this.path = '',
    this.packageId = '',
    List<int> fullSha256 = const [],
    this.enabled = false,
    this.approval,
    this.cursor = '',
    this.limit = 0,
  }) : id = Uint8List.fromList(id).asUnmodifiableView(),
       fullSha256 = Uint8List.fromList(fullSha256).asUnmodifiableView() {
    if (id.length != 16 ||
        id.every((b) => b == 0) ||
        id.any((b) => b < 0 || b > 255)) {
      throw const FormatException('Invalid catalog request identity');
    }
    final a = action;
    final hasRevisions =
        a != AgentWrapperAction.state && a != AgentWrapperAction.inspect;
    final hasPath =
        a == AgentWrapperAction.inspect || a == AgentWrapperAction.install;
    final hasId =
        a == AgentWrapperAction.baseEnable ||
        a == AgentWrapperAction.approve ||
        a == AgentWrapperAction.wrapperEnable ||
        a == AgentWrapperAction.remove;
    final hasSha =
        a != AgentWrapperAction.state &&
        a != AgentWrapperAction.inspect &&
        a != AgentWrapperAction.page;
    if (hasRevisions != (revisions != null) ||
        hasPath != path.isNotEmpty ||
        hasId != packageId.isNotEmpty ||
        hasSha != fullSha256.isNotEmpty ||
        (a == AgentWrapperAction.approve) != (approval != null) ||
        enabled &&
            a != AgentWrapperAction.baseEnable &&
            a != AgentWrapperAction.wrapperEnable ||
        a != AgentWrapperAction.page && (cursor.isNotEmpty || limit != 0)) {
      throw const FormatException('Unexpected catalog request fields');
    }
    if (hasPath) AgentWrapperValidation.text(path, max: 4096);
    if (hasId) AgentWrapperValidation.text(packageId);
    if (hasSha) AgentWrapperValidation.digest(fullSha256);
    if (a == AgentWrapperAction.page &&
        (limit < 1 ||
            limit > 16 ||
            cursor.isNotEmpty && !RegExp(r'^[0-9a-f]{64}$').hasMatch(cursor))) {
      throw const FormatException('Invalid catalog page request');
    }
  }
  final AgentWrapperAction action;
  final Uint8List id, fullSha256;
  final AgentWrapperRevisions? revisions;
  final String path, packageId, cursor;
  final bool enabled;
  final AgentWrapperApproval? approval;
  final int limit;
  bool get mutates =>
      action != AgentWrapperAction.state &&
      action != AgentWrapperAction.inspect &&
      action != AgentWrapperAction.page;
}

final class AgentWrapperReply {
  AgentWrapperReply({
    required this.action,
    required this.status,
    required List<int> id,
    required List<int> requestSha256,
    required this.result,
  }) : id = Uint8List.fromList(id).asUnmodifiableView(),
       requestSha256 = AgentWrapperValidation.digest(requestSha256);
  final AgentWrapperAction action;
  final AgentWrapperStatus status;
  final Uint8List id, requestSha256;
  final AgentWrapperResult result;
}

// Small typed accessors over the fixed private layout; no changes to old generated SDK files.
final class _Reader extends StructReader {
  _Reader(super.raw, {super.capabilities});
  _Reader? struct(int index) => getStructFieldWith(
    index,
    (r) => _Reader(r, capabilities: capabilityTable),
  );
  ListReader<_Reader>? structs(int index) => getStructListFieldWith(
    index,
    (r) => _Reader(r, capabilities: capabilityTable),
  );
  BigInt u64(int offset) =>
      (BigInt.from(getUint32Field(offset + 4).toUnsigned(32)) << 32) |
      BigInt.from(getUint32Field(offset).toUnsigned(32));
}

final class _Builder extends StructBuilder {
  _Builder(super.raw);
  @override
  _Reader asReader() => _Reader(rawToReader());
  _Builder struct(int index, int data, int pointers) =>
      initStructFieldWith(index, (r) => _Builder(r), data, pointers);
  ListBuilder<_Builder> structs(
    int index,
    int length,
    int data,
    int pointers,
  ) => initStructListFieldWith(
    index,
    length,
    (r) => _Builder(r),
    data,
    pointers,
  );
  void u64(int offset, BigInt value) {
    AgentWrapperValidation.u64(value);
    setUint32Field(offset, (value & BigInt.from(0xffffffff)).toInt());
    setUint32Field(offset + 4, (value >> 32).toInt());
  }
}

final class _Factory extends StructFactory<_Reader, _Builder> {
  _Factory(this.dataWords, this.ptrWords);
  @override
  final int dataWords;
  @override
  final int ptrWords;
  @override
  StructSchemaInfo get schema => StructSchemaInfo(
    id: 0, // Reflection only; the real contract identity is the 32-byte schema digest.
    displayName: 'agent_catalog',
    shortName: 'Admin',
    dataWords: dataWords,
    pointerWords: ptrWords,
    fields: const [],
  );
  @override
  _Reader fromRawReader(RawStructReader r) => _Reader(r);
  @override
  _Reader fromRawReaderWithCapabilities(
    RawStructReader r,
    List<Object?> capabilities,
  ) => _Reader(r, capabilities: capabilities);
  @override
  _Builder fromRawBuilder(RawStructBuilder r) => _Builder(r);
}

abstract final class AgentWrapperCodec {
  static const maxBytes = 128 * 1024;
  static final prefix = Uint8List.fromList(
    ascii.encode('MROWCA15'),
  ).asUnmodifiableView();
  static final schema = Uint8List.fromList(
    List.generate(
      32,
      (i) => int.parse(
        'f0fa231af569c7f8eef0c1bde8305b592ed5e2d616af07d1ecd517eef3a03a04'
            .substring(i * 2, i * 2 + 2),
        radix: 16,
      ),
    ),
  ).asUnmodifiableView();
  static Uint8List hash(List<int> bytes) =>
      Uint8List.fromList(sha256.convert(bytes).bytes);
  static void _revisions(_Builder b, AgentWrapperRevisions r) {
    b.u64(0, r.catalog);
    b.u64(8, r.manager);
  }

  static AgentWrapperRevisions _readRevisions(_Reader? r) {
    if (r == null) throw const FormatException('Missing catalog revisions');
    return AgentWrapperRevisions(catalog: r.u64(0), manager: r.u64(8));
  }

  static void _scopes(_Builder b, int index, List<String> values) {
    final list = b.initTextListField(index, values.length);
    for (var i = 0; i < values.length; i++) {
      list[i] = values[i];
    }
  }

  static List<String> _readScopes(_Reader r, int index) {
    final list = r.getTextListField(index);
    if (list == null || list.isEmpty || list.length > 16) {
      throw const FormatException('Missing catalog scopes');
    }
    return [
      for (final item in list)
        if (item != null)
          item
        else
          throw const FormatException('Missing scope'),
    ];
  }

  static void _approval(_Builder b, AgentWrapperApproval a) {
    b.setUint16Field(0, a.sessionBits);
    b.setUint16Field(2, a.processBits);
    _scopes(b, 0, a.sessions);
    b.setTextField(1, a.executionDomain);
  }

  static AgentWrapperApproval _readApproval(_Reader r) => AgentWrapperApproval(
    sessionBits: r.getUint16Field(0),
    processBits: r.getUint16Field(2),
    sessions: _readScopes(r, 0),
    executionDomain: r.getTextField(1) ?? '',
  );
  static void _review(_Builder b, AgentWrapperReview r) {
    b.setUint16Field(0, r.declaration.sessionBits);
    b.setUint16Field(2, r.declaration.processBits);
    b.setTextField(0, r.id);
    b.setTextField(1, r.version);
    b.setDataField(2, r.wrapperSha256);
    b.setDataField(3, r.baseSha256);
    b.setDataField(4, r.sessionSchema);
    b.setDataField(5, r.processSchema);
    _scopes(b, 6, r.declaration.sessions);
    b.setTextField(7, r.declaration.executionDomain);
  }

  static AgentWrapperReview _readReview(_Reader? r) {
    if (r == null) throw const FormatException('Missing wrapper review');
    return AgentWrapperReview(
      id: r.getTextField(0) ?? '',
      version: r.getTextField(1) ?? '',
      wrapperSha256: r.getDataField(2) ?? [],
      baseSha256: r.getDataField(3) ?? [],
      sessionSchema: r.getDataField(4) ?? [],
      processSchema: r.getDataField(5) ?? [],
      declaration: AgentWrapperApproval(
        sessionBits: r.getUint16Field(0),
        processBits: r.getUint16Field(2),
        sessions: _readScopes(r, 6),
        executionDomain: r.getTextField(7) ?? '',
      ),
    );
  }

  static Uint8List _frame(MessageBuilder builder) {
    final bytes = Uint8List.fromList([...prefix, ...builder.serialize()]);
    if (bytes.length > maxBytes) {
      throw const FormatException('Catalog frame exceeds limit');
    }
    return bytes;
  }

  static _Reader _root(Uint8List bytes, int pointers) {
    if (bytes.length < 24 ||
        bytes.length > maxBytes ||
        !AgentWrapperValidation.same(bytes.sublist(0, 8), prefix)) {
      throw const FormatException('Invalid catalog frame');
    }
    final body = Uint8List.sublistView(bytes, 8);
    final header = ByteData.sublistView(body);
    if (body.length % 8 != 0 ||
        header.getUint32(0, Endian.little) != 0 ||
        header.getUint32(4, Endian.little) * 8 + 8 != body.length) {
      throw const FormatException(
        'Catalog frame requires exact one-segment framing',
      );
    }
    return MessageReader.deserialize(
      body,
      MessageReaderOptions(
        traversalLimitInWords: maxBytes ~/ 8,
        nestingLimit: 12,
        maxSegments: 1,
      ),
    ).getRoot(_Factory(1, pointers));
  }

  static Uint8List encodeRequest(AgentWrapperRequest r) {
    final builder = MessageBuilder(initialCapacityWords: maxBytes ~/ 8);
    final b = builder.initRoot(_Factory(1, 8));
    b.setUint16Field(0, 1);
    b.setUint16Field(2, r.action.index);
    b.setBoolField(32, r.enabled);
    b.setUint16Field(6, r.limit);
    b.setDataField(0, schema);
    b.setDataField(1, r.id);
    if (r.revisions != null) _revisions(b.struct(2, 2, 0), r.revisions!);
    if (r.path.isNotEmpty) b.setTextField(3, r.path);
    if (r.packageId.isNotEmpty) b.setTextField(4, r.packageId);
    if (r.fullSha256.isNotEmpty) b.setDataField(5, r.fullSha256);
    if (r.approval != null) _approval(b.struct(6, 1, 2), r.approval!);
    if (r.cursor.isNotEmpty) b.setTextField(7, r.cursor);
    return _frame(builder);
  }

  static AgentWrapperRequest decodeRequest(Uint8List bytes) {
    final b = _root(bytes, 8);
    if (b.getUint16Field(0) != 1 ||
        !AgentWrapperValidation.same(b.getDataField(0) ?? [], schema) ||
        b.getUint16Field(2) >= AgentWrapperAction.values.length) {
      throw const FormatException('Catalog schema or action mismatch');
    }
    final approval = b.struct(6), revisions = b.struct(2);
    final r = AgentWrapperRequest(
      action: AgentWrapperAction.values[b.getUint16Field(2)],
      id: b.getDataField(1) ?? [],
      revisions: revisions == null ? null : _readRevisions(revisions),
      path: b.getTextField(3) ?? '',
      packageId: b.getTextField(4) ?? '',
      fullSha256: b.getDataField(5) ?? [],
      enabled: b.getBoolField(32),
      approval: approval == null ? null : _readApproval(approval),
      cursor: b.getTextField(7) ?? '',
      limit: b.getUint16Field(6),
    );
    if (!AgentWrapperValidation.same(encodeRequest(r), bytes)) {
      throw const FormatException('Noncanonical catalog request');
    }
    return r;
  }

  static Uint8List encodeReply(AgentWrapperReply r) {
    final builder = MessageBuilder(initialCapacityWords: maxBytes ~/ 8);
    final b = builder.initRoot(_Factory(1, 7));
    b.setUint16Field(0, 1);
    b.setUint16Field(2, r.action.index);
    b.setUint16Field(4, r.status.index);
    final kind = r.result.review != null
        ? 1
        : r.result.page != null
        ? 2
        : 0;
    b.setUint16Field(6, kind);
    b.setDataField(0, schema);
    b.setDataField(1, r.id);
    b.setDataField(2, r.requestSha256);
    _revisions(b.struct(3, 2, 0), r.result.revisions);
    if (r.result.review != null) _review(b.struct(4, 1, 8), r.result.review!);
    if (r.result.page != null) {
      final page = r.result.page!;
      final list = b.structs(5, page.entries.length, 1, 2);
      for (var i = 0; i < page.entries.length; i++) {
        final e = page.entries[i], out = list[i];
        out.setBoolField(0, e.selected);
        out.setBoolField(1, e.enabled);
        out.setBoolField(2, e.baseSelected);
        out.setBoolField(3, e.baseEnabled);
        _review(out.struct(0, 1, 8), e.review);
        if (e.approved != null) _approval(out.struct(1, 1, 2), e.approved!);
      }
      if (page.cursor.isNotEmpty) b.setTextField(6, page.cursor);
    }
    return _frame(builder);
  }

  static AgentWrapperReply decodeReply(
    AgentWrapperRequest request,
    Uint8List bytes,
  ) {
    final b = _root(bytes, 7);
    final action = b.getUint16Field(2),
        status = b.getUint16Field(4),
        kind = b.getUint16Field(6);
    if (b.getUint16Field(0) != 1 ||
        !AgentWrapperValidation.same(b.getDataField(0) ?? [], schema) ||
        action != request.action.index ||
        status >= AgentWrapperStatus.values.length ||
        kind > 2 ||
        !AgentWrapperValidation.same(b.getDataField(1) ?? [], request.id) ||
        !AgentWrapperValidation.same(
          b.getDataField(2) ?? [],
          hash(encodeRequest(request)),
        )) {
      throw const FormatException('Catalog reply correlation mismatch');
    }
    final revisions = _readRevisions(b.struct(3));
    AgentWrapperReview? review;
    AgentWrapperPage? page;
    if (status != 0 && kind != 0 ||
        status == 0 &&
            kind !=
                (action == 1 || action == 2
                    ? 1
                    : action == 9
                    ? 2
                    : 0)) {
      throw const FormatException('Unexpected catalog reply body');
    }
    if (kind == 1) {
      review = _readReview(b.struct(4));
      if (request.fullSha256.isNotEmpty &&
          !AgentWrapperValidation.same(
            review.wrapperSha256,
            request.fullSha256,
          )) {
        throw const FormatException('Catalog review identity changed');
      }
    }
    if (kind == 2) {
      final rows = b.structs(5);
      if (rows == null || rows.length > request.limit) {
        throw const FormatException('Catalog page exceeds request');
      }
      final entries = <AgentWrapperEntry>[];
      var previous = request.cursor;
      for (final row in rows) {
        final r = _readReview(row.struct(0)), approval = row.struct(1);
        if (r.key.compareTo(previous) <= 0) {
          throw const FormatException('Catalog page order changed');
        }
        previous = r.key;
        entries.add(
          AgentWrapperEntry(
            review: r,
            selected: row.getBoolField(0),
            enabled: row.getBoolField(1),
            baseSelected: row.getBoolField(2),
            baseEnabled: row.getBoolField(3),
            approved: approval == null ? null : _readApproval(approval),
          ),
        );
      }
      final cursor = b.getTextField(6) ?? '';
      if (cursor.isNotEmpty &&
          (entries.isEmpty || cursor != entries.last.review.key)) {
        throw const FormatException('Catalog continuation changed');
      }
      if (request.revisions == null || !revisions.same(request.revisions!)) {
        throw const FormatException('Catalog page revision changed');
      }
      page = AgentWrapperPage(
        revisions: revisions,
        entries: entries,
        cursor: cursor,
      );
    }
    final reply = AgentWrapperReply(
      action: request.action,
      status: AgentWrapperStatus.values[status],
      id: request.id,
      requestSha256: hash(encodeRequest(request)),
      result: AgentWrapperResult(
        revisions: revisions,
        review: review,
        page: page,
      ),
    );
    if (!AgentWrapperValidation.same(encodeReply(reply), bytes)) {
      throw const FormatException('Noncanonical catalog reply');
    }
    return reply;
  }
}
