import 'dart:convert';
import 'dart:typed_data';

// Presentation data, never a host grant. Limits follow frozen R2 lib.rs.
abstract final class AgentSessionValidation {
  static final maxU64 = (BigInt.one << 64) - BigInt.one;
  static final emptySha256 = Uint8List.fromList([
    for (var i = 0; i < 64; i += 2)
      int.parse(
        'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855'
            .substring(i, i + 2),
        radix: 16,
      ),
  ]).asUnmodifiableView();
  static BigInt u64(BigInt value) {
    if (value < BigInt.zero || value > maxU64) {
      throw const FormatException('Invalid session counter');
    }
    return value;
  }

  static String identity(String value) {
    if (value.isEmpty ||
        utf8.encode(value).length > 256 ||
        utf8.decode(utf8.encode(value)) != value ||
        value.runes.any(
          (c) =>
              c < 32 || c >= 127 && c <= 159 || c == 47 || c == 92 || c == 58,
        )) {
      throw const FormatException('Invalid session identity');
    }
    return value;
  }

  static Uint8List bytes(List<int> value, int max, {bool nonzero32 = false}) {
    if (value.length > max ||
        value.any((b) => b < 0 || b > 255) ||
        nonzero32 && (value.length != 32 || value.every((b) => b == 0))) {
      throw const FormatException('Invalid session bytes');
    }
    return Uint8List.fromList(value).asUnmodifiableView();
  }

  static bool same(List<int> a, List<int> b) {
    if (a.length != b.length) return false;
    for (var i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }
}

final class AgentSessionEvent {
  AgentSessionEvent(String id, List<int> body)
    : id = AgentSessionValidation.identity(id),
      body = AgentSessionValidation.bytes(body, 32 * 1024);
  final String id;
  final Uint8List body;
}

/// The bridge builds and validates canonical R2. No arbitrary wire payload,
/// Claim, Propose, Report, Inspect, permission edit or native Start is exposed.
sealed class AgentSessionCommand {
  const AgentSessionCommand();
}

final class AgentSessionCreate extends AgentSessionCommand {
  AgentSessionCreate(String sessionId, {String? parent, BigInt? parentTail})
    : sessionId = AgentSessionValidation.identity(sessionId),
      parent = parent == null ? null : AgentSessionValidation.identity(parent),
      parentTail = AgentSessionValidation.u64(parentTail ?? BigInt.zero) {
    if (parent == null && this.parentTail != BigInt.zero ||
        parent == sessionId) {
      throw const FormatException('Invalid session parent');
    }
  }
  final String sessionId;
  final String? parent;
  final BigInt parentTail;
}

final class AgentSessionList extends AgentSessionCommand {
  const AgentSessionList();
}

final class AgentSessionSnapshotRequest extends AgentSessionCommand {
  AgentSessionSnapshotRequest(
    String sessionId, {
    required BigInt after,
    int limit = 16,
    this.writerSlot,
  }) : sessionId = AgentSessionValidation.identity(sessionId),
       after = AgentSessionValidation.u64(after),
       limit = limit {
    if (after == AgentSessionValidation.maxU64 || limit < 1 || limit > 16) {
      throw const FormatException('Invalid session snapshot range');
    }
    if (writerSlot != null &&
        AgentSessionValidation.u64(writerSlot!) == BigInt.zero) {
      throw const FormatException('Invalid writer slot');
    }
  }
  final String sessionId;
  final BigInt after;
  final int limit;
  final BigInt? writerSlot;
}

final class AgentSessionAdmitWriter extends AgentSessionCommand {
  AgentSessionAdmitWriter(String sessionId)
    : sessionId = AgentSessionValidation.identity(sessionId);
  final String sessionId;
}

final class AgentSessionOpenWriter extends AgentSessionCommand {
  AgentSessionOpenWriter(String sessionId, BigInt slot, BigInt expectedEpoch)
    : sessionId = AgentSessionValidation.identity(sessionId),
      writerSlot = AgentSessionValidation.u64(slot),
      expectedEpoch = AgentSessionValidation.u64(expectedEpoch) {
    if (slot == BigInt.zero || expectedEpoch == AgentSessionValidation.maxU64) {
      throw const FormatException('Invalid writer opening');
    }
  }
  final String sessionId;
  final BigInt writerSlot, expectedEpoch;
}

final class AgentSessionAppend extends AgentSessionCommand {
  AgentSessionAppend(
    String sessionId,
    BigInt slot,
    BigInt epoch,
    BigInt expectedTail,
    List<AgentSessionEvent> events,
  ) : sessionId = AgentSessionValidation.identity(sessionId),
      writerSlot = AgentSessionValidation.u64(slot),
      epoch = AgentSessionValidation.u64(epoch),
      expectedTail = AgentSessionValidation.u64(expectedTail),
      events = List.unmodifiable(events) {
    if (slot == BigInt.zero ||
        epoch == BigInt.zero ||
        events.isEmpty ||
        events.length > 16 ||
        events.map((e) => e.id).toSet().length != events.length ||
        expectedTail + BigInt.from(events.length) >
            AgentSessionValidation.maxU64) {
      throw const FormatException('Invalid session append');
    }
  }
  final String sessionId;
  final BigInt writerSlot, epoch, expectedTail;
  final List<AgentSessionEvent> events;
}

final class AgentSessionCloseWriter extends AgentSessionCommand {
  AgentSessionCloseWriter(String sessionId, BigInt slot)
    : sessionId = AgentSessionValidation.identity(sessionId),
      writerSlot = AgentSessionValidation.u64(slot) {
    if (slot == BigInt.zero) throw const FormatException('Invalid writer slot');
  }
  final String sessionId;
  final BigInt writerSlot;
}

sealed class AgentSessionData {
  const AgentSessionData();
}

final class AgentSessionInfo extends AgentSessionData {
  AgentSessionInfo({
    required String sessionId,
    required BigInt epoch,
    required BigInt tail,
    required BigInt floor,
    required this.archived,
    required BigInt revision,
    String? parent,
    BigInt? parentTail,
    BigInt? checkpointTail,
    List<int>? checkpointSha256,
    this.checkpointSealed = false,
    List<int> parentCheckpointSha256 = const [],
  }) : sessionId = AgentSessionValidation.identity(sessionId),
       epoch = AgentSessionValidation.u64(epoch),
       tail = AgentSessionValidation.u64(tail),
       floor = AgentSessionValidation.u64(floor),
       revision = AgentSessionValidation.u64(revision),
       parent = parent == null ? null : AgentSessionValidation.identity(parent),
       parentTail = AgentSessionValidation.u64(parentTail ?? BigInt.zero),
       checkpointTail = AgentSessionValidation.u64(
         checkpointTail ?? BigInt.zero,
       ),
       checkpointSha256 = AgentSessionValidation.bytes(
         checkpointSha256 ?? AgentSessionValidation.emptySha256,
         32,
       ),
       parentCheckpointSha256 = AgentSessionValidation.bytes(
         parentCheckpointSha256,
         32,
       ) {
    if (revision == BigInt.zero ||
        floor == BigInt.zero ||
        floor > tail + BigInt.one ||
        this.checkpointTail > tail ||
        parent == null && this.parentTail != BigInt.zero ||
        parent == sessionId ||
        this.checkpointSha256.length != 32 ||
        (parent != null) != parentCheckpointSha256.isNotEmpty ||
        parentCheckpointSha256.isNotEmpty &&
            parentCheckpointSha256.length != 32 ||
        floor > BigInt.one &&
            (!checkpointSealed || this.checkpointTail + BigInt.one < floor) ||
        !checkpointSealed &&
            (this.checkpointTail != BigInt.zero ||
                !AgentSessionValidation.same(
                  this.checkpointSha256,
                  AgentSessionValidation.emptySha256,
                ))) {
      throw const FormatException('Invalid session metadata');
    }
  }
  final String sessionId;
  final BigInt epoch, tail, floor, revision;
  final bool archived;
  final String? parent;
  final BigInt parentTail, checkpointTail;
  final Uint8List checkpointSha256, parentCheckpointSha256;
  final bool checkpointSealed;
}

final class AgentSessionInfos extends AgentSessionData {
  AgentSessionInfos(List<AgentSessionInfo> sessions)
    : sessions = List.unmodifiable(sessions) {
    if (sessions.length > 16 ||
        sessions.map((s) => s.sessionId).toSet().length != sessions.length) {
      throw const FormatException('Invalid session list');
    }
  }
  final List<AgentSessionInfo> sessions;
}

final class AgentSessionStoredEvent {
  AgentSessionStoredEvent(BigInt sequence, this.event)
    : sequence = AgentSessionValidation.u64(sequence) {
    if (sequence == BigInt.zero)
      throw const FormatException('Invalid event sequence');
  }
  final BigInt sequence;
  final AgentSessionEvent event;
}

final class AgentSessionSnapshot extends AgentSessionData {
  AgentSessionSnapshot({
    required this.info,
    required List<AgentSessionStoredEvent> events,
    required this.gap,
    List<int> checkpoint = const [],
  }) : events = List.unmodifiable(events),
       checkpoint = AgentSessionValidation.bytes(checkpoint, 32 * 1024) {
    if (events.length > 16 ||
        events.map((e) => e.event.id).toSet().length != events.length) {
      throw const FormatException('Invalid snapshot events');
    }
    BigInt? previous;
    for (final e in events) {
      if (e.sequence < info.floor ||
          e.sequence > info.tail ||
          previous != null && e.sequence != previous + BigInt.one) {
        throw const FormatException('Invalid snapshot sequence');
      }
      previous = e.sequence;
    }
  }
  final AgentSessionInfo info;
  final List<AgentSessionStoredEvent> events;
  final bool gap;
  final Uint8List checkpoint;
}

final class AgentSessionWriterAdmitted extends AgentSessionData {
  AgentSessionWriterAdmitted(String sessionId, BigInt slot)
    : sessionId = AgentSessionValidation.identity(sessionId),
      slot = AgentSessionValidation.u64(slot) {
    if (slot == BigInt.zero) throw const FormatException('Invalid writer slot');
  }
  final String sessionId;
  // Local controller slot, not endpoint identity or authority.
  final BigInt slot;
}

final class AgentSessionWriterClosed extends AgentSessionData {
  const AgentSessionWriterClosed();
}

enum AgentSessionRejectionKind {
  invalid,
  contract,
  limit,
  correlation,
  denied,
  conflict,
  notFound,
  commitUnknown,
  storage,
}

final class AgentSessionRejected extends AgentSessionData {
  const AgentSessionRejected(this.kind);
  final AgentSessionRejectionKind kind;
  bool get uncertain =>
      kind == AgentSessionRejectionKind.commitUnknown ||
      kind == AgentSessionRejectionKind.storage;
}
