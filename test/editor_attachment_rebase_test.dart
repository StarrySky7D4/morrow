import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/attachments/attachment.dart';
import 'package:morrow_studio/content/editor_attachment_rebase.dart';
import 'package:morrow_studio/media/texture_source.dart';

IdeaAttachment local(String location, String name, {int size = 4}) =>
    IdeaAttachment(
      source: TextureSource(
        location: location,
        name: name,
        kind: TextureKind.file,
        local: true,
      ),
      size: size,
    );

IdeaAttachment stored(String id, String name, {int size = 4}) =>
    IdeaAttachment.versioned(
      source: TextureSource(
        location: 'C:/preview/$id.bin',
        name: name,
        kind: TextureKind.file,
        local: true,
      ),
      byteLength: BigInt.from(size),
      pluginId: id,
    );

void main() {
  test('rebinds selected S1 local in S2 order without changing text state', () {
    final original = local('C:/a.bin', 'a.bin');
    final committed = stored('asset-a', 'a.bin');
    final existing = stored('old-id', 'old.bin');
    final newer = local('C:/new.bin', 'new.bin');
    const value = TextEditingValue(
      text: 'Draft without links',
      selection: TextSelection(
        baseOffset: 10,
        extentOffset: 4,
        affinity: TextAffinity.upstream,
        isDirectional: true,
      ),
      composing: TextRange(start: 2, end: 7),
    );

    final rebased = rebaseEditorAttachments(
      submitted: [existing, original],
      confirmed: [existing, committed],
      selected: [newer, original, existing],
      description: value,
    );
    expect(
      committed.source.local,
      isTrue,
    ); // The real presenter exports a local preview.
    expect(rebased.attachments, [same(newer), same(committed), same(existing)]);
    expect(rebased.description, same(value));
    expect(original.pluginId, isNull);
  });

  test('does not convert a nonlocal attachment without a plugin ID', () {
    final invalid = IdeaAttachment(
      source: const TextureSource(
        location: 'https://example.invalid/file.bin',
        name: 'file.bin',
        kind: TextureKind.file,
      ),
      size: 4,
    );
    final canonical = stored('asset-file', 'file.bin');
    const value = TextEditingValue(text: 'attachment:file.bin');
    final rebased = rebaseEditorAttachments(
      submitted: [invalid],
      confirmed: [canonical],
      selected: [invalid],
      description: value,
    );
    expect(rebased.attachments.single, same(invalid));
    expect(rebased.description, same(value));
    // The V2 adapter still rejects null pluginId plus nonlocal source.
  });
  test('rewrites exact raw and encoded URLs while preserving UTF16 ranges', () {
    final original = local('C:/my file.png', 'my file.png');
    final committed = stored('asset-7', 'my file.png');
    const before =
        '🙂 attachment:C:/my file.png and attachment:my%20file.png end';
    final tail = before.indexOf(' end');
    final composingStart = before.indexOf('my%20');
    final composingEnd = composingStart + 'my%20file.png'.length;
    final value = TextEditingValue(
      text: before,
      selection: TextSelection(
        baseOffset: tail,
        extentOffset: 2,
        affinity: TextAffinity.upstream,
        isDirectional: true,
      ),
      composing: TextRange(start: composingStart, end: composingEnd),
    );

    final rebased = rebaseEditorAttachments(
      submitted: [original],
      confirmed: [committed],
      selected: [original],
      description: value,
    );
    const after = '🙂 attachment:asset-7 and attachment:asset-7 end';
    expect(rebased.attachments.single, same(committed));
    expect(rebased.description.text, after);
    expect(rebased.description.selection.baseOffset, after.indexOf(' end'));
    expect(rebased.description.selection.extentOffset, 2);
    expect(rebased.description.selection.affinity, TextAffinity.upstream);
    expect(rebased.description.selection.isDirectional, isTrue);
    final second = after.lastIndexOf('asset-7');
    expect(rebased.description.composing.start, second);
    expect(rebased.description.composing.end, second + 'asset-7'.length);
  });

  test('removed S1 local stays removed and S2-only local stays untouched', () {
    final original = local('C:/old.bin', 'old.bin');
    final committed = stored('asset-old', 'old.bin');
    final newer = local('C:/new.bin', 'new.bin');
    const value = TextEditingValue(
      text: 'attachment:C:/old.bin',
      selection: TextSelection.collapsed(offset: -1),
      composing: TextRange.empty,
    );
    final rebased = rebaseEditorAttachments(
      submitted: [original],
      confirmed: [committed],
      selected: [newer],
      description: value,
    );
    expect(rebased.attachments, [same(newer)]);
    expect(rebased.description, same(value));
    expect(rebased.description.selection.baseOffset, -1);
    expect(rebased.description.composing.start, -1);
  });

  test('rejects foreign canonical metadata instead of rebinding by name', () {
    final original = local('C:/one.bin', 'one.bin');
    final foreign = stored('foreign', 'other.bin');
    expect(
      () => rebaseEditorAttachments(
        submitted: [original],
        confirmed: [foreign],
        selected: [original],
        description: const TextEditingValue(text: 'attachment:C:/one.bin'),
      ),
      throwsFormatException,
    );
    expect(original.pluginId, isNull);
  });

  test('rejects ambiguous raw aliases for selected local assets', () {
    final first = local('C:/a/shared.bin', 'shared.bin');
    final second = local('C:/b/shared.bin', 'shared.bin');
    expect(
      () => rebaseEditorAttachments(
        submitted: [first, second],
        confirmed: [
          stored('asset-a', 'shared.bin'),
          stored('asset-b', 'shared.bin'),
        ],
        selected: [first, second],
        description: const TextEditingValue(text: 'attachment:shared.bin'),
      ),
      throwsFormatException,
    );
  });
}
