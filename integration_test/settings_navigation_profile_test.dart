import 'dart:convert';
import 'dart:io';
import 'dart:ui' show FrameTiming, FramePhase;
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
    'Profile repeated settings navigation on the shared canvas',
    (t) async {
      await initializeDesktopFrame();
      await windowManager.setSize(const Size(1180, 820));
      final samples = <FrameTiming>[];
      void collect(List<FrameTiming> frames) => samples.addAll(frames);
      Future<void> cycle() async {
        await tapVisible(t, find.byKey(const ValueKey('component-settings')));
        await tapVisible(t, find.byKey(const ValueKey('component-entry:hero')));
        await tapVisible(t, find.byType(BackButton));
        await tapVisible(t, find.byType(BackButton));
      }

      try {
        final storage = MemoryStorage()
          ..data = {
            'theme': 'white',
            'glass': Platform.environment['MORROW_PROFILE_GLASS'] ?? 'frosted',
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
        await t.pumpWidget(
          MorrowApp(storage: storage, nativeBackground: DesktopBackground()),
        );
        await tapVisible(t, find.byKey(const ValueKey('settings-expand')));
        WidgetsBinding.instance.addTimingsCallback(collect);
        final stages = <String, List<FrameTiming>>{};
        Future<void> measure(String name, Finder target) async {
          await t.ensureVisible(target);
          await t.pump(const Duration(milliseconds: 450));
          await t.runAsync(
            () => Future<void>.delayed(const Duration(seconds: 1)),
          );
          samples.clear();
          Future<void> action() async {
            if (Platform.environment['MORROW_PROFILE_DIRECT'] == '1') {
              final control = t.widget(target);
              if (control is OutlinedButton) {
                control.onPressed!();
              } else if (control is ListTile) {
                control.onTap!();
              } else {
                await t.tap(target);
              }
            } else {
              await t.tap(target);
            }
            final until = DateTime.now().add(const Duration(milliseconds: 100));
            while (DateTime.now().isBefore(until)) {
              await t.pump(const Duration(milliseconds: 8));
            }
          }

          if (Platform.environment['MORROW_PROFILE_TRACE'] == '1') {
            await binding.traceAction(action, reportKey: name);
          } else {
            await action();
          }
          final settleUntil = DateTime.now().add(
            const Duration(milliseconds: 600),
          );
          while (DateTime.now().isBefore(settleUntil)) {
            await t.pump(const Duration(milliseconds: 8));
          }
          await t.runAsync(
            () => Future<void>.delayed(const Duration(seconds: 1)),
          );
          stages[name] = List.of(samples);
          expect(t.takeException(), isNull, reason: name);
        }

        await measure(
          'cold-list-open',
          find.byKey(const ValueKey('component-settings')),
        );
        await measure(
          'cold-editor-open',
          find.byKey(const ValueKey('component-entry:hero')),
        );
        await measure('editor-back', find.byType(BackButton));
        await measure('list-back', find.byType(BackButton));
        samples.clear();
        for (var i = 0; i < 6; i++) {
          await cycle();
        }
        await t.runAsync(
          () => Future<void>.delayed(const Duration(seconds: 1)),
        );
        expect(samples, isNotEmpty);
        Map<String, Object> summarize(List<double> times) {
          if (times.isEmpty) return {'frames': 0};
          times.sort();
          double percentile(double p) =>
              times[((times.length - 1) * p).round()];
          return {
            'frames': times.length,
            'p50_ms': percentile(.5),
            'p95_ms': percentile(.95),
            'max_ms': times.last,
            'over_16_7_ms': times.where((v) => v > 16.7).length,
          };
        }

        Map<String, Object> stageSummary(List<FrameTiming> frames) => {
          'ui': summarize(
            frames.map((v) => v.buildDuration.inMicroseconds / 1000).toList(),
          ),
          'raster': summarize(
            frames.map((v) => v.rasterDuration.inMicroseconds / 1000).toList(),
          ),
          'frames': frames
              .map(
                (v) => {
                  'vsync_us': v.timestampInMicroseconds(FramePhase.vsyncStart),
                  'ui_us': v.buildDuration.inMicroseconds,
                  'raster_us': v.rasterDuration.inMicroseconds,
                  'total_us': v.totalSpan.inMicroseconds,
                },
              )
              .toList(),
        };
        final evidence = {
          'glass': Platform.environment['MORROW_PROFILE_GLASS'] ?? 'frosted',
          'input': Platform.environment['MORROW_PROFILE_DIRECT'] == '1'
              ? 'direct-callback-diagnostic'
              : 'tester-tap',
          'traced': Platform.environment['MORROW_PROFILE_TRACE'] == '1',
          'stages': {
            for (final entry in stages.entries)
              entry.key: stageSummary(entry.value),
          },
          'ui': summarize(
            samples.map((v) => v.buildDuration.inMicroseconds / 1000).toList(),
          ),
          'raster': summarize(
            samples.map((v) => v.rasterDuration.inMicroseconds / 1000).toList(),
          ),
        };
        final output = Platform.environment['MORROW_PROFILE_OUTPUT']!;
        if (binding.reportData != null) {
          await File(
            '$output.timeline.json',
          ).writeAsString(jsonEncode(binding.reportData));
        }
        await File(
          output,
        ).writeAsString(const JsonEncoder.withIndent('  ').convert(evidence));
        expect(t.takeException(), isNull);
      } finally {
        WidgetsBinding.instance.removeTimingsCallback(collect);
        await t.pumpWidget(const SizedBox());
      }
    },
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
