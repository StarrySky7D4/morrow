import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/editor_draft.dart';
import 'package:morrow_studio/plugins/editor_draft_binding.dart';
import 'package:morrow_studio/plugins/editor_draft_session.dart';

EditorDraftTextValue _text(
  String value, {
  int base = -1,
  int extent = -1,
  int start = -1,
  int end = -1,
  int affinity = 1,
  bool directional = false,
}) => EditorDraftTextValue(
  text: value,
  selectionBase: base,
  selectionExtent: extent,
  affinity: affinity,
  directional: directional,
  composingStart: start,
  composingEnd: end,
);

EditorDraftSnapshot _snapshot({
  EditorDraftTextValue? title,
  EditorDraftTextValue? todos,
}) => EditorDraftSnapshot(
  values: EditorDraftValues(
    title: title ?? _text('draft'),
    description: _text('body'),
    hypothesis: _text('hypothesis'),
    conclusion: _text('conclusion'),
    todos: todos ?? _text('one\ntwo'),
    category: 'category',
    stage: 'stage',
  ),
  assets: const [],
);

final class _Control implements EditorDraftControl {
  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw StateError('Binding must not invoke a host operation by itself');
}

class _Harness {
  _Harness(EditorDraftSnapshot snapshot) {
    session = EditorDraftSession.newSession(
      control: _Control(),
      cardId: 'card',
      draftId: 'draft',
      sourceRevision: BigInt.one,
      initialSnapshot: snapshot,
      operationFactory: () => 'save-${++sequence}',
      isCurrent: () => current,
      debounce: null,
    );
    controllers = [
      for (final raw in [
        snapshot.values.title,
        snapshot.values.description,
        snapshot.values.hypothesis,
        snapshot.values.conclusion,
        snapshot.values.todos,
      ])
        TextEditingController(text: raw.text),
    ];
    metadata = EditorDraftMetadata(
      category: snapshot.values.category,
      stage: snapshot.values.stage,
      assets: snapshot.assets,
    );
    binding = EditorDraftBinding(
      session: session,
      title: controllers[0],
      description: controllers[1],
      hypothesis: controllers[2],
      conclusion: controllers[3],
      todos: controllers[4],
      isCurrent: () => current,
      readMetadata: () {
        if (metadataError) throw StateError('Selected attachment unresolved');
        return metadata;
      },
    );
  }
  int sequence = 0;
  bool current = true, metadataError = false;
  late final EditorDraftSession session;
  late final List<TextEditingController> controllers;
  late final EditorDraftBinding binding;
  late EditorDraftMetadata metadata;
  void dispose() {
    binding.dispose();
    for (final controller in controllers) {
      controller.dispose();
    }
    session.dispose();
  }
}

void main() {
  test(
    'restore cannot clear newer incomplete input from a session listener',
    () {
      final h = _Harness(_snapshot());
      addTearDown(h.dispose);
      h.binding.attach();
      h.session.markCaptureIncomplete(StateError('before restore'));
      var newer = false;
      h.session.addListener(() {
        if (newer) return;
        newer = true;
        h.metadataError = true;
        h.controllers[0].text = 'newer input during restore notification';
      });
      expect(
        () => h.binding.applyCurrentToView((metadata) => h.metadata = metadata),
        throwsStateError,
      );
      expect(h.session.captureBlocked, isTrue);
      expect(h.binding.hasUncapturedChanges, isTrue);
      expect(h.controllers[0].text, 'newer input during restore notification');
    },
  );

  test(
    'reentrant incomplete capture cannot be cleared by an older callback',
    () {
      final h = _Harness(_snapshot());
      addTearDown(h.dispose);
      h.binding.attach();
      var nested = false;
      h.session.addListener(() {
        if (nested) return;
        nested = true;
        h.metadataError = true;
        h.controllers[1].text = 'new unresolved body';
      });
      h.controllers[0].text = 'first observation';
      expect(h.session.captureBlocked, isTrue);
      expect(h.binding.hasUncapturedChanges, isTrue);
      expect(h.binding.captureFailure, isNotNull);
      expect(h.controllers[1].text, 'new unresolved body');
    },
  );

  test('partial metadata restore does not certify a complete snapshot', () {
    final h = _Harness(_snapshot());
    addTearDown(h.dispose);
    h.binding.attach();
    expect(
      () => h.binding.applyCurrentToView((_) {
        h.metadata = EditorDraftMetadata(
          category: 'wrong',
          stage: 'stage',
          assets: [],
        );
      }),
      throwsStateError,
    );
    expect(h.session.current.values.category, 'category');
    expect(h.session.captureBlocked, isTrue);
    expect(h.binding.hasUncapturedChanges, isTrue);
  });

  test('explicit complete restore works before listener attachment', () {
    final h = _Harness(_snapshot());
    addTearDown(h.dispose);
    h.session.markCaptureIncomplete(StateError('old capture failed'));
    h.controllers[0].text = 'replace only through explicit restore';
    h.binding.applyCurrentToView((metadata) => h.metadata = metadata);
    expect(h.controllers[0].text, 'draft');
    expect(h.session.captureBlocked, isFalse);
    expect(h.binding.hasUncapturedChanges, isFalse);
    expect(h.binding.attached, isFalse);
  });

  test('attaching a disposed session does not retain controller listeners', () {
    final h = _Harness(_snapshot());
    addTearDown(h.dispose);
    h.session.dispose();
    expect(h.binding.attach, throwsStateError);
    expect(h.binding.attached, isFalse);
    h.controllers[0].text = 'unobserved';
    expect(h.binding.hasUncapturedChanges, isFalse);
    expect(h.session.current.values.title.text, 'draft');
  });

  test('attachment of a view does not restore stale text or write a draft', () {
    final h = _Harness(_snapshot());
    addTearDown(h.dispose);
    h.controllers[0].text = 'new local view';
    h.binding.attach();
    expect(h.controllers[0].text, 'new local view');
    expect(h.session.current.values.title.text, 'draft');
    expect(h.session.localGeneration, 0);
    expect(h.session.confirmed, isNull);
    expect(h.binding.capture(), isTrue);
    expect(h.session.current.values.title.text, 'new local view');
    expect(h.session.localGeneration, 1);
  });

  test(
    'selection affinity direction and composing changes reach the session intact',
    () {
      final h = _Harness(_snapshot(title: _text('x😀y')));
      addTearDown(h.dispose);
      h.binding.attach();
      h.controllers[0].value = const TextEditingValue(
        text: 'x😀y',
        selection: TextSelection(
          baseOffset: 2,
          extentOffset: 1,
          affinity: TextAffinity.upstream,
          isDirectional: true,
        ),
        composing: TextRange(start: 1, end: 3),
      );
      final value = h.session.current.values.title;
      expect(value.text, 'x😀y');
      expect(
        value.selectionBase,
        2,
      ); // UTF-16 code unit, even inside emoji pair.
      expect(value.selectionExtent, 1);
      expect(value.affinity, TextAffinity.upstream.index);
      expect(value.directional, isTrue);
      expect(value.composingStart, 1);
      expect(value.composingEnd, 3);
      final before = h.session.localGeneration;
      h.controllers[0].selection = const TextSelection.collapsed(offset: 4);
      expect(h.session.localGeneration, before + 1);
      expect(h.session.current.values.title.selectionBase, 4);
      h.binding.capture();
      expect(h.session.localGeneration, before + 1);
    },
  );

  test('metadata failure keeps raw view and reports uncaptured changes', () {
    final h = _Harness(_snapshot());
    addTearDown(h.dispose);
    h.binding.attach();
    h.metadataError = true;
    h.controllers[0].text = 'never drop this input';
    expect(h.binding.hasUncapturedChanges, isTrue);
    expect(h.binding.captureFailure, isA<StateError>());
    expect(h.controllers[0].text, 'never drop this input');
    expect(h.session.current.values.title.text, 'draft');
    h.metadataError = false;
    h.metadata = EditorDraftMetadata(
      category: 'changed',
      stage: 'new',
      assets: [
        EditorDraftAssetSelection(
          origin: EditorDraftAssetOrigin.source,
          assetId: 'asset',
          aliases: ['a\u0000b'],
        ),
      ],
    );
    expect(h.binding.capture(), isTrue);
    expect(h.binding.hasUncapturedChanges, isFalse);
    expect(h.binding.captureFailure, isNull);
    expect(h.session.current.values.title.text, 'never drop this input');
    expect(h.session.current.values.category, 'changed');
    expect(h.session.current.assets.single.assetId, 'asset');
  });

  test(
    'explicit restore applies the entire raw snapshot without feedback writes',
    () {
      final raw = _text(
        'x😀y',
        base: 3,
        extent: 1,
        start: 1,
        end: 3,
        affinity: TextAffinity.upstream.index,
        directional: true,
      );
      final h = _Harness(_snapshot(title: raw));
      addTearDown(h.dispose);
      h.controllers[0].text = 'view before restore';
      h.binding.attach();
      final generation = h.session.localGeneration;
      h.binding.applyCurrentToView((metadata) {
        h.metadata = metadata;
        expect(
          h.binding.capture(),
          isFalse,
          reason: 'Metadata listeners must not capture half-restored text',
        );
      });
      expect(
        h.controllers[0].value,
        const TextEditingValue(
          text: 'x😀y',
          selection: TextSelection(
            baseOffset: 3,
            extentOffset: 1,
            affinity: TextAffinity.upstream,
            isDirectional: true,
          ),
          composing: TextRange(start: 1, end: 3),
        ),
      );
      expect(h.controllers[1].text, 'body');
      expect(h.controllers[4].text, 'one\ntwo');
      expect(h.session.localGeneration, generation);
      expect(h.session.confirmed, isNull);
    },
  );

  test(
    'invalid restore is rejected before any field or metadata is changed',
    () {
      final h = _Harness(_snapshot(todos: _text('one', base: 100)));
      addTearDown(h.dispose);
      h.controllers[0].text = 'keep local input';
      h.binding.attach();
      var metadataApplied = false;
      expect(
        () => h.binding.applyCurrentToView((_) => metadataApplied = true),
        throwsFormatException,
      );
      expect(metadataApplied, isFalse);
      expect(h.controllers[0].text, 'keep local input');
      // Validation changed no raw field, but invalidated close/save eligibility.
      expect(h.session.localGeneration, 1);
      expect(h.session.captureBlocked, isTrue);
    },
  );

  test(
    'detach reattach and binding disposal preserve separately owned session',
    () {
      final h = _Harness(_snapshot());
      addTearDown(h.dispose);
      h.binding.attach();
      h.controllers[0].text = 'session survives';
      h.binding.detach();
      h.controllers[0].text = 'not observed yet';
      expect(h.session.current.values.title.text, 'session survives');
      expect(h.session.disposed, isFalse);
      var notifications = 0;
      h.session.addListener(() => notifications++);
      h.binding.attach();
      h.binding.capture();
      expect(h.session.current.values.title.text, 'not observed yet');
      expect(notifications, greaterThan(0));
      h.binding.dispose();
      h.controllers[0].text = 'controller is still alive';
      expect(h.session.disposed, isFalse);
      expect(h.session.current.values.title.text, 'not observed yet');
    },
  );

  test(
    'old workspace cannot capture or apply and invalid Unicode stays raw locally',
    () {
      final h = _Harness(_snapshot());
      addTearDown(h.dispose);
      h.binding.attach();
      final invalid = String.fromCharCode(0xd800);
      h.controllers[0].text = invalid;
      expect(h.session.current.values.title.text.codeUnits, [0xd800]);
      h.current = false;
      h.controllers[0].text = 'still visible';
      expect(h.binding.hasUncapturedChanges, isTrue);
      expect(h.session.current.values.title.text.codeUnits, [0xd800]);
      expect(() => h.binding.applyCurrentToView((_) {}), throwsStateError);
      expect(h.controllers[0].text, 'still visible');
    },
  );
}
