import 'package:capnproto_dart/capnproto_dart.dart';

final maxUint64 = (BigInt.one << 64) - BigInt.one;
final _low = (BigInt.one << 32) - BigInt.one;

/// Wire offsets come from the generated schema; no signed/Number carrier.
void writeUint64(StructBuilder value, int offset, BigInt number) {
  if (number < BigInt.zero || number > maxUint64) {
    throw const FormatException('UInt64 range');
  }
  value.setUint32Field(offset, (number & _low).toInt());
  value.setUint32Field(offset + 4, (number >> 32).toInt());
}

BigInt readUint64(StructReader value, int offset) =>
    BigInt.from(value.getUint32Field(offset)) |
    (BigInt.from(value.getUint32Field(offset + 4)) << 32);
