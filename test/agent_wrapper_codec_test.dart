import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/agent_wrapper_codec_native.dart';
import 'package:morrow_studio/plugins/agent_wrapper_models.dart';
import 'agent_wrapper_test_helpers.dart';

void main() {
  test('decoder rejects unselected page entry retaining approved authority', () {
    final rev = revisions();
    final request = AgentWrapperRequest(
      action: AgentWrapperAction.page,
      id: List.filled(16, 1),
      revisions: rev,
      limit: 16,
    );
    final reply = AgentWrapperReply(
      action: request.action,
      status: AgentWrapperStatus.ok,
      id: request.id,
      requestSha256: AgentWrapperCodec.hash(
        AgentWrapperCodec.encodeRequest(request),
      ),
      result: AgentWrapperResult(
        revisions: rev,
        page: AgentWrapperPage(
          revisions: rev,
          entries: [entry(approved: review().declaration)],
        ),
      ),
    );
    final frame = AgentWrapperCodec.encodeReply(reply);
    expect(
      AgentWrapperCodec.decodeReply(
        request,
        frame,
      ).result.page!.entries.single.selected,
      true,
    );
    // Fixed schema Reply.entries pointer: prefix/header/root/data + five pointer words.
    // Follow its inline-composite list tag, then clear only Entry.selected bit 0.
    final data = ByteData.sublistView(frame);
    const pointer = 8 + 8 + 8 + 8 + 5 * 8;
    final listTag =
        pointer + 8 + (data.getInt32(pointer, Endian.little) >> 2) * 8;
    final entryData = listTag + 8;
    expect(frame[entryData] & 1, 1);
    frame[entryData] &= ~1;
    expect(
      () => AgentWrapperCodec.decodeReply(request, frame),
      throwsFormatException,
    );
  });
  test('u64 versions above JavaScript precision retain every bit', () {
    final expected = revisions(
      catalog: (BigInt.one << 64) - BigInt.one,
      manager: (BigInt.one << 63) + BigInt.from(123),
    );
    final request = AgentWrapperRequest(
      action: AgentWrapperAction.page,
      id: List.filled(16, 1),
      revisions: expected,
      limit: 16,
    );
    final encoded = AgentWrapperCodec.encodeRequest(request);
    final decoded = AgentWrapperCodec.decodeRequest(encoded);
    expect(decoded.revisions!.catalog, expected.catalog);
    expect(decoded.revisions!.manager, expected.manager);
    final reply = AgentWrapperReply(
      action: request.action,
      status: AgentWrapperStatus.ok,
      id: request.id,
      requestSha256: AgentWrapperCodec.hash(encoded),
      result: AgentWrapperResult(
        revisions: expected,
        page: AgentWrapperPage(revisions: expected, entries: []),
      ),
    );
    expect(
      AgentWrapperCodec.decodeReply(
        request,
        AgentWrapperCodec.encodeReply(reply),
      ).result.revisions.manager,
      expected.manager,
    );
  });
  test(
    'wrong identity, request SHA, action and trailing bytes reject delivery',
    () {
      final request = AgentWrapperRequest(
        action: AgentWrapperAction.state,
        id: List.filled(16, 1),
      );
      AgentWrapperReply reply({
        List<int>? id,
        List<int>? digest,
        AgentWrapperAction? action,
      }) => AgentWrapperReply(
        action: action ?? request.action,
        status: AgentWrapperStatus.ok,
        id: id ?? request.id,
        requestSha256:
            digest ??
            AgentWrapperCodec.hash(AgentWrapperCodec.encodeRequest(request)),
        result: AgentWrapperResult(revisions: revisions()),
      );
      for (final bad in [
        reply(id: List.filled(16, 2)),
        reply(digest: List.filled(32, 3)),
        reply(action: AgentWrapperAction.remove),
      ]) {
        expect(
          () => AgentWrapperCodec.decodeReply(
            request,
            AgentWrapperCodec.encodeReply(bad),
          ),
          throwsFormatException,
        );
      }
      final frame = AgentWrapperCodec.encodeReply(reply());
      expect(
        () => AgentWrapperCodec.decodeReply(
          request,
          Uint8List.fromList([...frame, ...List.filled(8, 0)]),
        ),
        throwsFormatException,
      );
      final badSchema = Uint8List.fromList(frame)..[16 + 8 + 8 + 7 * 8] ^= 1;
      expect(
        () => AgentWrapperCodec.decodeReply(request, badSchema),
        throwsFormatException,
      );
    },
  );
  test(
    'request unused fields, oversized path, invalid subsets and scopes reject',
    () {
      expect(
        () => AgentWrapperRequest(
          action: AgentWrapperAction.state,
          id: List.filled(16, 1),
          path: 'private',
        ),
        throwsFormatException,
      );
      expect(
        () => AgentWrapperRequest(
          action: AgentWrapperAction.inspect,
          id: List.filled(16, 1),
          path: 'x' * 4097,
        ),
        throwsFormatException,
      );
      expect(
        () => AgentWrapperApproval(
          sessionBits: 32,
          processBits: 0,
          sessions: ['main'],
          executionDomain: 'synthetic:test',
        ),
        throwsFormatException,
      );
      expect(
        () => AgentWrapperApproval(
          sessionBits: 1,
          processBits: 0,
          sessions: ['z', 'a'],
          executionDomain: 'synthetic:test',
        ),
        throwsFormatException,
      );
      expect(
        () => AgentWrapperApproval(
          sessionBits: 1,
          processBits: 0,
          sessions: ['a', 'a'],
          executionDomain: 'synthetic:test',
        ),
        throwsFormatException,
      );
      expect(
        () => AgentWrapperApproval(
          sessionBits: 1,
          processBits: 0,
          sessions: ['main/escape'],
          executionDomain: 'synthetic:test',
        ),
        throwsFormatException,
      );
      expect(
        () => AgentWrapperEntry(
          review: review(),
          selected: true,
          enabled: false,
          baseSelected: true,
          baseEnabled: true,
          approved: AgentWrapperApproval(
            sessionBits: 1,
            processBits: 4,
            sessions: ['main'],
            executionDomain: 'synthetic:test',
          ),
        ),
        throwsFormatException,
      );
    },
  );
  test('page order, cursor and expected revision bound the snapshot', () {
    final request = AgentWrapperRequest(
      action: AgentWrapperAction.page,
      id: List.filled(16, 1),
      revisions: revisions(),
      limit: 16,
    );
    final hash = AgentWrapperCodec.hash(
      AgentWrapperCodec.encodeRequest(request),
    );
    for (final page in [
      AgentWrapperPage(
        revisions: revisions(),
        entries: [
          entry(package: review(seed: 2)),
          entry(),
        ],
      ),
      AgentWrapperPage(
        revisions: revisions(),
        entries: [entry()],
        cursor: review(seed: 2).key,
      ),
      AgentWrapperPage(
        revisions: revisions(catalog: BigInt.two),
        entries: [],
      ),
    ]) {
      final reply = AgentWrapperReply(
        action: request.action,
        status: AgentWrapperStatus.ok,
        id: request.id,
        requestSha256: hash,
        result: AgentWrapperResult(revisions: page.revisions, page: page),
      );
      expect(
        () => AgentWrapperCodec.decodeReply(
          request,
          AgentWrapperCodec.encodeReply(reply),
        ),
        throwsFormatException,
      );
    }
  });
  test(
    'large legal pages use one canonical segment and separate same-base wrappers',
    () {
      final rev = revisions();
      final request = AgentWrapperRequest(
        action: AgentWrapperAction.page,
        id: List.filled(16, 1),
        revisions: rev,
        limit: 16,
      );
      final entries = [
        for (var i = 1; i <= 16; i++)
          AgentWrapperEntry(
            review: AgentWrapperReview(
              id: 'org.example.agent',
              version: '1.0.0',
              wrapperSha256: List.filled(32, i),
              baseSha256: List.filled(32, 7),
              sessionSchema: List.filled(32, 8),
              processSchema: List.filled(32, 9),
              declaration: AgentWrapperApproval(
                sessionBits: 31,
                processBits: 127,
                sessions: List.generate(
                  16,
                  (n) => 's${n.toString().padLeft(2, '0')}${'x' * 100}',
                ),
                executionDomain: 'synthetic:test',
              ),
            ),
            selected: false,
            enabled: false,
            baseSelected: true,
            baseEnabled: true,
          ),
      ];
      final reply = AgentWrapperReply(
        action: request.action,
        status: AgentWrapperStatus.ok,
        id: request.id,
        requestSha256: AgentWrapperCodec.hash(
          AgentWrapperCodec.encodeRequest(request),
        ),
        result: AgentWrapperResult(
          revisions: rev,
          page: AgentWrapperPage(revisions: rev, entries: entries),
        ),
      );
      final bytes = AgentWrapperCodec.encodeReply(reply);
      expect(bytes.length, greaterThan(8192));
      expect(ByteData.sublistView(bytes).getUint32(8, Endian.little), 0);
      expect(
        AgentWrapperCodec.decodeReply(
          request,
          bytes,
        ).result.page!.entries.length,
        16,
      );
    },
  );
}
