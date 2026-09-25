"""Adapt optional int64 reflection and signed getters for JavaScript.

Upstream 0.1.0 metadata IDs are int, which cannot represent all uint64 values in
JavaScript. Web omits that optional reflection API. The optional exact64 mode
adds BigInt fields on both platforms and rejects unsafe legacy Web conversions.
Exact node IDs remain available as hexadecimal strings and BigInt in a sidecar.
Unknown generator shapes fail closed instead of rewriting arbitrary literals.
Signed 8/16/32-bit getters restore the sign after upstream default-mask XOR;
Dart's JavaScript bitwise operations otherwise expose an unsigned result.
"""
import re

DECLARATION = re.compile(r"^const (?:Enum|Struct)SchemaInfo (\w+) = (?:Enum|Struct)SchemaInfo\(\n(.*?)^\);", re.M | re.S)
READER = re.compile(r"^  static const StructSchemaInfo schema = \w+;\n", re.M)
FACTORY = re.compile(r"^  @override\n  StructSchemaInfo get schema => \w+;\n", re.M)
SIGNED_GETTER = re.compile(r"(^  int get \w+ => )(getInt(8|16|32)Field\(\d+(?:, defaultValue: -?\d+)?\));$", re.M)
WIDE_GETTER = re.compile(r"^  int get (\w+) => get(Uint|Int)64Field\((\d+)\);$", re.M)
WIDE_SETTER = re.compile(r"^  set (\w+)\(int v\) \{\n    set(Uint|Int)64Field\((\d+), v\);\n  \}", re.M)
FLOAT_GETTER = re.compile(r"getFloat64Field\((\d+)(, defaultValue: [\d.eE+-]+)?\)")
FLOAT_SETTER = re.compile(r"setFloat64Field\((\d+), v(, defaultValue: [\d.eE+-]+)?\)")


def float_fields(source: str) -> str:
    # Upstream implements Float64 default masking through Uint64, which dart2js
    # does not support. XOR each 32-bit word without converting the float to int.
    if len(FLOAT_GETTER.findall(source)) != source.count('getFloat64Field(') or len(FLOAT_SETTER.findall(source)) != source.count('setFloat64Field('):
        raise ValueError('unsupported Float64 field shape')
    if not FLOAT_GETTER.search(source) and not FLOAT_SETTER.search(source):
        return source
    source = FLOAT_GETTER.sub(lambda m: f'_readWebFloat64(this, {m[1]}{m[2] or ""})', source)
    source = FLOAT_SETTER.sub(lambda m: f'_writeWebFloat64(this, {m[1]}, v{m[2] or ""})', source)
    return source + '''
double _readWebFloat64(StructReader reader, int offset, {double defaultValue = 0.0}) {
  final bits = ByteData(8)..setFloat64(0, defaultValue, Endian.little);
  for (var word = 0; word < 8; word += 4) {
    bits.setUint32(word, reader.getUint32Field(offset + word) ^ bits.getUint32(word, Endian.little), Endian.little);
  }
  return bits.getFloat64(0, Endian.little);
}
void _writeWebFloat64(StructBuilder builder, int offset, double value, {double defaultValue = 0.0}) {
  final bits = ByteData(8)..setFloat64(0, value, Endian.little);
  final mask = ByteData(8)..setFloat64(0, defaultValue, Endian.little);
  for (var word = 0; word < 8; word += 4) {
    builder.setUint32Field(offset + word, bits.getUint32(word, Endian.little) ^ mask.getUint32(word, Endian.little));
  }
}
'''


def exact_fields(source: str, *, web: bool) -> str:
    # Fail closed if the generator adds defaults/unions or changes its layout.
    getters = list(WIDE_GETTER.finditer(source))
    setters = list(WIDE_SETTER.finditer(source))
    if len(getters) != len(re.findall(r"get(?:Uint|Int)64Field\(", source)) or len(setters) != len(re.findall(r"set(?:Uint|Int)64Field\(", source)):
        raise ValueError("unsupported 64-bit field shape")
    def getter(m):
        name, kind, offset = m[1], m[2], int(m[3])
        expression = f"((BigInt.from(getUint32Field({offset + 4}).toUnsigned(32)) << 32) | BigInt.from(getUint32Field({offset}).toUnsigned(32)))"
        if kind == "Int": expression += ".toSigned(64)"
        legacy = f"  int get {name} => _checkedWireInt({name}BigInt);" if web else m[0]
        return legacy + f"\n  BigInt get {name}BigInt => {expression};"
    def setter(m):
        name, kind, offset = m[1], m[2], int(m[3])
        legacy = f"  set {name}(int v) {{\n    if (BigInt.from(v).abs() > BigInt.from(9007199254740991)) {{ throw RangeError('Use the BigInt field for an exact 64-bit value'); }}\n    {name}BigInt = BigInt.from(v).to{'Unsigned' if kind == 'Uint' else 'Signed'}(64);\n  }}" if web else m[0]
        low = "BigInt.zero" if kind == "Uint" else "-(BigInt.one << 63)"
        high = "(BigInt.one << 64) - BigInt.one" if kind == "Uint" else "(BigInt.one << 63) - BigInt.one"
        return legacy + f"\n  set {name}BigInt(BigInt v) {{\n    if (v < {low} || v > {high}) {{ throw RangeError('64-bit field {name}'); }}\n    final bits = v.toUnsigned(64);\n    setUint32Field({offset}, (bits & BigInt.from(0xffffffff)).toInt());\n    setUint32Field({offset + 4}, (bits >> 32).toInt());\n  }}"
    result = WIDE_SETTER.sub(setter, WIDE_GETTER.sub(getter, source))
    if web and (getters or setters):
        result += "\nint _checkedWireInt(BigInt value) {\n  final signed = value.toSigned(64);\n  if (signed.abs() > BigInt.from(9007199254740991)) {\n    throw RangeError('Use the BigInt field for an exact 64-bit value');\n  }\n  return signed.toInt();\n}\n"
    return result


def adapt(name: str, source: str, *, generator: str = "tool/generate_core_client.py", exact64: bool = False) -> dict[str, str]:
    if not name.endswith(".capnp.dart"):
        raise ValueError("expected generated Cap'n Proto file")
    identities = {}
    for match in DECLARATION.finditer(source):
        identity = re.search(r"^  id: 0x([0-9a-fA-F]{16}),$", match[2], re.M)
        if identity is None or match[1] in identities:
            raise ValueError("unexpected reflection identity")
        identities[match[1]] = identity[1].lower()
    if not identities:
        raise ValueError("missing generated reflection")
    web = FACTORY.sub("", READER.sub("", DECLARATION.sub("", source)))
    web = float_fields(web)
    web, signed_getters = SIGNED_GETTER.subn(
        lambda match: f"{match[1]}{match[2]}.toSigned({match[3]});", web
    )
    if "SchemaInfo" in web or re.search(r"0x[0-9a-fA-F]{14,}", web):
        raise ValueError("unsupported metadata or large wire literal")
    native = source
    if exact64:
        native = exact_fields(native, web=False)
        web = exact_fields(web, web=True)
    refs = re.findall(r"(?:Struct|Enum)RefTypeSchemaInfo\(0x([0-9a-fA-F]{16})\)", source)
    if not set(value.lower() for value in refs).issubset(set(identities.values())):
        raise ValueError("external reflection reference needs explicit adaptation")
    stem = name[:-5]
    native_name = stem + ".native.dart"
    web_name = stem + ".web.dart"
    ids_name = stem + ".ids.dart"
    header = f"// Generated by {generator}; do not edit.\n"
    ids = header + "// Full unsigned schema IDs. Never convert these values to a JS int.\n"
    ids += "const schemaIds = <String, String>{\n"
    for key, value in identities.items():
        ids += f"  '{key}': '{value}',\n"
    ids += "};\nBigInt schemaId(String name) => BigInt.parse(schemaIds[name]!, radix: 16);\n"
    wrapper = header + f"export '{native_name}' if (dart.library.js_interop) '{web_name}';\nexport '{ids_name}';\n"
    wire_note = (
        "// Binary layouts and builders are unchanged; signed getters restore the JS sign.\n"
        if signed_getters else
        "// Binary readers, builders, field offsets and schema validation are unchanged.\n"
    )
    if exact64:
        wire_note = wire_note.replace("layouts and builders", "layouts")
        wire_note += "// Use BigInt fields for exact 64-bit values; legacy int access rejects unsafe JS values.\n"
    web = header + "// Optional upstream int-based reflection is unavailable on Web.\n" + wire_note + web
    return {name: wrapper, native_name: native + "\nconst schemaReflectionAvailable = true;\n", web_name: web + "\nconst schemaReflectionAvailable = false;\n", ids_name: ids}
