import 'dart:typed_data';

import 'package:capnproto_dart/capnproto_dart.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/channel_task_manager.dart';
import 'package:morrow_studio/plugins/channel_task_models.dart';
import 'package:morrow_studio/plugins/generated/channel.capnp.dart' as wire;
import 'package:morrow_studio/plugins/generated/identity.dart' as contract;
import 'package:morrow_studio/plugins/plugin_library.dart';

import 'channel_task_session_test.dart' show budget, identity;
import 'plugin_library_test.dart' show FakeBackend, click, page;

PluginTransformHandler directoryHandler({
  String name = 'channel.consume',
  String inputType = 'morrow.channel.directory.v1',
  int capacity = 256,
}) => PluginTransformHandler(
  name: name,
  inputType: inputType,
  outputType: 'summary',
  maxInputBytes: capacity,
  maxOutputBytes: 256,
);

PluginLibraryEntry directoryEntry({
  int capacity = 256,
  List<PluginTransformHandler>? handlers,
  List<String> channelHandlers = const ['channel.consume'],
  List<ChannelSourceKind> kinds = const [
    ChannelSourceKind.byteStream,
    ChannelSourceKind.events,
  ],
  bool budgetPresent = true,
  bool channelSupported = true,
  List<String> declared = const [],
  List<String> approved = const [],
  List<String> declaredIo = const [],
  List<String> approvedIo = const [],
  List<String> ioHandlers = const [],
  List<String> dependencies = const [],
  bool mutationSupported = false,
}) => PluginLibraryEntry(
  id: 'org.example.directory',
  name: 'Directory package',
  version: '1.0.0',
  digest: Uint8List.fromList(identity(99)),
  enabled: true,
  builtin: false,
  available: true,
  declared: declared,
  approved: approved,
  declaredIo: declaredIo,
  approvedIo: approvedIo,
  ioHandlers: ioHandlers,
  dependencies: dependencies,
  handlers: handlers ?? [directoryHandler(capacity: capacity)],
  issue: '',
  channelSupported: channelSupported,
  channelHandlers: channelHandlers,
  channelKinds: kinds,
  channelBudget: budgetPresent ? budget() : null,
  mutationSupported: mutationSupported,
);

final class DirectoryBackend extends FakeBackend implements ChannelTaskBackend {
  DirectoryBackend(super.entries);
  @override
  bool get supportsLocalChannels => true;
  @override
  Future<ChannelTaskSnapshot> prepareChannel(ChannelPrepareRequest request) =>
      throw StateError('No grant was requested');
  @override
  Future<ChannelTaskSnapshot> appendChannel(
    Uint8List key,
    ChannelSourceFrame frame,
  ) => throw StateError('No append was requested');
  @override
  Future<ChannelTaskSnapshot> runChannel(Uint8List key, Uint8List input) =>
      throw StateError('No run was requested');
  @override
  Future<ChannelTaskSnapshot> statusChannel(Uint8List key) =>
      throw StateError('No status was requested');
  @override
  Future<ChannelTaskSnapshot> closeChannel(Uint8List key) =>
      throw StateError('No Close was requested');
  @override
  Future<ChannelSentFrame?> readChannelSent(Uint8List key, BigInt sequence) =>
      throw StateError('No receipt was requested');
}

void main() {
  test('one canonical endpoint requires 256 wire bytes including header', () {
    final message = MessageBuilder();
    final directory = message.initRoot(wire.directoryFactory);
    directory.version = 1;
    directory.schemaSha256 = Uint8List.fromList(contract.channelDigest);
    directory.scopeSha256 = Uint8List.fromList(identity(1));
    final endpoint = directory.initChannels(1)[0];
    endpoint.reference = Uint8List.fromList(identity(2));
    endpoint.sourceEpoch = Uint8List.fromList(identity(3));
    endpoint.kind = wire.Kind.byteStream;
    final declared = budget();
    final ceiling = endpoint.initBudget();
    ceiling.maxChannels = declared.maxChannels;
    ceiling.maxFrameBytes = declared.maxFrameBytes;
    ceiling.maxBytesBigInt = declared.maxBytes;
    ceiling.maxMessagesBigInt = declared.maxMessages;
    ceiling.maxRequestsBigInt = declared.maxRequests;
    ceiling.maxDurationMsBigInt = declared.maxDurationMs;
    final bytes = message.serialize();
    expect(bytes.length, ChannelDirectory.singleEndpointWireBytes);
    expect(bytes.length, 256);
    final header = ByteData.sublistView(bytes);
    expect(header.getUint32(0, Endian.little), 0);
    expect(header.getUint32(4, Endian.little) * 8, 248);
    expect(ChannelDirectory.inputType, 'morrow.channel.directory.v1');
  });

  for (final capacity in [65, 248, 255, 256, 65536, 65537]) {
    test('declared handler Directory capacity $capacity', () {
      final entry = directoryEntry(capacity: capacity);
      expect(
        entry.hasEligibleDirectoryChannels,
        capacity >= 256 && capacity <= 65536,
      );
      expect(entry.available, isTrue);
      expect(entry.enabled, isTrue);
    });
  }

  final ineligible = <String, PluginLibraryEntry>{
    'channel feature missing': directoryEntry(channelSupported: false),
    'content declaration': directoryEntry(declared: ['read-content']),
    'content approval': directoryEntry(approved: ['read-content']),
    'IO declaration': directoryEntry(declaredIo: ['file-read']),
    'IO approval': directoryEntry(approvedIo: ['file-read']),
    'IO handler': directoryEntry(ioHandlers: ['file.read']),
    'dependency call': directoryEntry(dependencies: ['org.example.other']),
    'mutation feature': directoryEntry(mutationSupported: true),
    'budget missing': directoryEntry(budgetPresent: false),
    'kinds missing': directoryEntry(kinds: []),
    'kind duplicated': directoryEntry(
      kinds: [ChannelSourceKind.events, ChannelSourceKind.events],
    ),
    'channel handlers missing': directoryEntry(channelHandlers: []),
    'channel handler empty': directoryEntry(channelHandlers: ['']),
    'channel handler duplicated': directoryEntry(
      channelHandlers: ['channel.consume', 'channel.consume'],
    ),
    'declared handler missing': directoryEntry(handlers: []),
    'declared handler mismatch': directoryEntry(
      channelHandlers: ['channel.other'],
    ),
    'typed handler duplicated': directoryEntry(
      handlers: [directoryHandler(), directoryHandler()],
    ),
    'typed input alias': directoryEntry(
      handlers: [directoryHandler(inputType: 'morrow.channel.directory.v01')],
    ),
    'legacy private 65-byte handler': directoryEntry(
      handlers: [directoryHandler(inputType: 'bytes', capacity: 65)],
    ),
    'another declared handler is not Directory typed': directoryEntry(
      channelHandlers: ['channel.consume', 'channel.private'],
      handlers: [
        directoryHandler(),
        directoryHandler(name: 'channel.private', inputType: 'bytes'),
      ],
    ),
  };
  for (final item in ineligible.entries) {
    test('Directory eligibility fails closed: ${item.key}', () {
      expect(item.value.directoryChannelHandlers, isEmpty);
      expect(item.value.hasEligibleDirectoryChannels, isFalse);
      expect(item.value.available, isTrue);
      expect(item.value.enabled, isTrue);
    });
  }

  test('eligible Directory handlers preserve declared order and ownership', () {
    final entry = directoryEntry(
      channelHandlers: ['channel.events', 'channel.consume'],
      handlers: [
        directoryHandler(),
        directoryHandler(name: 'channel.events'),
      ],
    );
    final selected = entry.directoryChannelHandlers;
    expect(selected.map((handler) => handler.name), [
      'channel.events',
      'channel.consume',
    ]);
    expect(() => selected.clear(), throwsUnsupportedError);
  });

  final catalogCases = <String, (PluginLibraryEntry, bool)>{
    '255-byte Directory': (directoryEntry(capacity: 255), false),
    '256-byte Directory': (directoryEntry(), true),
    '65536-byte Directory': (directoryEntry(capacity: 65536), true),
    '65537-byte Directory': (directoryEntry(capacity: 65537), false),
    'source with content': (directoryEntry(declared: ['read-content']), false),
    'source-only metadata without handler': (
      directoryEntry(handlers: []),
      false,
    ),
    'private legacy route': (
      ineligible['legacy private 65-byte handler']!,
      false,
    ),
  };
  for (final item in catalogCases.entries) {
    testWidgets('only Directory UI is gated: ${item.key}', (tester) async {
      final (entry, eligible) = item.value;
      final backend = DirectoryBackend([entry]);
      await tester.pumpWidget(page(backend));
      await tester.pumpAndSettle();
      await click(tester, 'plugin-entry-${entry.id}');
      final channels = find.byKey(ValueKey('plugin-channel-${entry.id}'));
      expect(channels, findsOneWidget);
      expect(
        tester.widget<OutlinedButton>(channels).onPressed,
        eligible ? isNotNull : isNull,
      );
      final approval = find.byKey(ValueKey('plugin-approve-${entry.id}'));
      final disable = find.byKey(ValueKey('plugin-disable-${entry.id}'));
      expect(tester.widget<OutlinedButton>(approval).onPressed, isNotNull);
      expect(tester.widget<OutlinedButton>(disable).onPressed, isNotNull);
      expect(entry.available, isTrue);
      expect(entry.enabled, isTrue);
      await tester.pumpWidget(const SizedBox.shrink());
    });
  }

  for (final (index, item) in [
    directoryEntry(),
    directoryEntry(capacity: 255),
    directoryEntry(declaredIo: ['file-read']),
  ].indexed) {
    testWidgets(
      'direct manager also gates preparation $index: ${item.hasEligibleDirectoryChannels}',
      (tester) async {
        final backend = DirectoryBackend([item]);
        await tester.pumpWidget(
          MaterialApp(
            home: ChannelTaskManager(
              backend: backend,
              entry: item,
              registryRevision: BigInt.one,
            ),
          ),
        );
        expect(
          tester
              .widget<FilledButton>(find.byKey(const ValueKey('channel-run')))
              .onPressed,
          item.hasEligibleDirectoryChannels ? isNotNull : isNull,
        );
        await tester.pumpWidget(const SizedBox.shrink());
      },
    );
  }
}
