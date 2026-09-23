import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/versioned_content.dart';
import 'package:morrow_studio/plugins/versioned_idea_view.dart';

VersionedContentRecord _record({
  required int formatVersion,
  String stage = '计划中',
  String projectedStage = '宿主投影阶段',
  List<String> todos = const [],
  List<String> completed = const [],
  List<VersionedTask> tasks = const [],
  List<VersionedAsset> assets = const [],
  VersionedOrigin? origin,
  int completeCount = 0,
  int incompleteCount = 0,
  int ambiguousCount = 0,
}) => VersionedContentRecord(
  id: 'display-card',
  title: 'Card title',
  revision: BigInt.from(7),
  formatVersion: formatVersion,
  description: '**Markdown**',
  category: '进行中',
  stage: stage,
  hypothesis: 'hypothesis',
  conclusion: 'conclusion',
  favorite: true,
  assets: assets,
  icon: 2,
  color: 0xff8866aa,
  deleted: false,
  deletedAt: BigInt.zero,
  todos: todos,
  completed: completed,
  tasks: tasks,
  origin: origin,
  retiredTaskIds: const [],
  projectedStage: projectedStage,
  completeCount: completeCount,
  incompleteCount: incompleteCount,
  ambiguousCount: ambiguousCount,
);

void main() {
  test('legacy duplicate labels retain text membership and host stage', () {
    final source = _record(
      formatVersion: 1,
      stage: '计划中',
      projectedStage: '宿主投影阶段',
      todos: ['same', 'same'],
      completed: ['same'],
      completeCount: 2,
    );
    final view = VersionedIdeaView.fromRecord(source);
    expect(view.source, same(source));
    expect(view.formatVersion, 1);
    expect(view.revision, BigInt.from(7));
    expect(view.stage, '计划中');
    expect(view.projectedStage, source.projectedStage);
    expect(view.completeCount, source.completeCount);
    expect(view.incompleteCount, source.incompleteCount);
    expect(view.ambiguousCount, 0);
    expect(view.tasks, everyElement(isA<LegacyIdeaTaskView>()));
    final first = view.tasks.first as LegacyIdeaTaskView;
    final second = view.tasks.last as LegacyIdeaTaskView;
    expect(first.text, 'same');
    expect(first.completedByName, isTrue);
    expect(second.completedByName, isTrue);
    expect(first.duplicateCount, 2);
    expect(second.duplicateCount, 2);
  });

  test(
    'format 2 preserves distinct TaskIds, ambiguity, origin and u64 bytes',
    () {
      final maxU64 = (BigInt.one << 64) - BigInt.one;
      final origin = VersionedOrigin(
        cardId: 'display-card',
        sourceRevision: BigInt.one,
        sourceSha256: List.filled(32, 3),
        migratorVersion: 1,
        targetVersion: 2,
        originalProperties: const [1, 2, 3],
        mapping: const [
          VersionedTaskMapping(sourceIndex: 0, taskId: 'task-a'),
          VersionedTaskMapping(sourceIndex: 1, taskId: 'task-b'),
        ],
        historicalProjectStage: '旧投影',
        originalTitle: 'Original',
      );
      final asset = VersionedAsset(
        id: 'asset-a',
        name: 'large.bin',
        kind: 'file',
        bytes: maxU64,
      );
      final source = _record(
        formatVersion: 2,
        stage: '待验证',
        projectedStage: '宿主V2阶段',
        origin: origin,
        assets: [asset],
        tasks: const [
          VersionedTask(
            id: 'task-a',
            text: 'same',
            completion: VersionedTaskCompletion.legacyAmbiguous,
            legacyCompleted: true,
            legacyDuplicates: 2,
          ),
          VersionedTask(
            id: 'task-b',
            text: 'same',
            completion: VersionedTaskCompletion.legacyAmbiguous,
            legacyCompleted: true,
            legacyDuplicates: 2,
          ),
        ],
        ambiguousCount: 2,
      );
      final view = VersionedIdeaView.fromRecord(source);
      expect(view.formatVersion, 2);
      expect(view.projectedStage, source.projectedStage);
      expect(view.origin, same(origin));
      expect(view.assets.single.bytes, maxU64);
      expect(view.assets.single, same(asset));
      expect(view.ambiguousCount, source.ambiguousCount);
      expect(view.tasks, everyElement(isA<IdentifiedIdeaTaskView>()));
      final first = view.tasks.first as IdentifiedIdeaTaskView;
      final second = view.tasks.last as IdentifiedIdeaTaskView;
      expect(first.text, second.text);
      expect(first.taskId, 'task-a');
      expect(second.taskId, 'task-b');
      expect(first.needsExplicitDecision, isTrue);
      expect(second.needsExplicitDecision, isTrue);
      expect(() => view.tasks.add(first), throwsUnsupportedError);
    },
  );

  test('invalid display shape and out-of-range asset length fail closed', () {
    expect(
      () => VersionedIdeaView.fromRecord(
        _record(formatVersion: 2, todos: ['legacy'], incompleteCount: 1),
      ),
      throwsFormatException,
    );
    expect(
      () => VersionedIdeaView.fromRecord(
        _record(
          formatVersion: 1,
          assets: [
            VersionedAsset(
              id: 'oversize',
              name: 'large.bin',
              kind: 'file',
              bytes: BigInt.one << 64,
            ),
          ],
        ),
      ),
      throwsFormatException,
    );
  });
}
