import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/main.dart';
import 'package:morrow_studio/plugins/workbench_backend.dart';
import 'package:morrow_studio/plugins/workbench_ids.dart';
import 'package:morrow_studio/plugins/workbench_native.dart';

void main() {
  final executable = Platform.environment['MORROW_WORKBENCH_HOST'];
  final package = Platform.environment['MORROW_WORKBENCH_PACKAGE'];
  test(
    'actual persisted v1 query intent accepts typed adapter before and after reopen',
    () async {
      final directory = await Directory.systemTemp.createTemp(
        'morrow-query-ids-',
      );
      RustWorkbench? backend;
      const operation = 'legacy-chinese-query-operation';
      Future<List<String>> query(
        RustWorkbench current, {
        String id = operation,
      }) => current.query(
        WorkbenchV1.section(WorkbenchPage.projects),
        WorkbenchV1.filter(const StageFilter(WorkbenchStage.planned)),
        '原始 😀',
        WorkbenchV1.sort(WorkbenchSort.title),
        operation: id,
      );
      try {
        backend = await RustWorkbench.open(
          executable: executable!,
          package: package!,
          directory: directory,
        );
        final saved = await backend.apply(
          PluginAction.create,
          Idea(
            '原始 😀 project',
            'body',
            '进行中',
            Idea.icons.first,
            Colors.blue,
            id: 'legacy-project',
            stage: '计划中',
          ),
        );
        expect(saved.category, '进行中');
        expect(saved.stage, '计划中');
        // Create the persisted intent using the old caller's exact literal fields.
        expect(
          await backend.query(
            '小项目',
            '计划中',
            '原始 😀',
            '标题排序',
            operation: operation,
          ),
          ['legacy-project'],
        );
        expect(await query(backend), ['legacy-project']);
        await backend.apply(PluginAction.delete, saved);
        expect(await query(backend), ['legacy-project']);
        expect(await query(backend, id: 'new-typed-query'), isEmpty);
        await backend.close();
        backend = null;
        backend = await RustWorkbench.open(
          executable: executable,
          package: package,
          directory: directory,
        );
        expect(await query(backend), ['legacy-project']);
        expect(await query(backend, id: 'new-after-reopen'), isEmpty);
      } finally {
        await backend?.close();
        await directory.delete(recursive: true);
      }
    },
    skip: executable == null || package == null
        ? 'Requires the actual Rust host and workbench Wasm bundle'
        : false,
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
