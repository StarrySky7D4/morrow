"""Bind the fixed Windows policy owner to one observed deployment identity.

This module only derives source. It never looks up an account, opens an LSA
policy, changes a right, or connects to a VM. The caller must pin the original
sources and supply independently observed, approved VM and account identities.
"""

from __future__ import annotations

import re
import struct
import uuid
from collections.abc import Mapping


POLICY = "AssignPrimary1709.cs"
PURE = "AssignPrimary1709.Pure.cs"
TESTS = "AssignPrimary1709.PureTests.cs"


def account_sid_bytes(sid: str) -> bytes:
    """Encode a canonical local account SID, without resolving any account."""
    parts = sid.split("-")
    if len(parts) != 8 or parts[:4] != ["S", "1", "5", "21"]:
        raise ValueError("Expected a canonical local account SID")
    numbers = []
    for part in parts[3:]:
        if not re.fullmatch(r"0|[1-9][0-9]*", part) or int(part) > 0xFFFFFFFF:
            raise ValueError("Noncanonical or overflowing SID subauthority")
        numbers.append(int(part))
    return bytes([1, 5, 0, 0, 0, 0, 0, 5]) + struct.pack("<5I", *numbers)


def decode_account_sid(data: bytes) -> str:
    if len(data) != 28 or data[:8] != bytes([1, 5, 0, 0, 0, 0, 0, 5]):
        raise ValueError("Unexpected local account SID binary shape")
    numbers = struct.unpack("<5I", data[8:])
    if numbers[0] != 21:
        raise ValueError("Expected a local account domain SID")
    return "S-1-5-" + "-".join(str(value) for value in numbers)


_ENCODER = '''  internal static byte[] TargetBinarySid(){
   if(!CanonicalSid(FixedTarget.Sid))throw new InvalidOperationException("FIXED_TARGET_SID_SYNTAX");
   string[] parts=FixedTarget.Sid.Split('-');
   if(parts.Length!=8||parts[2]!="5"||parts[3]!="21")throw new InvalidOperationException("FIXED_LOCAL_ACCOUNT_SID_SHAPE");
   byte[] b=new byte[28];b[0]=1;b[1]=5;b[7]=5;
   for(int i=0;i<5;i++){
    uint value=UInt32.Parse(parts[i+3],System.Globalization.CultureInfo.InvariantCulture);
    for(int j=0;j<4;j++)b[8+4*i+j]=(byte)(value>>(8*j));
   }
   return b;
  }
  internal static bool TargetBinarySidMatches(byte[] b){
   if(b==null||b.Length!=28||b[0]!=1||b[1]!=5||b[2]!=0||b[3]!=0||b[4]!=0||b[5]!=0||b[6]!=0||b[7]!=5)return false;
   string value="S-1-5";
   for(int i=0;i<5;i++){
    uint sub=0;for(int j=0;j<4;j++)sub|=((uint)b[8+4*i+j])<<(8*j);
    value+="-"+sub.ToString(System.Globalization.CultureInfo.InvariantCulture);
   }
   return String.Equals(value,FixedTarget.Sid,StringComparison.Ordinal);
  }'''


def _replace_once(text: str, old: str, new: str) -> str:
    if text.count(old) != 1:
        raise ValueError("Expected exactly one original policy source anchor")
    return text.replace(old, new)


def _constant(text: str, name: str) -> str:
    values = re.findall(r'public const string ' + name + r'="([^"\r\n]+)";', text)
    if len(values) != 1:
        raise ValueError("Missing or ambiguous fixed target constant")
    return values[0]


def rebind_policy_sources(
    sources: Mapping[str, str], *, vm_id: str, account_sid: str
) -> tuple[dict[str, str], dict[str, object]]:
    if set(sources) != {POLICY, PURE, TESTS}:
        raise ValueError("Expected the three exact policy source files")
    if str(uuid.UUID(vm_id)) != vm_id:
        raise ValueError("Expected a canonical VM UUID")
    expected = account_sid_bytes(account_sid)
    result = {name: value.replace("\r\n", "\n") for name, value in sources.items()}
    pure = result[PURE]
    old_vm, old_sid = _constant(pure, "Vm"), _constant(pure, "Sid")
    if _constant(pure, "Right") != "SeAssignPrimaryTokenPrivilege":
        raise ValueError("The one-right boundary changed")
    if _constant(pure, "Account") != r"windev2407eval\\User":
        raise ValueError("The fixed account boundary changed")
    account_sid_bytes(old_sid)
    original = re.findall(
        r"  internal static byte\[\] TargetBinarySid\(\)\{\n"
        r"   uint\[\] sub=\{([0-9,]+)\};\n"
        r"   byte\[\] b=new byte\[28\];b\[0\]=1;b\[1\]=5;b\[7\]=5;\n"
        r"   for\(int i=0;i<sub.Length;i\+\+\)Buffer.BlockCopy\(BitConverter.GetBytes\(sub\[i\]\),0,b,8\+4\*i,4\);return b;\n"
        r"  \}", pure
    )
    if len(original) != 1:
        raise ValueError("Original fixed binary SID constructor changed")
    encoded_sid = "S-1-5-" + original[0].replace(",", "-")
    account_sid_bytes(encoded_sid)
    old_encoder = (
        "  internal static byte[] TargetBinarySid(){\n"
        "   uint[] sub={" + original[0] + "};\n"
        "   byte[] b=new byte[28];b[0]=1;b[1]=5;b[7]=5;\n"
        "   for(int i=0;i<sub.Length;i++)Buffer.BlockCopy(BitConverter.GetBytes(sub[i]),0,b,8+4*i,4);return b;\n"
        "  }"
    )
    pure = _replace_once(pure, old_encoder, _ENCODER)
    pure = _replace_once(pure, f'public const string Vm="{old_vm}";', f'public const string Vm="{vm_id}";')
    pure = _replace_once(pure, f'public const string Sid="{old_sid}";', f'public const string Sid="{account_sid}";')
    result[PURE] = pure
    layout = '     Need(apply,Marshal.SizeOf(typeof(Unicode))==16&&Marshal.SizeOf(typeof(Attributes))==48,"X64_LSA_LAYOUT");'
    guard = '     Need(apply,PolicyRules.TargetBinarySidMatches(sidBytes),"FIXED_TARGET_BINARY_SID_MISMATCH");\n'
    result[POLICY] = _replace_once(result[POLICY], layout, guard + layout)
    # Preserve the existing independent literal-vector test, then add a corrupt
    # input check to that same test. The original 18-test inventory stays intact.
    tests = result[TESTS]
    numbers = [int(value) for value in account_sid.split("-")[3:]]
    original_assertions = re.findall(
        r"   Assert\(BitConverter.ToUInt32\(b,8\)==21&&BitConverter.ToUInt32\(b,12\)==[0-9]+u\n"
        r"    &&BitConverter.ToUInt32\(b,16\)==[0-9]+u&&BitConverter.ToUInt32\(b,20\)==[0-9]+u&&BitConverter.ToUInt32\(b,24\)==[0-9]+\);", tests
    )
    if len(original_assertions) != 1:
        raise ValueError("Original binary SID regression test changed")
    assertions = (
        f"   Assert(BitConverter.ToUInt32(b,8)==21&&BitConverter.ToUInt32(b,12)=={numbers[1]}u\n"
        f"    &&BitConverter.ToUInt32(b,16)=={numbers[2]}u&&BitConverter.ToUInt32(b,20)=={numbers[3]}u&&BitConverter.ToUInt32(b,24)=={numbers[4]});\n"
        "   Assert(PolicyRules.TargetBinarySidMatches(b));b[12]^=1;Assert(!PolicyRules.TargetBinarySidMatches(b));\n"
        "   Assert(!PolicyRules.TargetBinarySidMatches(null)&&!PolicyRules.TargetBinarySidMatches(new byte[27]));"
    )
    result[TESTS] = _replace_once(tests, original_assertions[0], assertions)
    if result[POLICY].index(guard.strip()) > result[POLICY].index("InitializeInput(apply);InitializeInput(restore);"):
        raise ValueError("Binary identity guard must precede native input allocation")
    return result, {
        "original_vm_id": old_vm,
        "original_declared_sid": old_sid,
        "original_encoded_sid": encoded_sid,
        "original_identity_consistent": old_sid == encoded_sid,
        "vm_id": vm_id,
        "account_sid": account_sid,
        "expected_binary_sid_hex": expected.hex(),
        "native_input_derived_from_fixed_text": True,
        "guard_before_native_allocation_and_lsa": True,
        "vm_calls": 0,
        "lsa_calls": 0,
    }
