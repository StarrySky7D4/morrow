import 'dart:js_interop';
import 'package:morrow_core_client/ui.dart';

@JS('uiFixture')
external JSPromise<JSUint8Array> fixture(JSString name);
@JS('uiEvent')
external set eventBytes(JSUint8Array bytes);
@JS('coreProbeResult')
external set result(JSString text);
Future<void> main() async {
  try {
    final doc = UiDocument.decode(
      (await fixture('document'.toJS).toDart).toDart,
    );
    if (doc.nodes.length != 5 || doc.nodes[2].text != '灵感🌈')
      throw StateError('Document mismatch');
    final expected = UiEvent.decode(
      (await fixture('expected-event'.toJS).toDart).toDart,
    );
    final event = UiEvent(
      view: 'view',
      generation: (BigInt.one << 64) - BigInt.one,
      revision: BigInt.one,
      serial: BigInt.one,
      node: 'title',
      action: 'title.edit',
      kind: EventKind.editText,
      text: '从 Dart 编辑🌈',
    );
    if (expected.generation != event.generation || expected.text != event.text)
      throw StateError('Event mismatch');
    eventBytes = event.encode().toJS;
    result =
        'PASS: browser Dart/Wasm decoded Rust form and encoded UInt64 UI event'
            .toJS;
  } catch (error) {
    result = 'FAIL: $error'.toJS;
  }
}
