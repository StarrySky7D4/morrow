import 'dart:convert';
import 'dart:developer' as developer;
import 'dart:io';
import 'dart:ui' show FrameTiming, FramePhase;
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/desktop_frame.dart';
import 'package:morrow_studio/storage.dart';
import 'package:morrow_studio/window_effects.dart';
import 'package:window_manager/window_manager.dart';
import 'live_ui_helpers.dart';

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  testWidgets(
    'profile style selection through actual settings controls',
    (t) async {
      expect(
        kProfileMode,
        isTrue,
        reason: 'debug timings are not release evidence',
      );
      await initializeDesktopFrame();
      await windowManager.setSize(const Size(1180, 820));
      final glass = Platform.environment['MORROW_PROFILE_GLASS'] ?? 'frosted';
      final output = Platform.environment['MORROW_PROFILE_OUTPUT']!;
      final trace = Platform.environment['MORROW_PROFILE_TRACE'] == '1';
      final storage = MemoryStorage()
        ..data = {
          'theme': 'white',
          'glass': glass,
          'background': 'ambient',
          'ideas': List.generate(
            300,
            (i) => Idea(
              'Profile card $i',
              '',
              '灵感',
              Idea.icons.first,
              Colors.purple,
              id: 'profile-$i',
            ).toJson(),
          ),
        };
      final samples = <FrameTiming>[];
      void collect(List<FrameTiming> frames) => samples.addAll(frames);
      Future<void> drain() =>
          t.runAsync(() => Future<void>.delayed(const Duration(seconds: 1)));
      Map<String, Object> summary(Iterable<double> values) {
        final times = values.toList()..sort();
        expect(times, isNotEmpty);
        return {
          'frames': times.length,
          'p50_ms': times[((times.length - 1) * .5).round()],
          'p95_ms': times[((times.length - 1) * .95).round()],
          'max_ms': times.last,
          'over_16_7_ms': times.where((x) => x > 16.7).length,
          'over_8_3_ms': times.where((x) => x > 8.3).length,
        };
      }

      final stages = <Map<String, Object>>[];
      try {
        await t.pumpWidget(
          MorrowApp(
            initialLocale: const Locale('zh'),
            storage: storage,
            nativeBackground: DesktopBackground(),
          ),
        );
        await tapVisible(t, find.byKey(const ValueKey('settings-expand')));
        WidgetsBinding.instance.addTimingsCallback(collect);
        for (var round = 0; round < (trace ? 1 : 2); round++) {
          for (final style in VisualStyle.values) {
            if (round == 0 && style == VisualStyle.flat) continue;
            if (trace && style.index > VisualStyle.paper.index) break;
            await tapVisible(
              t,
              find.byKey(const ValueKey('visual-style-toggle')),
            );
            final choice = find.byKey(ValueKey('visual-style-${style.name}'));
            await t.ensureVisible(choice);
            await t.pump(const Duration(milliseconds: 450));
            expect(choice.hitTestable(), findsOneWidget);
            await drain();
            samples.clear();
            final start = developer.Timeline.now;
            Future<void> action() async {
              await t.tap(choice.hitTestable());
              final watch = Stopwatch()..start();
              while (watch.elapsedMilliseconds < 650) {
                await t.pump(const Duration(milliseconds: 8));
              }
            }

            if (trace) {
              await binding.traceAction(
                action,
                reportKey: 'selection-${style.name}',
              );
            } else {
              await action();
            }
            final end = developer.Timeline.now;
            await drain();
            final frames = samples.where((f) {
              final time = f.timestampInMicroseconds(FramePhase.buildStart);
              return time >= start && time <= end;
            }).toList();
            expect(t.takeException(), isNull, reason: '$round/${style.name}');
            expect(storage.data!['visualStyle'], style.name);
            expect(
              t
                  .widget<Studio>(find.byType(Studio))
                  .palette
                  .surfaces
                  .visualStyle,
              style,
            );
            expect(choice.hitTestable(), findsNothing);
            stages.add({
              'round': round,
              'style': style.name,
              'ui': summary(
                frames.map((f) => f.buildDuration.inMicroseconds / 1000),
              ),
              'raster': summary(
                frames.map((f) => f.rasterDuration.inMicroseconds / 1000),
              ),
              'frames': frames
                  .map(
                    (f) => {
                      'build_start_us': f.timestampInMicroseconds(
                        FramePhase.buildStart,
                      ),
                      'ui_us': f.buildDuration.inMicroseconds,
                      'raster_us': f.rasterDuration.inMicroseconds,
                      'total_us': f.totalSpan.inMicroseconds,
                    },
                  )
                  .toList(),
            });
            await File(output).writeAsString(
              const JsonEncoder.withIndent('  ').convert({
                'mode': 'profile',
                'traced': trace,
                'glass': glass,
                'backend':
                    'MemoryStorage; 300 cards; no native plugin or media tasks',
                'input': 'tester taps; actual settings controls',
                'window_logical': [1180, 820],
                'first_pass':
                    'first selection in this process, picker previews already painted; not shader cold start',
                'complete': (trace
                    ? style == VisualStyle.paper
                    : round == 1 && style == VisualStyle.values.last),
                'stages': stages,
              }),
            );
          }
        }
        if (trace) {
          await File(
            '$output.timeline.json',
          ).writeAsString(jsonEncode(binding.reportData));
        }
      } finally {
        WidgetsBinding.instance.removeTimingsCallback(collect);
        await t.pumpWidget(const SizedBox());
      }
    },
    timeout: const Timeout(Duration(minutes: 5)),
  );
}
