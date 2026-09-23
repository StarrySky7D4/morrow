import 'dart:convert';
import 'dart:io';
import 'dart:ui';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/workspace_viewport.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  testWidgets(
    'profile bounded workspace navigation with growing libraries',
    (t) async {
      final frames = <FrameTiming>[];
      void collect(List<FrameTiming> values) => frames.addAll(values);
      Map<String, Object> summarize(Iterable<double> source) {
        final values = source.toList()..sort();
        return {
          'frames': values.length,
          'p95_ms': values.isEmpty
              ? 0
              : values[((values.length - 1) * .95).round()],
          'max_ms': values.isEmpty ? 0 : values.last,
          'over_16_7_ms': values.where((v) => v > 16.7).length,
          'over_8_3_ms': values.where((v) => v > 8.3).length,
        };
      }

      final results = <Object>[];
      WidgetsBinding.instance.addTimingsCallback(collect);
      try {
        for (final count in [300, 1000, 10000]) {
          final storage = MemoryStorage()
            ..data = {
              'version': 1,
              'theme': 'white',
              'glass': 'frosted',
              'background': 'ambient',
              'ideas': List.generate(
                count,
                (i) => Idea(
                  'Card $i',
                  'A varying description. ' * (1 + i % 4),
                  ['灵感', '进行中', '实验'][i % 3],
                  Idea.icons[i % Idea.icons.length],
                  Colors.purple,
                  id: 'lifecycle-$i',
                  favorite: i % 4 == 0,
                ).toJson(),
              ),
            };
          frames.clear();
          final start = Stopwatch()..start();
          await t.pumpWidget(
            MorrowApp(
              key: ValueKey(count),
              storage: storage,
              initialLocale: const Locale('zh'),
            ),
          );
          await t.pumpAndSettle();
          final readyMs = start.elapsedMilliseconds;
          int cards() => find
              .byWidgetPredicate(
                (w) =>
                    w is Glass && (w.componentId?.startsWith('card:') ?? false),
              )
              .evaluate()
              .length;
          final firstCards = cards();
          expect(firstCards, lessThan(40));
          await t.runAsync(
            () => Future<void>.delayed(const Duration(seconds: 1)),
          );
          frames.clear();
          final residentBytes = <int>[];
          final visibleCounts = <int>[];
          for (var cycle = 0; cycle < 8; cycle++) {
            for (final label in ['灵感收件箱', '小项目', '实验室', '概览']) {
              final target = find.text(label).first;
              await t.ensureVisible(target);
              await t.tap(target);
              await t.pumpAndSettle();
              expect(find.byType(WorkspaceViewport), findsOneWidget);
              visibleCounts.add(cards());
              expect(cards(), lessThan(40));
              expect(t.takeException(), isNull);
            }
            residentBytes.add(ProcessInfo.currentRss);
          }
          await t.runAsync(
            () => Future<void>.delayed(const Duration(seconds: 1)),
          );
          results.add({
            'cards': count,
            'initial_settled_ms': readyMs,
            'initial_mounted_cards': firstCards,
            'mounted_cards_each_switch': visibleCounts,
            'process_rss_bytes_each_cycle': residentBytes,
            'ui': summarize(
              frames.map((v) => v.buildDuration.inMicroseconds / 1000),
            ),
            'raster': summarize(
              frames.map((v) => v.rasterDuration.inMicroseconds / 1000),
            ),
          });
        }
        final evidence = {
          'mode': 'profile',
          'backend': 'MemoryStorage; no plugin or native media tasks',
          'viewport': [t.view.physicalSize.width, t.view.physicalSize.height],
          'results': results,
        };
        await File(
          Platform.environment['MORROW_PROFILE_OUTPUT']!,
        ).writeAsString(const JsonEncoder.withIndent('  ').convert(evidence));
      } finally {
        WidgetsBinding.instance.removeTimingsCallback(collect);
        await t.pumpWidget(const SizedBox());
      }
    },
    timeout: const Timeout(Duration(minutes: 5)),
  );
}
