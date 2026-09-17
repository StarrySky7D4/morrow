// This is a generated file - do not edit.
//
// Generated from language_pack.proto.

// @dart = 3.3

// ignore_for_file: annotate_overrides, camel_case_types, comment_references
// ignore_for_file: constant_identifier_names
// ignore_for_file: curly_braces_in_flow_control_structures
// ignore_for_file: deprecated_member_use_from_same_package, library_prefixes
// ignore_for_file: non_constant_identifier_names, prefer_relative_imports
// ignore_for_file: unused_import

import 'dart:convert' as $convert;
import 'dart:core' as $core;
import 'dart:typed_data' as $typed_data;

@$core.Deprecated('Use languagePackDescriptor instead')
const LanguagePack$json = {
  '1': 'LanguagePack',
  '2': [
    {'1': 'schema_version', '3': 1, '4': 1, '5': 13, '10': 'schemaVersion'},
    {'1': 'locale', '3': 2, '4': 1, '5': 9, '10': 'locale'},
    {'1': 'catalog_sha256', '3': 3, '4': 1, '5': 12, '10': 'catalogSha256'},
    {
      '1': 'messages',
      '3': 4,
      '4': 3,
      '5': 11,
      '6': '.morrow.localization.v1.Message',
      '10': 'messages'
    },
  ],
};

/// Descriptor for `LanguagePack`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List languagePackDescriptor = $convert.base64Decode(
    'CgxMYW5ndWFnZVBhY2sSJQoOc2NoZW1hX3ZlcnNpb24YASABKA1SDXNjaGVtYVZlcnNpb24SFg'
    'oGbG9jYWxlGAIgASgJUgZsb2NhbGUSJQoOY2F0YWxvZ19zaGEyNTYYAyABKAxSDWNhdGFsb2dT'
    'aGEyNTYSOwoIbWVzc2FnZXMYBCADKAsyHy5tb3Jyb3cubG9jYWxpemF0aW9uLnYxLk1lc3NhZ2'
    'VSCG1lc3NhZ2Vz');

@$core.Deprecated('Use messageDescriptor instead')
const Message$json = {
  '1': 'Message',
  '2': [
    {'1': 'key', '3': 1, '4': 1, '5': 9, '10': 'key'},
    {'1': 'icu', '3': 2, '4': 1, '5': 9, '10': 'icu'},
    {'1': 'metadata_json', '3': 3, '4': 1, '5': 12, '10': 'metadataJson'},
  ],
};

/// Descriptor for `Message`. Decode as a `google.protobuf.DescriptorProto`.
final $typed_data.Uint8List messageDescriptor = $convert.base64Decode(
    'CgdNZXNzYWdlEhAKA2tleRgBIAEoCVIDa2V5EhAKA2ljdRgCIAEoCVIDaWN1EiMKDW1ldGFkYX'
    'RhX2pzb24YAyABKAxSDG1ldGFkYXRhSnNvbg==');
