import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

void main() {
  final executable = Platform.environment['MORROW_WORKBENCH_HOST'];
  final package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  test(
    'actual query operation retries retain original results across changes and reopen',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-query-op-',
      );
      RustWorkbench? backend;
      try {
        backend = await RustWorkbench.open(
          executable: executable!,
          package: package!,
          directory: directory,
        );
        final saved = await backend.apply(
          PluginAction.create,
          Idea(
            'Original result',
            'body',
            '灵感',
            Idea.icons[0],
            Colors.blue,
            id: 'query-operation-card',
          ),
        );
        const operation = 'native-query-fixed-operation';
        expect(
          await backend.query('概览', '全部', '', '最近添加', operation: operation),
          ['query-operation-card'],
        );
        await backend.apply(PluginAction.delete, saved);
        expect(
          await backend.query('概览', '全部', '', '最近添加', operation: operation),
          ['query-operation-card'],
          reason:
              'same operation returns its frozen result, never reruns on current content',
        );
        expect(
          await backend.query(
            '概览',
            '全部',
            '',
            '最近添加',
            operation: 'native-query-new-operation',
          ),
          isEmpty,
        );
        // Callers without an operation also receive a fresh identity for each request.
        expect(await backend.query('概览', '全部', '', '最近添加'), isEmpty);
        await expectLater(
          backend.query(
            '概览',
            '全部',
            'changed intent',
            '最近添加',
            operation: operation,
          ),
          throwsA(
            isA<QueryFailure>().having(
              (error) => error.terminal,
              'terminal',
              isTrue,
            ),
          ),
        );
        await backend.close();
        backend = await RustWorkbench.open(
          executable: executable,
          package: package,
          directory: directory,
        );
        expect(
          await backend.query('概览', '全部', '', '最近添加', operation: operation),
          ['query-operation-card'],
          reason:
              'explicit historical retry survives process restart with current admission',
        );
        expect(await backend.query('概览', '全部', '', '最近添加'), isEmpty);
      } finally {
        await backend?.close();
        await directory.delete(recursive: true);
      }
    },
    skip: executable == null || package == null
        ? 'Requires the actual test.46 Rust host and first-party Wasm bundle'
        : false,
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
