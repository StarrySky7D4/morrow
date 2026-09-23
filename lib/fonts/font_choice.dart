import 'dart:convert';

/// A font preference contains a family name OR a content-addressed local asset.
/// Font bytes never enter ordinary preference messages.
class FontChoice {
  const FontChoice({this.family = '', this.asset = '', this.name = ''});
  final String family, asset, name;
  bool get imported => asset.isNotEmpty;
  String? get resolvedFamily => imported
      ? 'MorrowFont_$asset'
      : family.isEmpty
      ? null
      : family;
  void validate() {
    bool text(String s, int max) =>
        utf8.encode(s).length <= max &&
        !s.runes.any((v) => v < 32 || (v >= 127 && v <= 159));
    if (!text(family, 128) ||
        family != family.trim() ||
        !text(name, 255) ||
        (asset.isNotEmpty &&
            (!RegExp(r'^[0-9a-f]{64}$').hasMatch(asset) ||
                family.isNotEmpty ||
                name.isEmpty)) ||
        (asset.isEmpty && name.isNotEmpty)) {
      throw const FormatException('Invalid font preference');
    }
  }

  factory FontChoice.fromJson(dynamic value) {
    if (value == null) return const FontChoice();
    if (value is! Map) throw const FormatException('Invalid font preference');
    final choice = FontChoice(
      family: value['family'] as String? ?? '',
      asset: value['asset'] as String? ?? '',
      name: value['name'] as String? ?? '',
    );
    choice.validate();
    return choice;
  }
  Map<String, String> toJson() => {
    'family': family,
    'asset': asset,
    'name': name,
  };
  @override
  bool operator ==(Object other) =>
      other is FontChoice &&
      family == other.family &&
      asset == other.asset &&
      name == other.name;
  @override
  int get hashCode => Object.hash(family, asset, name);
}
