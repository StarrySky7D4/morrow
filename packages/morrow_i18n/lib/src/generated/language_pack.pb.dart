// This is a generated file - do not edit.
//
// Generated from language_pack.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports

import 'dart:core' as $core;

import 'package:protobuf/protobuf.dart' as $pb;

export 'package:protobuf/protobuf.dart' show GeneratedMessageGenericExtensions;

/// Compile-time ARB source remains authoritative. No executable or external paths.
class LanguagePack extends $pb.GeneratedMessage {
  factory LanguagePack({
    $core.int? schemaVersion,
    $core.String? locale,
    $core.List<$core.int>? catalogSha256,
    $core.Iterable<Message>? messages,
  }) {
    final result = create();
    if (schemaVersion != null) result.schemaVersion = schemaVersion;
    if (locale != null) result.locale = locale;
    if (catalogSha256 != null) result.catalogSha256 = catalogSha256;
    if (messages != null) result.messages.addAll(messages);
    return result;
  }

  LanguagePack._();

  factory LanguagePack.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      create()..mergeFromBuffer(data, registry);
  factory LanguagePack.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      create()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'LanguagePack',
      package: const $pb.PackageName(
          _omitMessageNames ? '' : 'morrow.localization.v1'),
      createEmptyInstance: create)
    ..aI(1, _omitFieldNames ? '' : 'schemaVersion',
        fieldType: $pb.PbFieldType.OU3)
    ..aOS(2, _omitFieldNames ? '' : 'locale')
    ..a<$core.List<$core.int>>(
        3, _omitFieldNames ? '' : 'catalogSha256', $pb.PbFieldType.OY)
    ..pPM<Message>(4, _omitFieldNames ? '' : 'messages',
        subBuilder: Message.create)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  LanguagePack clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  LanguagePack copyWith(void Function(LanguagePack) updates) =>
      super.copyWith((message) => updates(message as LanguagePack))
          as LanguagePack;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static LanguagePack create() => LanguagePack._();
  @$core.override
  LanguagePack createEmptyInstance() => create();
  @$core.pragma('dart2js:noInline')
  static LanguagePack getDefault() => _defaultInstance ??=
      $pb.GeneratedMessage.$_defaultFor<LanguagePack>(create);
  static LanguagePack? _defaultInstance;

  @$pb.TagNumber(1)
  $core.int get schemaVersion => $_getIZ(0);
  @$pb.TagNumber(1)
  set schemaVersion($core.int value) => $_setUnsignedInt32(0, value);
  @$pb.TagNumber(1)
  $core.bool hasSchemaVersion() => $_has(0);
  @$pb.TagNumber(1)
  void clearSchemaVersion() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get locale => $_getSZ(1);
  @$pb.TagNumber(2)
  set locale($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasLocale() => $_has(1);
  @$pb.TagNumber(2)
  void clearLocale() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.List<$core.int> get catalogSha256 => $_getN(2);
  @$pb.TagNumber(3)
  set catalogSha256($core.List<$core.int> value) => $_setBytes(2, value);
  @$pb.TagNumber(3)
  $core.bool hasCatalogSha256() => $_has(2);
  @$pb.TagNumber(3)
  void clearCatalogSha256() => $_clearField(3);

  @$pb.TagNumber(4)
  $pb.PbList<Message> get messages => $_getList(3);
}

class Message extends $pb.GeneratedMessage {
  factory Message({
    $core.String? key,
    $core.String? icu,
    $core.List<$core.int>? metadataJson,
  }) {
    final result = create();
    if (key != null) result.key = key;
    if (icu != null) result.icu = icu;
    if (metadataJson != null) result.metadataJson = metadataJson;
    return result;
  }

  Message._();

  factory Message.fromBuffer($core.List<$core.int> data,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      create()..mergeFromBuffer(data, registry);
  factory Message.fromJson($core.String json,
          [$pb.ExtensionRegistry registry = $pb.ExtensionRegistry.EMPTY]) =>
      create()..mergeFromJson(json, registry);

  static final $pb.BuilderInfo _i = $pb.BuilderInfo(
      _omitMessageNames ? '' : 'Message',
      package: const $pb.PackageName(
          _omitMessageNames ? '' : 'morrow.localization.v1'),
      createEmptyInstance: create)
    ..aOS(1, _omitFieldNames ? '' : 'key')
    ..aOS(2, _omitFieldNames ? '' : 'icu')
    ..a<$core.List<$core.int>>(
        3, _omitFieldNames ? '' : 'metadataJson', $pb.PbFieldType.OY)
    ..hasRequiredFields = false;

  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Message clone() => deepCopy();
  @$core.Deprecated('See https://github.com/google/protobuf.dart/issues/998.')
  Message copyWith(void Function(Message) updates) =>
      super.copyWith((message) => updates(message as Message)) as Message;

  @$core.override
  $pb.BuilderInfo get info_ => _i;

  @$core.pragma('dart2js:noInline')
  static Message create() => Message._();
  @$core.override
  Message createEmptyInstance() => create();
  @$core.pragma('dart2js:noInline')
  static Message getDefault() =>
      _defaultInstance ??= $pb.GeneratedMessage.$_defaultFor<Message>(create);
  static Message? _defaultInstance;

  @$pb.TagNumber(1)
  $core.String get key => $_getSZ(0);
  @$pb.TagNumber(1)
  set key($core.String value) => $_setString(0, value);
  @$pb.TagNumber(1)
  $core.bool hasKey() => $_has(0);
  @$pb.TagNumber(1)
  void clearKey() => $_clearField(1);

  @$pb.TagNumber(2)
  $core.String get icu => $_getSZ(1);
  @$pb.TagNumber(2)
  set icu($core.String value) => $_setString(1, value);
  @$pb.TagNumber(2)
  $core.bool hasIcu() => $_has(1);
  @$pb.TagNumber(2)
  void clearIcu() => $_clearField(2);

  @$pb.TagNumber(3)
  $core.List<$core.int> get metadataJson => $_getN(2);
  @$pb.TagNumber(3)
  set metadataJson($core.List<$core.int> value) => $_setBytes(2, value);
  @$pb.TagNumber(3)
  $core.bool hasMetadataJson() => $_has(2);
  @$pb.TagNumber(3)
  void clearMetadataJson() => $_clearField(3);
}

const $core.bool _omitFieldNames =
    $core.bool.fromEnvironment('protobuf.omit_field_names');
const $core.bool _omitMessageNames =
    $core.bool.fromEnvironment('protobuf.omit_message_names');
