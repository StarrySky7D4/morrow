import 'dart:io';
import 'dart:typed_data';
import 'package:flutter_test/flutter_test.dart';
import 'package:morrow_studio/plugins/agent_wrapper_codec_native.dart';
import 'package:morrow_studio/plugins/agent_wrapper_models.dart';

Uint8List unhex(String value) => Uint8List.fromList(
  List.generate(
    value.length ~/ 2,
    (i) => int.parse(value.substring(i * 2, i * 2 + 2), radix: 16),
  ),
);
void main() {
  final path = Platform.environment['MORROW_AGENT_CATALOG_VECTORS'];
  test(
    'five actual Rust vector pairs decode and reencode byte for byte',
    () {
      final fields = <String, String>{};
      for (final line in File(path!).readAsLinesSync()) {
        final index = line.indexOf('=');
        if (index > 0) {
          fields[line.substring(0, index)] = line.substring(index + 1).trim();
        }
      }
      for (final name in [
        'state',
        'inspect_unicode',
        'install_max_u64',
        'approve',
        'page',
      ]) {
        final requestBytes = unhex(fields['$name.request.hex']!);
        final replyBytes = unhex(fields['$name.reply.hex']!);
        final request = AgentWrapperCodec.decodeRequest(requestBytes);
        expect(AgentWrapperCodec.encodeRequest(request), requestBytes);
        expect(
          AgentWrapperValidation.hex(AgentWrapperCodec.hash(requestBytes)),
          fields['$name.request.sha256'],
        );
        final reply = AgentWrapperCodec.decodeReply(request, replyBytes);
        expect(AgentWrapperCodec.encodeReply(reply), replyBytes);
        expect(
          AgentWrapperValidation.hex(AgentWrapperCodec.hash(replyBytes)),
          fields['$name.reply.sha256'],
        );
        expect(reply.result.revisions.catalog, AgentWrapperValidation.maxU64);
        expect(
          reply.result.revisions.manager,
          (BigInt.one << 53) + BigInt.from(19),
        );
        if (name == 'page') {
          expect(reply.result.page!.entries.single.enabled, true);
          expect(reply.result.page!.entries.single.baseEnabled, false);
        }
      }
    },
    skip: path == null
        ? 'Root must supply the actual sealed Rust vectors output in MORROW_AGENT_CATALOG_VECTORS'
        : false,
  );
}
