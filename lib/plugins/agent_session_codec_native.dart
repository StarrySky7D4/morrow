import 'dart:convert';
import 'dart:typed_data';
import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:crypto/crypto.dart';
import 'agent_session_control.dart';
import 'agent_session_models.dart';
import 'agent_session_native.dart';

// Layout read from actual Rust generated files (f5d1d1/0), not field ordinals:
// private 139175/3752349747bb6b5de2c9100e286ef3a517ce8a25998acf5b65544606bcc10fc8;
// R2 358939/160ec3e2c7bf238c4dd350861b5d63f04d0f8140089adc9d39fe94afd0574dd2.
final class _Reader extends StructReader {
  _Reader(super.raw, {super.capabilities});
  void shape(int data, int pointers) {
    if (raw.dataWords != data || raw.ptrWords != pointers) {
      throw const FormatException('Unexpected private session struct shape');
    }
  }

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
    AgentSessionValidation.u64(value);
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
    id: 0,
    displayName: 'private_agent_product_control',
    shortName: 'Session',
    dataWords: dataWords,
    pointerWords: ptrWords,
    fields: const [],
  );
  @override
  _Reader fromRawReader(RawStructReader r) => _Reader(r);
  @override
  _Reader fromRawReaderWithCapabilities(RawStructReader r, List<Object?> c) =>
      _Reader(r, capabilities: c);
  @override
  _Builder fromRawBuilder(RawStructBuilder r) => _Builder(r);
}

/// Fixed private native-only codec. It never creates R2 requests or authority.
/// A Reply is a transport completion; its original R2 Rejected remains rejected.
final class AgentSessionCodec implements AgentSessionWireCodec {
  static final prefix = Uint8List.fromList(
    ascii.encode('MROWPS30'),
  ).asUnmodifiableView();
  static final schema = _hex(
    'df8c44ce102983b495efe17881d2d3db67fa2c47ec7639ab6d5c7c30a542098c',
  );
  static final r2Schema = _hex(
    'f905eaed5107bebc9612a9dede757fa54ce674f13120adce31ed331cf7557685',
  );
  // Exact original R2 generated Core identity, not an execution permission.
  static final runtimeDigest = _hex(
    '18f584f69472373bab33a6c47dcfda874477e36e0a52c32b3af1fe4fbb748855',
  );
  static final contentDigest = _hex(
    '8493c9194c760bad05d480a82a676098e002513eac2166418410c274ec957ff4',
  );
  static Uint8List _hex(String s) => Uint8List.fromList([
    for (var i = 0; i < s.length; i += 2)
      int.parse(s.substring(i, i + 2), radix: 16),
  ]).asUnmodifiableView();
  static Uint8List hash(List<int> data) =>
      Uint8List.fromList(sha256.convert(data).bytes);
  static bool _same(List<int> a, List<int> b) =>
      AgentSessionValidation.same(a, b);
  static String _id(List<int> bytes) =>
      bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();
  static Never _bad() =>
      throw const FormatException('Private session receipt mismatch');

  @override
  Uint8List encode(AgentSessionWireRequest r) {
    final command = r.command;
    var action = 0, sid = r.sessionId, parent = '';
    var slot = r.writerSlot, parentTail = BigInt.zero, after = BigInt.zero;
    var epoch = BigInt.zero, tail = BigInt.zero;
    var limit = 0;
    var events = <AgentSessionEvent>[];
    switch (command) {
      case AgentSessionList():
        action = 1;
      case AgentSessionCreate():
        action = 2;
        sid = command.sessionId;
        parent = command.parent ?? '';
        parentTail = command.parentTail;
      case AgentSessionSnapshotRequest():
        action = 3;
        sid = command.sessionId;
        after = command.after;
        limit = command.limit;
        slot = command.writerSlot ?? BigInt.zero;
      case AgentSessionOpenWriter():
        action = 4;
        sid = command.sessionId;
        slot = command.writerSlot;
        epoch = command.expectedEpoch;
      case AgentSessionAppend():
        action = 5;
        sid = command.sessionId;
        slot = command.writerSlot;
        epoch = command.epoch;
        tail = command.expectedTail;
        events = command.events;
      case null:
        break;
      default:
        _bad();
    }
    // Matches the final private Rust HeapAllocator.first_segment_words(16384).
    // Frozen R2's allocator/encoding is not changed or reproduced here.
    final message = MessageBuilder(initialCapacityWords: 16384);
    final b = message.initRoot(_Factory(10, 6));
    b.setUint16Field(0, 1);
    b.setDataField(0, schema);
    b.setDataField(1, r.id);
    b.setDataField(2, r.taskKey);
    b.u64(8, r.attachment);
    b.setUint16Field(2, r.operation.index + 1);
    b.u64(16, r.submission);
    b.u64(24, slot);
    b.u64(32, r.generation);
    b.setUint16Field(4, action);
    b.setTextField(3, sid);
    b.setTextField(4, parent);
    b.u64(40, parentTail);
    b.u64(48, after);
    b.setUint32Field(56, limit);
    b.u64(64, epoch);
    b.u64(72, tail);
    final list = b.structs(5, events.length, 0, 2);
    for (var i = 0; i < events.length; i++) {
      list[i].setTextField(0, events[i].id);
      list[i].setDataField(1, events[i].body);
    }
    final bytes = Uint8List.fromList([...prefix, ...message.serialize()]);
    if (bytes.length > NativeAgentSessionClient.maxRequestBytes) _bad();
    return bytes;
  }

  static _Reader _root(
    Uint8List bytes,
    int max,
    int data,
    int pointers, {
    bool private = true,
  }) {
    if (bytes.isEmpty || bytes.length > max) _bad();
    if (private && (bytes.length < 24 || !_same(bytes.sublist(0, 8), prefix)))
      _bad();
    final body = private ? Uint8List.sublistView(bytes, 8) : bytes;
    if (body.length < 16 || body.length % 8 != 0) _bad();
    final header = ByteData.sublistView(body);
    final count = header.getUint32(0, Endian.little) + 1;
    if (count < 1 || count > 512) _bad();
    var total = ((count + 2) ~/ 2) * 8;
    if (total > body.length) _bad();
    for (var i = 0; i < count; i++) {
      total += header.getUint32(4 + i * 4, Endian.little) * 8;
      if (total > body.length) _bad();
    }
    if (total != body.length) _bad();
    final result = MessageReader.deserialize(
      body,
      MessageReaderOptions(
        traversalLimitInWords: max ~/ 8 * 4,
        nestingLimit: 12,
        maxSegments: 512,
      ),
    ).getRoot(_Factory(data, pointers));
    result.shape(data, pointers);
    return result;
  }

  @override
  AgentSessionResult decode(AgentSessionWireRequest request, Uint8List bytes) {
    final r = _root(bytes, NativeAgentSessionClient.maxReplyBytes, 6, 9);
    final statusIndex = r.getUint16Field(4), kind = r.getUint16Field(40);
    if (r.getUint16Field(0) != 1 ||
        !_same(r.getDataField(0) ?? [], schema) ||
        !_same(r.getDataField(1) ?? [], request.id) ||
        !_same(r.getDataField(2) ?? [], request.taskKey) ||
        r.getUint16Field(2) != request.operation.index + 1 ||
        !_same(r.getDataField(3) ?? [], hash(encode(request))) ||
        statusIndex >= AgentSessionStatus.values.length ||
        kind > 4)
      _bad();
    final status = AgentSessionStatus.values[statusIndex];
    final attachment = r.u64(8),
        token = r.u64(16),
        slot = r.u64(24),
        generation = r.u64(32);
    final raw = Uint8List.fromList(r.getDataField(4) ?? []);
    final List<int> requestHash = r.getDataField(5) ?? <int>[];
    if (requestHash.length != 32 ||
        raw.length > 128 * 1024 ||
        request.operation != AgentSessionOperation.attach &&
            attachment != request.attachment ||
        status == AgentSessionStatus.attached && attachment == BigInt.zero)
      _bad();
    final sessions = r.structs(6), events = r.structs(7);
    if ((sessions?.length ?? 0) > 16 || (events?.length ?? 0) > 16) _bad();
    final List<int> checkpoint = r.getDataField(8) ?? <int>[];
    final gap = r.getBoolField(50);
    final rejected = r.getUint16Field(42);
    AgentSessionData? data;
    switch (kind) {
      case 0:
        if ((sessions?.length ?? 0) != 0 ||
            (events?.length ?? 0) != 0 ||
            checkpoint.isNotEmpty ||
            gap ||
            rejected != 0)
          _bad();
      case 1:
        if (sessions?.length != 1 ||
            (events?.length ?? 0) != 0 ||
            checkpoint.isNotEmpty ||
            gap ||
            rejected != 0)
          _bad();
        data = _info(sessions![0]);
      case 2:
        if ((events?.length ?? 0) != 0 ||
            checkpoint.isNotEmpty ||
            gap ||
            rejected != 0)
          _bad();
        data = AgentSessionInfos([
          for (final row in sessions ?? <_Reader>[]) _info(row),
        ]);
      case 3:
        if (sessions?.length != 1 || rejected != 0) _bad();
        data = _snapshot(
          _info(sessions![0]),
          [for (final row in events ?? <_Reader>[]) _stored(row)],
          gap,
          checkpoint,
        );
      case 4:
        if (rejected < 1 ||
            rejected > 9 ||
            (sessions?.length ?? 0) != 0 ||
            (events?.length ?? 0) != 0 ||
            checkpoint.isNotEmpty ||
            gap)
          _bad();
        data = AgentSessionRejected(
          AgentSessionRejectionKind.values[rejected - 1],
        );
    }
    if (status == AgentSessionStatus.reply) {
      if (request.operation != AgentSessionOperation.read ||
          request.pendingOperation != AgentSessionOperation.submit ||
          request.pendingCommand == null ||
          raw.isEmpty ||
          kind == 0 ||
          slot != BigInt.zero ||
          token != request.submission ||
          generation != request.expectedGeneration)
        _bad();
      final original = _readR2(request, raw, requestHash);
      if (!_sameData(original, data!)) _bad();
      _validateData(request.pendingCommand!, data);
    } else {
      if (kind != 0 || raw.isNotEmpty || requestHash.any((b) => b != 0)) _bad();
      if (status == AgentSessionStatus.writerAdmitted) {
        if (request.operation != AgentSessionOperation.read ||
            request.pendingOperation != AgentSessionOperation.admitWriter ||
            request.pendingCommand is! AgentSessionAdmitWriter ||
            token != request.submission ||
            generation != request.expectedGeneration ||
            slot == BigInt.zero)
          _bad();
        data = AgentSessionWriterAdmitted(
          (request.pendingCommand as AgentSessionAdmitWriter).sessionId,
          slot,
        );
      } else if (slot != BigInt.zero)
        _bad();
    }
    _validateStatus(request, status, token, generation);
    return AgentSessionResult(
      operation: request.operation,
      status: status,
      taskKey: request.taskKey,
      attachment: attachment,
      submission: token,
      endpointGeneration: generation,
      leaseLive: r.getBoolField(48),
      deliveryUnknown: r.getBoolField(49),
      data: data,
    );
  }

  static void _validateStatus(
    AgentSessionWireRequest q,
    AgentSessionStatus status,
    BigInt token,
    BigInt generation,
  ) {
    if (status.index <= AgentSessionStatus.limit.index)
      return; // Correlated error observations can retain a pending token.
    final allowed = switch (q.operation) {
      AgentSessionOperation.attach => status == AgentSessionStatus.attached,
      AgentSessionOperation.open ||
      AgentSessionOperation.submit ||
      AgentSessionOperation.admitWriter =>
        status == AgentSessionStatus.submitted,
      AgentSessionOperation.status ||
      AgentSessionOperation.markLost ||
      AgentSessionOperation.closeWriter =>
        status == AgentSessionStatus.observed,
      AgentSessionOperation.detach => status == AgentSessionStatus.detached,
      AgentSessionOperation.read =>
        status == AgentSessionStatus.pending ||
            q.pendingOperation == AgentSessionOperation.open &&
                status == AgentSessionStatus.opened ||
            q.pendingOperation == AgentSessionOperation.submit &&
                status == AgentSessionStatus.reply ||
            q.pendingOperation == AgentSessionOperation.admitWriter &&
                status == AgentSessionStatus.writerAdmitted,
    };
    if (!allowed ||
        status == AgentSessionStatus.attached &&
            (token != BigInt.zero || generation != BigInt.zero) ||
        status == AgentSessionStatus.detached && token != BigInt.zero ||
        status == AgentSessionStatus.submitted && token == BigInt.zero ||
        q.operation == AgentSessionOperation.read && token != q.submission ||
        status == AgentSessionStatus.opened && generation == BigInt.zero)
      _bad();
  }

  static AgentSessionInfo _info(_Reader r, {bool r2 = false}) {
    r.shape(7, 4);
    final parentText = r.getTextField(1) ?? '';
    if (r2 && r.getBoolField(0) != parentText.isNotEmpty) _bad();
    return AgentSessionInfo(
      sessionId: r.getTextField(0) ?? '',
      parent: parentText.isEmpty ? null : parentText,
      parentTail: r.u64(r2 ? 8 : 0),
      epoch: r.u64(r2 ? 16 : 8),
      tail: r.u64(r2 ? 24 : 16),
      floor: r.u64(r2 ? 32 : 24),
      archived: r.getBoolField(r2 ? 1 : 256),
      revision: r.u64(40),
      checkpointTail: r.u64(48),
      checkpointSha256: r.getDataField(2) ?? [],
      checkpointSealed: r.getBoolField(r2 ? 2 : 257),
      parentCheckpointSha256: r.getDataField(3) ?? [],
    );
  }

  static AgentSessionStoredEvent _stored(_Reader r) {
    r.shape(1, 2);
    return AgentSessionStoredEvent(
      r.u64(0),
      AgentSessionEvent(r.getTextField(0) ?? '', r.getDataField(1) ?? []),
    );
  }

  static AgentSessionSnapshot _snapshot(
    AgentSessionInfo info,
    List<AgentSessionStoredEvent> events,
    bool gap,
    List<int> checkpoint,
  ) {
    if (checkpoint.length > 32 * 1024 ||
        !info.checkpointSealed && checkpoint.isNotEmpty ||
        !_same(hash(checkpoint), info.checkpointSha256))
      _bad();
    return AgentSessionSnapshot(
      info: info,
      events: events,
      gap: gap,
      checkpoint: checkpoint,
    );
  }

  static AgentSessionData _readR2(
    AgentSessionWireRequest q,
    Uint8List bytes,
    List<int> requestHash,
  ) {
    final r = _root(bytes, 128 * 1024, 3, 6, private: false);
    if (r.getUint16Field(0) != 1 ||
        r.getUint16Field(2) != 2 ||
        r.getUint16Field(4) != 7 ||
        !_same(r.getDataField(0) ?? [], r2Schema) ||
        !_same(r.getDataField(1) ?? [], runtimeDigest) ||
        !_same(r.getDataField(2) ?? [], contentDigest) ||
        q.pendingRequestId == null ||
        r.getTextField(3) != _id(q.pendingRequestId!) ||
        !_same(r.getDataField(4) ?? [], requestHash) ||
        r.u64(16) != q.expectedGeneration)
      _bad();
    switch (r.getUint16Field(6)) {
      case 0:
        final info = r.struct(5);
        if (info == null) _bad();
        return _info(info, r2: true);
      case 1:
        final rows = r.structs(5);
        if (rows == null || rows.length > 16) _bad();
        return AgentSessionInfos([
          for (final row in rows) _info(row, r2: true),
        ]);
      case 2:
        final snap = r.struct(5);
        if (snap == null) _bad();
        snap.shape(1, 3);
        final info = snap.struct(0), rows = snap.structs(1);
        if (info == null || rows == null || rows.length > 16) _bad();
        final events = <AgentSessionStoredEvent>[];
        for (final row in rows) {
          row.shape(1, 1);
          final event = row.struct(0);
          if (event == null) _bad();
          event.shape(0, 2);
          events.add(
            AgentSessionStoredEvent(
              row.u64(0),
              AgentSessionEvent(
                event.getTextField(0) ?? '',
                event.getDataField(1) ?? [],
              ),
            ),
          );
        }
        return _snapshot(
          _info(info, r2: true),
          events,
          snap.getBoolField(0),
          snap.getDataField(2) ?? [],
        );
      case 5:
        final code = r.getUint16Field(8);
        if (code > 8 || r.hasPointerField(5)) _bad();
        return AgentSessionRejected(AgentSessionRejectionKind.values[code]);
      default:
        _bad(); // Tool/Claimed and unknown union arms are never session data.
    }
  }

  static bool _sameInfo(AgentSessionInfo a, AgentSessionInfo b) =>
      a.sessionId == b.sessionId &&
      a.parent == b.parent &&
      a.parentTail == b.parentTail &&
      a.epoch == b.epoch &&
      a.tail == b.tail &&
      a.floor == b.floor &&
      a.archived == b.archived &&
      a.revision == b.revision &&
      a.checkpointTail == b.checkpointTail &&
      a.checkpointSealed == b.checkpointSealed &&
      _same(a.checkpointSha256, b.checkpointSha256) &&
      _same(a.parentCheckpointSha256, b.parentCheckpointSha256);
  static bool _sameData(AgentSessionData a, AgentSessionData b) {
    if (a is AgentSessionInfo && b is AgentSessionInfo) return _sameInfo(a, b);
    if (a is AgentSessionInfos && b is AgentSessionInfos) {
      return a.sessions.length == b.sessions.length &&
          [
            for (var i = 0; i < a.sessions.length; i++)
              _sameInfo(a.sessions[i], b.sessions[i]),
          ].every((v) => v);
    }
    if (a is AgentSessionSnapshot && b is AgentSessionSnapshot) {
      return _sameInfo(a.info, b.info) &&
          a.gap == b.gap &&
          _same(a.checkpoint, b.checkpoint) &&
          a.events.length == b.events.length &&
          [
            for (var i = 0; i < a.events.length; i++)
              a.events[i].sequence == b.events[i].sequence &&
                  a.events[i].event.id == b.events[i].event.id &&
                  _same(a.events[i].event.body, b.events[i].event.body),
          ].every((v) => v);
    }
    return a is AgentSessionRejected &&
        b is AgentSessionRejected &&
        a.kind == b.kind;
  }

  static void _validateData(
    AgentSessionCommand command,
    AgentSessionData data,
  ) {
    if (data is AgentSessionRejected) return;
    if (command is AgentSessionList && data is AgentSessionInfos) return;
    if (command is AgentSessionCreate &&
        data is AgentSessionInfo &&
        data.sessionId == command.sessionId &&
        data.parent == command.parent &&
        data.parentTail == command.parentTail &&
        data.epoch == BigInt.zero &&
        data.tail == BigInt.zero &&
        data.floor == BigInt.one &&
        !data.archived &&
        data.checkpointTail == BigInt.zero &&
        (command.parent == null
            ? !data.checkpointSealed
            : data.checkpointSealed &&
                  _same(data.checkpointSha256, data.parentCheckpointSha256)))
      return;
    if (command is AgentSessionOpenWriter &&
        data is AgentSessionInfo &&
        data.sessionId == command.sessionId &&
        data.epoch == command.expectedEpoch + BigInt.one &&
        !data.archived)
      return;
    if (command is AgentSessionAppend &&
        data is AgentSessionInfo &&
        data.sessionId == command.sessionId &&
        data.epoch == command.epoch &&
        data.tail ==
            command.expectedTail + BigInt.from(command.events.length) &&
        !data.archived)
      return;
    if (command is AgentSessionSnapshotRequest &&
        data is AgentSessionSnapshot &&
        data.info.sessionId == command.sessionId &&
        command.after <= data.info.tail) {
      final start = command.after + BigInt.one > data.info.floor
          ? command.after + BigInt.one
          : data.info.floor;
      final available = start > data.info.tail
          ? BigInt.zero
          : data.info.tail - start + BigInt.one;
      final count = available < BigInt.from(command.limit)
          ? available.toInt()
          : command.limit;
      if (data.gap == (command.after + BigInt.one < data.info.floor) &&
          data.events.length == count &&
          (data.events.isEmpty || data.events.first.sequence == start))
        return;
    }
    _bad();
  }
}
