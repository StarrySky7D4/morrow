import 'dart:convert';
import 'dart:typed_data';

import 'package:capnproto_dart/capnproto_dart.dart';

import 'editor_draft.dart';
import 'generated/editor_draft_api.capnp.dart' as wire;
import 'generated/identity.dart' as contract;
import 'versioned_content_codec.dart';

/// The protobuf journal body is limited to 4 MiB by the host. Cap'n Proto
/// framing can be larger, so the private transfer lane admits up to 8 MiB.
abstract final class EditorDraftCodec {
  static const int maxFrameBytes = 8 * 1024 * 1024;
  static const int _maxTextBytes = 512 * 1024;
  static const int _maxAssets = 20;
  static const int _maxAliases = 8;
  static final BigInt _maxU64 = VersionedContentCodec.maxU64;

  static BigInt unsigned(int carrier) =>
      VersionedContentCodec.unsigned(carrier);
  static int wireU64(BigInt value) => VersionedContentCodec.wireU64(value);

  static bool _same(List<int> a, List<int> b) {
    if (a.length != b.length) return false;
    for (var i = 0; i < a.length; i++) {
      if (a[i] != b[i]) return false;
    }
    return true;
  }

  static void _digest(List<int> bytes, {required bool emptyAllowed}) {
    if (bytes.length != 32 && !(emptyAllowed && bytes.isEmpty)) {
      throw const FormatException('Invalid editor draft digest');
    }
    for (final byte in bytes) {
      if (byte < 0 || byte > 255) {
        throw const FormatException('Invalid editor draft digest byte');
      }
    }
  }

  // Dart strings are UTF-16. Its UTF-8 encoder can replace an unmatched
  // surrogate, which would silently change the user's raw editor value.
  static void _string(String value, int maxBytes) {
    final units = value.codeUnits;
    for (var i = 0; i < units.length; i++) {
      final unit = units[i];
      if (unit >= 0xd800 && unit <= 0xdbff) {
        if (++i >= units.length || units[i] < 0xdc00 || units[i] > 0xdfff) {
          throw const FormatException('Unpaired editor draft surrogate');
        }
      } else if (unit >= 0xdc00 && unit <= 0xdfff) {
        throw const FormatException('Unpaired editor draft surrogate');
      }
    }
    if (utf8.encode(value).length > maxBytes) {
      throw const FormatException('Editor draft text limit');
    }
  }

  static void _identity(String value) {
    _string(value, 256);
    if (value.isEmpty ||
        value.runes.any(
          (rune) =>
              rune < 0x20 ||
              (rune >= 0x7f && rune <= 0x9f) ||
              rune == 0x2f ||
              rune == 0x5c ||
              rune == 0x3a,
        )) {
      throw const FormatException('Invalid editor draft identity');
    }
  }

  static void _u64(
    BigInt value, {
    bool positive = false,
    bool canBeMax = true,
  }) {
    if (value < (positive ? BigInt.one : BigInt.zero) ||
        value > _maxU64 ||
        (!canBeMax && value == _maxU64)) {
      throw const FormatException(
        'Editor draft unsigned value is out of range',
      );
    }
  }

  static void _text(EditorDraftTextValue value) {
    _string(value.text, _maxTextBytes);
    if (value.affinity < 0 || value.affinity > 1) {
      throw const FormatException('Invalid editor draft affinity');
    }
    final units = value.text.length;
    bool offset(int n) => n == -1 || (n >= 0 && n <= units);
    if (!offset(value.selectionBase) ||
        !offset(value.selectionExtent) ||
        !offset(value.composingStart) ||
        !offset(value.composingEnd) ||
        (value.composingStart >= 0 &&
            value.composingEnd >= 0 &&
            value.composingStart > value.composingEnd)) {
      throw const FormatException('Invalid editor draft UTF-16 offset');
    }
  }

  static void _values(EditorDraftValues values) {
    for (final value in [
      values.title,
      values.description,
      values.hypothesis,
      values.conclusion,
      values.todos,
    ]) {
      _text(value);
    }
    _string(values.category, 16 * 1024);
    _string(values.stage, 16 * 1024);
  }

  static void _selection(
    EditorDraftAssetSelection asset, {
    required bool hasPredecessor,
    required bool hasPrevious,
    required bool isNewCard,
  }) {
    _identity(asset.assetId);
    if ((isNewCard &&
            (asset.origin == EditorDraftAssetOrigin.source ||
                asset.origin == EditorDraftAssetOrigin.predecessor)) ||
        (asset.origin == EditorDraftAssetOrigin.predecessor &&
            !hasPredecessor) ||
        (asset.origin == EditorDraftAssetOrigin.previousDraft &&
            !hasPrevious) ||
        (asset.origin == EditorDraftAssetOrigin.parentDraft &&
            (isNewCard || hasPrevious || hasPredecessor)) ||
        asset.aliases.length > _maxAliases) {
      throw const FormatException('Invalid editor draft asset origin');
    }
    for (final alias in asset.aliases) {
      _string(alias, 16 * 1024);
      if (alias.isEmpty) {
        throw const FormatException('Empty editor draft alias');
      }
    }
  }

  static void _request(EditorDraftWriteRequest request) {
    _identity(request.cardId);
    _identity(request.draftId);
    _identity(request.operation);
    _u64(request.expectedGeneration, canBeMax: false);
    final isNewCard = request.sourceKind == EditorDraftSourceKind.newCard;
    _u64(request.sourceRevision, positive: !isNewCard);
    if (isNewCard &&
        (request.sourceRevision != BigInt.zero ||
            request.predecessorOperation.isNotEmpty ||
            request.predecessorDigest.isNotEmpty)) {
      throw const FormatException('Invalid new-card draft source');
    }
    final hasPredecessor = request.predecessorOperation.isNotEmpty;
    if (hasPredecessor) _identity(request.predecessorOperation);
    _digest(request.predecessorDigest, emptyAllowed: !hasPredecessor);
    if (hasPredecessor != request.predecessorDigest.isNotEmpty) {
      throw const FormatException('Editor draft predecessor pair mismatch');
    }
    _values(request.values);
    if (request.assets.length > _maxAssets) {
      throw const FormatException('Editor draft asset limit');
    }
    final ids = <String>{};
    for (final asset in request.assets) {
      _selection(
        asset,
        hasPredecessor: hasPredecessor,
        hasPrevious: request.expectedGeneration > BigInt.zero,
        isNewCard: isNewCard,
      );
      if (!ids.add(asset.assetId)) {
        throw const FormatException('Duplicate editor draft asset');
      }
    }
  }

  static void _writeText(
    wire.TextValueBuilder out,
    EditorDraftTextValue value,
  ) {
    out.text = value.text;
    out.selectionBase = value.selectionBase;
    out.selectionExtent = value.selectionExtent;
    out.affinity = value.affinity;
    out.directional = value.directional;
    out.composingStart = value.composingStart;
    out.composingEnd = value.composingEnd;
  }

  static void _writeValues(wire.ValuesBuilder out, EditorDraftValues values) {
    _writeText(out.initTitle(), values.title);
    _writeText(out.initDescription(), values.description);
    _writeText(out.initHypothesis(), values.hypothesis);
    _writeText(out.initConclusion(), values.conclusion);
    _writeText(out.initTodos(), values.todos);
    out.category = values.category;
    out.stage = values.stage;
  }

  static void _writeSelection(
    wire.AssetSelectionBuilder out,
    EditorDraftAssetSelection value,
  ) {
    out.origin = value.origin.index;
    out.assetId = value.assetId;
    final aliases = out.initAliases(value.aliases.length);
    for (var i = 0; i < value.aliases.length; i++) {
      aliases[i] = value.aliases[i];
    }
  }

  static void _writeRequest(
    wire.WriteRequestBuilder out,
    EditorDraftWriteRequest request,
  ) {
    out.version = 1;
    out.digest = Uint8List.fromList(contract.editor_draft_apiDigest);
    out.cardId = request.cardId;
    out.draftId = request.draftId;
    out.operation = request.operation;
    out.expectedGenerationBigInt = request.expectedGeneration;
    out.sourceRevisionBigInt = request.sourceRevision;
    out.sourceKind = request.sourceKind.index;
    out.predecessorOperation = request.predecessorOperation;
    out.predecessorDigest = Uint8List.fromList(request.predecessorDigest);
    _writeValues(out.initValues(), request.values);
    final assets = out.initAssets(request.assets.length);
    for (var i = 0; i < request.assets.length; i++) {
      _writeSelection(assets[i], request.assets[i]);
    }
  }

  static void _linkShape(EditorDraftParentLink link) {
    for (final value in [
      link.parentDraftId,
      link.parentSaveOperation,
      link.committedOperation,
      link.childOperation,
    ]) {
      _identity(value);
    }
    _u64(link.parentGeneration, positive: true, canBeMax: false);
    _digest(link.parentRequestSha256, emptyAllowed: false);
    _digest(link.committedSha256, emptyAllowed: false);
  }

  static void _parentLink(
    EditorDraftWriteRequest request,
    EditorDraftParentLink link,
  ) {
    _request(request);
    _linkShape(link);
    if (request.sourceKind != EditorDraftSourceKind.existingCard ||
        request.predecessorOperation.isNotEmpty ||
        request.predecessorDigest.isNotEmpty ||
        link.parentDraftId == request.draftId ||
        (request.expectedGeneration == BigInt.zero &&
            link.childOperation != request.operation)) {
      throw const FormatException('Invalid editor draft parent binding');
    }
  }

  static void _writeParentLink(
    wire.ParentLinkBuilder out,
    EditorDraftParentLink link,
  ) {
    out.parentDraftId = link.parentDraftId;
    out.parentGenerationBigInt = link.parentGeneration;
    out.parentSaveOperation = link.parentSaveOperation;
    out.parentRequestSha256 = Uint8List.fromList(link.parentRequestSha256);
    out.committedOperation = link.committedOperation;
    out.committedSha256 = Uint8List.fromList(link.committedSha256);
    out.childOperation = link.childOperation;
  }

  static Uint8List encodeHandoff(EditorDraftHandoffRequest handoff) {
    if (handoff.request.expectedGeneration != BigInt.zero) {
      throw const FormatException('Handoff must create the first child draft');
    }
    _parentLink(handoff.request, handoff.parentLink);
    final message = MessageBuilder();
    final out = message.initRoot(wire.handoffRequestFactory);
    out.version = 1;
    out.digest = Uint8List.fromList(contract.editor_draft_apiDigest);
    _writeRequest(out.initRequest(), handoff.request);
    _writeParentLink(out.initParentLink(), handoff.parentLink);
    final bytes = message.serialize();
    if (bytes.length > maxFrameBytes) {
      throw const FormatException('Editor draft handoff frame limit');
    }
    return bytes;
  }

  static void _proposal(EditorDraftHandoffProposal proposal) {
    final handoff = proposal.handoff;
    if (handoff.request.expectedGeneration != BigInt.zero) {
      throw const FormatException('Proposal must create the first child draft');
    }
    _parentLink(handoff.request, handoff.parentLink);
    _identity(proposal.retirementOperation);
    if (proposal.retirementOperation == handoff.request.operation ||
        proposal.retirementOperation ==
            handoff.parentLink.parentSaveOperation ||
        proposal.retirementOperation == handoff.parentLink.committedOperation) {
      throw const FormatException('Reused editor draft proposal operation');
    }
  }

  static Uint8List encodeHandoffProposal(EditorDraftHandoffProposal proposal) {
    // Validate the entire immutable input before allocating the wire frame.
    _proposal(proposal);
    final message = MessageBuilder();
    final out = message.initRoot(wire.handoffProposalFactory);
    out.version = 1;
    out.digest = Uint8List.fromList(contract.editor_draft_apiDigest);
    _writeRequest(out.initRequest(), proposal.handoff.request);
    _writeParentLink(out.initParentLink(), proposal.handoff.parentLink);
    out.retirementOperation = proposal.retirementOperation;
    final bytes = message.serialize();
    if (bytes.length > maxFrameBytes) {
      throw const FormatException('Editor draft proposal frame limit');
    }
    return bytes;
  }

  static EditorDraftHandoffProposal _decodeProposal(
    wire.HandoffProposalReader? value,
  ) {
    if (value == null || value.retirementOperation == null) {
      throw const FormatException('Missing editor draft handoff proposal');
    }
    _header(value.version, value.digest);
    final request = _decodeRequest(value.request);
    final result = EditorDraftHandoffProposal(
      handoff: EditorDraftHandoffRequest(
        request: request,
        parentLink: _decodeParentLink(value.parentLink, request),
      ),
      retirementOperation: value.retirementOperation!,
    );
    _proposal(result);
    return result;
  }

  static EditorDraftHandoffProposal decodeHandoffProposal(Uint8List bytes) {
    try {
      return _decodeProposal(_read(bytes).getRoot(wire.handoffProposalFactory));
    } on FormatException {
      rethrow;
    } catch (_) {
      throw const FormatException('Malformed editor draft handoff proposal');
    }
  }

  static Uint8List encodeWrite(EditorDraftWriteRequest request) {
    _request(request);
    final message = MessageBuilder();
    _writeRequest(message.initRoot(wire.writeRequestFactory), request);
    final bytes = message.serialize();
    if (bytes.length > maxFrameBytes) {
      throw const FormatException('Editor draft transfer frame limit');
    }
    return bytes;
  }

  static MessageReader _read(Uint8List bytes) {
    if (bytes.length < 8 || bytes.length > maxFrameBytes) {
      throw const FormatException('Editor draft frame length');
    }
    final view = ByteData.sublistView(bytes);
    final count = view.getUint32(0, Endian.little) + 1;
    if (count > 512) throw const FormatException('Editor draft segment count');
    var total = ((count + 2) ~/ 2) * 8;
    if (total > bytes.length) {
      throw const FormatException('Editor draft frame header');
    }
    for (var i = 0; i < count; i++) {
      total += view.getUint32(4 + i * 4, Endian.little) * 8;
      if (total > bytes.length) {
        throw const FormatException('Editor draft segment length');
      }
    }
    if (total != bytes.length) {
      throw const FormatException('Trailing editor draft frame data');
    }
    return MessageReader.deserialize(
      bytes,
      MessageReaderOptions(
        traversalLimitInWords: maxFrameBytes ~/ 8 * 4,
        nestingLimit: 24,
        maxSegments: 512,
      ),
    );
  }

  static void _header(int version, Uint8List? digest) {
    if (version != 1 ||
        digest == null ||
        !_same(digest, contract.editor_draft_apiDigest)) {
      throw const FormatException('Editor draft protocol mismatch');
    }
  }

  static EditorDraftTextValue _decodeText(wire.TextValueReader? value) {
    if (value == null || value.text == null) {
      throw const FormatException('Missing editor draft text value');
    }
    final result = EditorDraftTextValue(
      text: value.text!,
      selectionBase: value.selectionBase,
      selectionExtent: value.selectionExtent,
      affinity: value.affinity,
      directional: value.directional,
      composingStart: value.composingStart,
      composingEnd: value.composingEnd,
    );
    _text(result);
    return result;
  }

  static EditorDraftValues _decodeValues(wire.ValuesReader? value) {
    if (value == null || value.category == null || value.stage == null) {
      throw const FormatException('Missing editor draft values');
    }
    final result = EditorDraftValues(
      title: _decodeText(value.title),
      description: _decodeText(value.description),
      hypothesis: _decodeText(value.hypothesis),
      conclusion: _decodeText(value.conclusion),
      todos: _decodeText(value.todos),
      category: value.category!,
      stage: value.stage!,
    );
    _values(result);
    return result;
  }

  static List<String> _aliases(ListReader<String?>? values) {
    if (values != null && values.length > _maxAliases) {
      throw const FormatException('Editor draft alias limit');
    }
    final result = <String>[];
    for (final alias in values ?? <String?>[]) {
      if (alias == null) {
        throw const FormatException('Missing editor draft alias');
      }
      result.add(alias);
    }
    return result;
  }

  static EditorDraftAssetSelection _decodeSelection(
    wire.AssetSelectionReader? value, {
    required bool hasPredecessor,
    required bool hasPrevious,
    required bool isNewCard,
  }) {
    if (value == null ||
        value.assetId == null ||
        value.origin >= EditorDraftAssetOrigin.values.length) {
      throw const FormatException('Missing editor draft asset selection');
    }
    final result = EditorDraftAssetSelection(
      origin: EditorDraftAssetOrigin.values[value.origin],
      assetId: value.assetId!,
      aliases: _aliases(value.aliases),
    );
    _selection(
      result,
      hasPredecessor: hasPredecessor,
      hasPrevious: hasPrevious,
      isNewCard: isNewCard,
    );
    return result;
  }

  static EditorDraftWriteRequest _decodeRequest(
    wire.WriteRequestReader? value,
  ) {
    if (value == null) {
      throw const FormatException('Missing editor draft request');
    }
    _header(value.version, value.digest);
    if (value.cardId == null ||
        value.draftId == null ||
        value.operation == null ||
        value.predecessorOperation == null ||
        value.predecessorDigest == null) {
      throw const FormatException('Missing editor draft request identity');
    }
    if (value.assets != null && value.assets!.length > _maxAssets) {
      throw const FormatException('Editor draft asset limit');
    }
    if (value.sourceKind >= EditorDraftSourceKind.values.length) {
      throw const FormatException('Unknown editor draft source kind');
    }
    final sourceKind = EditorDraftSourceKind.values[value.sourceKind];
    final hasPredecessor = value.predecessorOperation!.isNotEmpty;
    final expected = value.expectedGenerationBigInt;
    final result = EditorDraftWriteRequest(
      cardId: value.cardId!,
      draftId: value.draftId!,
      operation: value.operation!,
      expectedGeneration: expected,
      sourceRevision: value.sourceRevisionBigInt,
      sourceKind: sourceKind,
      predecessorOperation: value.predecessorOperation!,
      predecessorDigest: value.predecessorDigest!,
      values: _decodeValues(value.values),
      assets: [
        for (final item in value.assets ?? <wire.AssetSelectionReader>[])
          _decodeSelection(
            item,
            hasPredecessor: hasPredecessor,
            hasPrevious: expected > BigInt.zero,
            isNewCard: sourceKind == EditorDraftSourceKind.newCard,
          ),
      ],
    );
    _request(result);
    return result;
  }

  static EditorDraftParentLink _decodeParentLink(
    wire.ParentLinkReader? value,
    EditorDraftWriteRequest? request,
  ) {
    if (value == null ||
        value.parentDraftId == null ||
        value.parentSaveOperation == null ||
        value.parentRequestSha256 == null ||
        value.committedOperation == null ||
        value.committedSha256 == null ||
        value.childOperation == null) {
      throw const FormatException('Missing editor draft parent link');
    }
    final result = EditorDraftParentLink(
      parentDraftId: value.parentDraftId!,
      parentGeneration: value.parentGenerationBigInt,
      parentSaveOperation: value.parentSaveOperation!,
      parentRequestSha256: value.parentRequestSha256!,
      committedOperation: value.committedOperation!,
      committedSha256: value.committedSha256!,
      childOperation: value.childOperation!,
    );
    if (request == null) {
      _linkShape(result);
    } else {
      _parentLink(request, result);
    }
    return result;
  }

  static EditorDraftHandoffRequest decodeHandoff(Uint8List bytes) {
    try {
      final value = _read(bytes).getRoot(wire.handoffRequestFactory);
      _header(value.version, value.digest);
      final request = _decodeRequest(value.request);
      if (request.expectedGeneration != BigInt.zero) {
        throw const FormatException(
          'Handoff must create the first child draft',
        );
      }
      return EditorDraftHandoffRequest(
        request: request,
        parentLink: _decodeParentLink(value.parentLink, request),
      );
    } on FormatException {
      rethrow;
    } catch (_) {
      throw const FormatException('Malformed editor draft handoff');
    }
  }

  static EditorDraftParentRetirement _decodeRetirement(
    wire.ParentRetirementReader? value,
    EditorDraftWriteRequest request,
    BigInt generation,
    bool active,
  ) {
    if (value == null ||
        value.childDraftId == null ||
        value.childOperation == null ||
        value.operation == null) {
      throw const FormatException('Missing editor draft parent retirement');
    }
    for (final id in [
      value.childDraftId!,
      value.childOperation!,
      value.operation!,
    ]) {
      _identity(id);
    }
    final parentGeneration = value.parentGenerationBigInt;
    _u64(parentGeneration, positive: true, canBeMax: false);
    if (active ||
        value.childDraftId == request.draftId ||
        parentGeneration + BigInt.one != generation ||
        parentGeneration != request.expectedGeneration + BigInt.one) {
      throw const FormatException('Invalid editor draft parent retirement');
    }
    return EditorDraftParentRetirement(
      childDraftId: value.childDraftId!,
      childOperation: value.childOperation!,
      operation: value.operation!,
      parentGeneration: parentGeneration,
    );
  }

  static bool _sameSelection(
    EditorDraftAssetSelection a,
    EditorDraftAssetSelection b,
  ) {
    if (a.origin != b.origin ||
        a.assetId != b.assetId ||
        a.aliases.length != b.aliases.length) {
      return false;
    }
    for (var i = 0; i < a.aliases.length; i++) {
      if (a.aliases[i] != b.aliases[i]) return false;
    }
    return true;
  }

  static EditorDraftStoredAsset _decodeStoredAsset(
    wire.StoredAssetReader value,
    EditorDraftWriteRequest request,
  ) {
    if (value.name == null || value.mediaType == null || value.sha256 == null) {
      throw const FormatException('Missing stored editor draft asset');
    }
    final selection = _decodeSelection(
      value.selection,
      hasPredecessor: request.predecessorOperation.isNotEmpty,
      hasPrevious: request.expectedGeneration > BigInt.zero,
      isNewCard: request.sourceKind == EditorDraftSourceKind.newCard,
    );
    _string(value.name!, 16 * 1024);
    _string(value.mediaType!, 1024);
    _digest(value.sha256!, emptyAllowed: false);
    final bytes = value.bytesBigInt;
    if (bytes > BigInt.from(64 * 1024 * 1024)) {
      throw const FormatException('Stored editor draft asset budget');
    }
    return EditorDraftStoredAsset(
      selection: selection,
      name: value.name!,
      mediaType: value.mediaType!,
      bytes: bytes,
      sha256: value.sha256!,
    );
  }

  static EditorDraftRecord _decodeRecord(wire.RecordReader? value) {
    if (value == null) {
      throw const FormatException('Missing editor draft record');
    }
    final request = _decodeRequest(value.request);
    final generation = value.generationBigInt;
    final current = value.currentGenerationBigInt;
    final source = value.sourceRevisionBigInt;
    final predecessor = value.predecessorRevisionBigInt;
    if (generation == BigInt.zero ||
        current < generation ||
        (request.sourceKind == EditorDraftSourceKind.newCard
            ? value.sourceFormat != 0
            : value.sourceFormat < 1 || value.sourceFormat > 2) ||
        source != request.sourceRevision ||
        value.sourceSha256 == null ||
        value.predecessorSha256 == null ||
        value.requestSha256 == null) {
      throw const FormatException('Invalid editor draft record identity');
    }
    _digest(value.requestSha256!, emptyAllowed: false);
    final isNewCard = request.sourceKind == EditorDraftSourceKind.newCard;
    _digest(value.sourceSha256!, emptyAllowed: isNewCard);
    if (isNewCard && value.sourceSha256!.isNotEmpty) {
      throw const FormatException('New-card draft has a source hash');
    }
    final hasPredecessor = request.predecessorOperation.isNotEmpty;
    _digest(value.predecessorSha256!, emptyAllowed: !hasPredecessor);
    if (hasPredecessor != value.predecessorSha256!.isNotEmpty ||
        (hasPredecessor && predecessor != source + BigInt.one) ||
        (!hasPredecessor && predecessor != BigInt.zero) ||
        generation !=
            request.expectedGeneration + BigInt.from(value.active ? 1 : 2) ||
        (!value.repeated &&
            (current != generation || value.currentActive != value.active))) {
      throw const FormatException('Invalid editor draft record revision');
    }
    if (value.assets != null && value.assets!.length > _maxAssets) {
      throw const FormatException('Stored editor draft asset limit');
    }
    final assets = <EditorDraftStoredAsset>[
      for (final item in value.assets ?? <wire.StoredAssetReader>[])
        _decodeStoredAsset(item, request),
    ];
    if (assets.length != request.assets.length) {
      throw const FormatException('Editor draft selected assets changed');
    }
    for (var i = 0; i < assets.length; i++) {
      if (!_sameSelection(assets[i].selection, request.assets[i])) {
        throw const FormatException('Editor draft selected asset mismatch');
      }
    }
    final parentLink = value.parentLink == null
        ? null
        : _decodeParentLink(value.parentLink, request);
    if ((parentLink == null &&
            request.assets.any(
              (asset) => asset.origin == EditorDraftAssetOrigin.parentDraft,
            )) ||
        (request.expectedGeneration > BigInt.zero &&
            request.assets.any(
              (asset) => asset.origin == EditorDraftAssetOrigin.parentDraft,
            ))) {
      throw const FormatException('Unbound editor draft parent asset');
    }
    final retirement = value.retirement == null
        ? null
        : _decodeRetirement(
            value.retirement,
            request,
            generation,
            value.active,
          );

    return EditorDraftRecord(
      request: request,
      generation: generation,
      active: value.active,
      currentGeneration: current,
      currentActive: value.currentActive,
      repeated: value.repeated,
      sourceFormat: value.sourceFormat,
      sourceRevision: source,
      sourceSha256: value.sourceSha256!,
      predecessorRevision: predecessor,
      predecessorSha256: value.predecessorSha256!,
      assets: assets,
      requestSha256: value.requestSha256!,
      parentLink: parentLink,
      retirement: retirement,
    );
  }

  static EditorDraftSummary _decodeSummary(wire.SummaryReader value) {
    if (value.cardId == null || value.draftId == null) {
      throw const FormatException('Missing editor draft summary');
    }
    _identity(value.cardId!);
    _identity(value.draftId!);
    final generation = value.generationBigInt;
    _u64(generation, positive: true);
    return EditorDraftSummary(
      cardId: value.cardId!,
      draftId: value.draftId!,
      generation: generation,
      active: value.active,
    );
  }

  static EditorDraftImportedAsset _decodeImported(
    wire.ImportedAssetReader? value,
  ) {
    if (value == null ||
        value.id == null ||
        value.name == null ||
        value.kind == null) {
      throw const FormatException('Missing imported editor draft asset');
    }
    _identity(value.id!);
    _string(value.name!, 16 * 1024);
    _string(value.kind!, 1024);
    return EditorDraftImportedAsset(
      id: value.id!,
      name: value.name!,
      kind: value.kind!,
      bytes: value.bytesBigInt,
    );
  }

  static bool _lineageCursor(String value, {bool allowEmpty = false}) =>
      (allowEmpty && value.isEmpty) ||
      RegExp(r'^morrow-host-editor-draft-[0-9a-f]{64}$').hasMatch(value);

  static void validateRetirement(EditorDraftRetirementRequest request) {
    for (final id in [
      request.cardId,
      request.childDraftId,
      request.parentDraftId,
      request.childOperation,
      request.operation,
    ]) {
      _identity(id);
    }
    _u64(request.parentGeneration, positive: true, canBeMax: false);
    if (request.childDraftId == request.parentDraftId) {
      throw const FormatException('Parent and child draft IDs must differ');
    }
  }

  static void validateLineagePageRequest({String cursor = '', int limit = 32}) {
    if (!_lineageCursor(cursor, allowEmpty: true) || limit < 1 || limit > 32) {
      throw const FormatException('Invalid editor draft lineage page request');
    }
  }

  static EditorDraftLineage _decodeLineage(wire.LineageReader value) {
    if (value.cardId == null ||
        value.childDraftId == null ||
        value.cursor == null) {
      throw const FormatException('Missing editor draft lineage identity');
    }
    _identity(value.cardId!);
    _identity(value.childDraftId!);
    final childGeneration = value.childGenerationBigInt;
    final parentGeneration = value.parentGenerationBigInt;
    _u64(childGeneration, positive: true);
    _u64(parentGeneration, positive: true);
    final link = _decodeParentLink(value.parentLink, null);
    if (link.parentDraftId == value.childDraftId ||
        parentGeneration < link.parentGeneration ||
        (value.parentActive
            ? parentGeneration != link.parentGeneration
            : parentGeneration != link.parentGeneration + BigInt.one) ||
        !_lineageCursor(value.cursor!)) {
      throw const FormatException('Invalid editor draft lineage');
    }
    return EditorDraftLineage(
      cardId: value.cardId!,
      childDraftId: value.childDraftId!,
      childGeneration: childGeneration,
      childActive: value.childActive,
      parentGeneration: parentGeneration,
      parentActive: value.parentActive,
      parentLink: link,
      cursor: value.cursor!,
    );
  }

  static EditorDraftLineagePage _decodeLineagePage(wire.EnvelopeReader value) {
    if (value.lineages == null ||
        value.nextCursor == null ||
        value.requestCursor == null ||
        value.requestLimit < 1 ||
        value.requestLimit > 32 ||
        value.lineages!.length > value.requestLimit ||
        !_lineageCursor(value.requestCursor!, allowEmpty: true) ||
        !_lineageCursor(value.nextCursor!, allowEmpty: true)) {
      throw const FormatException('Invalid editor draft lineage page');
    }
    final entries = <EditorDraftLineage>[
      for (final row in value.lineages!) _decodeLineage(row),
    ];
    var cursor = value.requestCursor!;
    final seen = <String>{};
    for (final row in entries) {
      if (row.cursor.compareTo(cursor) <= 0 ||
          !seen.add('${row.cardId.length}:${row.cardId}${row.childDraftId}')) {
        throw const FormatException('Unordered or duplicate draft lineage');
      }
      cursor = row.cursor;
    }
    if (value.nextCursor!.isNotEmpty &&
        (entries.isEmpty || value.nextCursor != entries.last.cursor)) {
      throw const FormatException('Invalid next editor draft lineage cursor');
    }
    return EditorDraftLineagePage(
      lineages: entries,
      nextCursor: value.nextCursor!,
      requestCursor: value.requestCursor!,
      requestLimit: value.requestLimit,
    );
  }

  static bool _proposalCursor(String value, {bool allowEmpty = false}) =>
      (allowEmpty && value.isEmpty) ||
      RegExp(
        r'^morrow-host-editor-handoff-proposal-[0-9a-f]{64}$',
      ).hasMatch(value);

  static void validateHandoffProposalIdentity(
    String cardId,
    String parentDraftId,
    String childOperation,
  ) {
    _identity(cardId);
    _identity(parentDraftId);
    _identity(childOperation);
  }

  static void validateHandoffProposalPageRequest({
    required String cursor,
    required int limit,
  }) {
    if (!_proposalCursor(cursor, allowEmpty: true) || limit < 1 || limit > 32) {
      throw const FormatException('Invalid editor draft proposal page request');
    }
  }

  static EditorDraftHandoffProposalSummary _decodeProposalSummary(
    wire.HandoffProposalSummaryReader value,
  ) {
    if (value.cardId == null ||
        value.parentDraftId == null ||
        value.childDraftId == null ||
        value.childOperation == null ||
        value.retirementOperation == null ||
        value.cursor == null) {
      throw const FormatException('Missing editor draft proposal summary');
    }
    validateHandoffProposalIdentity(
      value.cardId!,
      value.parentDraftId!,
      value.childOperation!,
    );
    _identity(value.childDraftId!);
    _identity(value.retirementOperation!);
    final statusIndex = value.status;
    if (statusIndex >= EditorDraftHandoffProposalStatus.values.length) {
      throw const FormatException('Unknown editor draft proposal status');
    }
    final status = EditorDraftHandoffProposalStatus.values[statusIndex];
    final revision = value.revisionBigInt;
    final parentGeneration = value.parentGenerationBigInt;
    final childGeneration = value.childGenerationBigInt;
    _u64(revision, positive: true);
    _u64(parentGeneration);
    _u64(childGeneration);
    if (value.parentDraftId == value.childDraftId ||
        value.childOperation == value.retirementOperation ||
        !_proposalCursor(value.cursor!) ||
        (parentGeneration == BigInt.zero && value.parentActive) ||
        (childGeneration == BigInt.zero && value.childActive) ||
        (status == EditorDraftHandoffProposalStatus.cancelled &&
            revision != BigInt.from(2)) ||
        (status != EditorDraftHandoffProposalStatus.cancelled &&
            status != EditorDraftHandoffProposalStatus.conflict &&
            revision != BigInt.one) ||
        (status == EditorDraftHandoffProposalStatus.pending &&
            (!value.parentActive || childGeneration != BigInt.zero)) ||
        (status == EditorDraftHandoffProposalStatus.childCommitted &&
            (!value.parentActive ||
                !value.childActive ||
                childGeneration == BigInt.zero)) ||
        (status == EditorDraftHandoffProposalStatus.parentRetired &&
            (value.parentActive ||
                parentGeneration == BigInt.zero ||
                childGeneration == BigInt.zero)) ||
        (status == EditorDraftHandoffProposalStatus.cancelled &&
            childGeneration != BigInt.zero)) {
      throw const FormatException('Invalid editor draft proposal summary');
    }
    return EditorDraftHandoffProposalSummary(
      cardId: value.cardId!,
      parentDraftId: value.parentDraftId!,
      childDraftId: value.childDraftId!,
      childOperation: value.childOperation!,
      retirementOperation: value.retirementOperation!,
      revision: revision,
      status: status,
      parentGeneration: parentGeneration,
      parentActive: value.parentActive,
      childGeneration: childGeneration,
      childActive: value.childActive,
      cursor: value.cursor!,
    );
  }

  static EditorDraftHandoffProposalRecord _decodeProposalRecord(
    wire.HandoffProposalRecordReader? value,
  ) {
    if (value == null || value.summary == null) {
      throw const FormatException('Missing editor draft proposal record');
    }
    final proposal = _decodeProposal(value.proposal);
    final summary = _decodeProposalSummary(value.summary!);
    final request = proposal.handoff.request;
    final linkedGeneration = proposal.handoff.parentLink.parentGeneration;
    if (summary.cardId != request.cardId ||
        summary.parentDraftId != proposal.handoff.parentLink.parentDraftId ||
        summary.childDraftId != request.draftId ||
        summary.childOperation != request.operation ||
        summary.retirementOperation != proposal.retirementOperation ||
        ((summary.status == EditorDraftHandoffProposalStatus.pending ||
                summary.status ==
                    EditorDraftHandoffProposalStatus.childCommitted) &&
            summary.parentGeneration != linkedGeneration) ||
        (summary.status == EditorDraftHandoffProposalStatus.parentRetired &&
            summary.parentGeneration != linkedGeneration + BigInt.one)) {
      throw const FormatException('Editor draft proposal record mismatch');
    }
    return EditorDraftHandoffProposalRecord(
      proposal: proposal,
      summary: summary,
    );
  }

  static EditorDraftHandoffProposalPage _decodeProposalPage(
    wire.EnvelopeReader value,
  ) {
    final rows = value.handoffProposalSummaries;
    if (rows == null ||
        value.nextCursor == null ||
        value.requestCursor == null ||
        value.requestLimit < 1 ||
        value.requestLimit > 32 ||
        rows.length > value.requestLimit ||
        !_proposalCursor(value.requestCursor!, allowEmpty: true) ||
        !_proposalCursor(value.nextCursor!, allowEmpty: true)) {
      throw const FormatException('Invalid editor draft proposal page');
    }
    final proposals = <EditorDraftHandoffProposalSummary>[
      for (final row in rows) _decodeProposalSummary(row),
    ];
    var cursor = value.requestCursor!;
    final seen = <String>{};
    for (final row in proposals) {
      final identity =
          '${row.cardId.length}:${row.cardId}${row.parentDraftId.length}:${row.parentDraftId}${row.childOperation}';
      if (row.cursor.compareTo(cursor) <= 0 || !seen.add(identity)) {
        throw const FormatException('Unordered or duplicate draft proposal');
      }
      cursor = row.cursor;
    }
    if (value.nextCursor!.isNotEmpty &&
        (proposals.isEmpty || value.nextCursor != proposals.last.cursor)) {
      throw const FormatException('Invalid next draft proposal cursor');
    }
    return EditorDraftHandoffProposalPage(
      proposals: proposals,
      nextCursor: value.nextCursor!,
      requestCursor: value.requestCursor!,
      requestLimit: value.requestLimit,
    );
  }

  static EditorDraftEnvelope decodeEnvelope(Uint8List bytes) {
    try {
      final value = _read(bytes).getRoot(wire.envelopeFactory);
      _header(value.version, value.digest);
      if (value.kind == null ||
          value.cardId == null ||
          value.draftId == null ||
          value.operation == null) {
        throw const FormatException('Missing editor draft envelope identity');
      }
      final kind = EditorDraftResultKind.values[value.kind!.index];
      final cardId = value.cardId!;
      final draftId = value.draftId!;
      final operation = value.operation!;
      final expected = value.expectedGenerationBigInt;
      EditorDraftRecord? record;
      List<EditorDraftSummary>? summaries;
      EditorDraftImportedAsset? asset;
      EditorDraftLineagePage? lineagePage;
      EditorDraftHandoffProposalRecord? handoffProposal;
      EditorDraftHandoffProposalPage? handoffProposalPage;
      if (kind != EditorDraftResultKind.handoffProposal &&
          value.handoffProposal != null) {
        throw const FormatException('Unexpected editor draft proposal record');
      }
      if (kind != EditorDraftResultKind.handoffProposals &&
          value.handoffProposalSummaries != null) {
        throw const FormatException(
          'Unexpected editor draft proposal summaries',
        );
      }
      if (kind != EditorDraftResultKind.lineages &&
          kind != EditorDraftResultKind.handoffProposals &&
          (value.lineages != null ||
              value.nextCursor != null ||
              value.requestCursor != null ||
              value.requestLimit != 0)) {
        throw const FormatException('Unexpected editor draft lineage data');
      }
      switch (kind) {
        case EditorDraftResultKind.absent:
          _identity(cardId);
          _identity(draftId);
          // A draft read has no operation; a missing proposal inspect retains
          // its original child operation so the caller can bind the reply.
          if (operation.isNotEmpty) {
            _identity(operation);
          }
          if ((operation.isNotEmpty && expected != BigInt.zero) ||
              value.record != null ||
              value.summaries != null ||
              value.asset != null) {
            throw const FormatException('Invalid absent editor draft');
          }
        case EditorDraftResultKind.record:
          _identity(cardId);
          _identity(draftId);
          if (value.summaries != null || value.asset != null) {
            throw const FormatException('Invalid editor draft record envelope');
          }
          record = _decodeRecord(value.record);
          if (record.request.cardId != cardId ||
              record.request.draftId != draftId) {
            throw const FormatException(
              'Editor draft envelope target mismatch',
            );
          }
        case EditorDraftResultKind.list:
          if (cardId.isNotEmpty ||
              draftId.isNotEmpty ||
              operation.isNotEmpty ||
              expected != BigInt.zero ||
              value.record != null ||
              value.summaries == null ||
              value.asset != null ||
              value.summaries!.length > 16) {
            throw const FormatException('Invalid editor draft list envelope');
          }
          summaries = [for (final row in value.summaries!) _decodeSummary(row)];
          final seen = <String>{};
          for (final row in summaries) {
            if (!row.active ||
                !seen.add('${row.cardId.length}:${row.cardId}${row.draftId}')) {
              throw const FormatException(
                'Duplicate or inactive editor draft summary',
              );
            }
          }
        case EditorDraftResultKind.imported:
          _identity(cardId);
          _identity(draftId);
          if (operation.isNotEmpty ||
              value.record != null ||
              value.summaries != null) {
            throw const FormatException('Invalid imported draft envelope');
          }
          asset = _decodeImported(value.asset);
        case EditorDraftResultKind.exported:
          _identity(cardId);
          _identity(draftId);
          if (operation.isNotEmpty ||
              value.record != null ||
              value.summaries != null ||
              value.asset != null) {
            throw const FormatException('Invalid exported draft envelope');
          }
        case EditorDraftResultKind.lineages:
          if (cardId.isNotEmpty ||
              draftId.isNotEmpty ||
              operation.isNotEmpty ||
              expected != BigInt.zero ||
              value.record != null ||
              value.summaries != null ||
              value.asset != null) {
            throw const FormatException(
              'Invalid editor draft lineages envelope',
            );
          }
          lineagePage = _decodeLineagePage(value);
        case EditorDraftResultKind.handoffProposal:
          _identity(cardId);
          _identity(draftId);
          _identity(operation);
          if (expected != BigInt.zero ||
              value.record != null ||
              value.summaries != null ||
              value.asset != null) {
            throw const FormatException(
              'Invalid draft proposal record envelope',
            );
          }
          handoffProposal = _decodeProposalRecord(value.handoffProposal);
          final summary = handoffProposal.summary;
          if (summary.cardId != cardId ||
              summary.parentDraftId != draftId ||
              summary.childOperation != operation) {
            throw const FormatException(
              'Draft proposal envelope target mismatch',
            );
          }
        case EditorDraftResultKind.handoffProposals:
          if (cardId.isNotEmpty ||
              draftId.isNotEmpty ||
              operation.isNotEmpty ||
              expected != BigInt.zero ||
              value.lineages != null ||
              value.record != null ||
              value.summaries != null ||
              value.asset != null) {
            throw const FormatException('Invalid draft proposal page envelope');
          }
          handoffProposalPage = _decodeProposalPage(value);
      }
      return EditorDraftEnvelope(
        kind: kind,
        cardId: cardId,
        draftId: draftId,
        operation: operation,
        expectedGeneration: expected,
        record: record,
        summaries: summaries,
        asset: asset,
        lineagePage: lineagePage,
        handoffProposal: handoffProposal,
        handoffProposalPage: handoffProposalPage,
      );
    } on FormatException {
      rethrow;
    } catch (_) {
      throw const FormatException('Malformed editor draft envelope');
    }
  }
}
