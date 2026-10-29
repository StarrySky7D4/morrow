"""Regress the textual/binary policy identity split without native side effects."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from windows.rebind_fixed_policy import (  # noqa: E402
    POLICY, PURE, TESTS, account_sid_bytes, decode_account_sid,
    rebind_policy_sources,
)

OLD = "S-1-5-21-2826996052-4136963862-3109888007-1000"
NEW = "S-1-5-21-307430596-3187481878-1423720967-1000"
VM = "b077176b-5c7d-4549-aa93-5aa09b4a8e85"


def inconsistent_sources():
    # The same split observed in case684: the textual SID is already new,
    # while the native binary constructor and its literal test remain old.
    return {
        PURE: r'''  public const string Vm="b077176b-5c7d-4549-aa93-5aa09b4a8e85";
  public const string Sid="S-1-5-21-307430596-3187481878-1423720967-1000";
  public const string Right="SeAssignPrimaryTokenPrivilege";
  public const string Account="windev2407eval\\User";
  internal static byte[] TargetBinarySid(){
   uint[] sub={21,2826996052,4136963862,3109888007,1000};
   byte[] b=new byte[28];b[0]=1;b[1]=5;b[7]=5;
   for(int i=0;i<sub.Length;i++)Buffer.BlockCopy(BitConverter.GetBytes(sub[i]),0,b,8+4*i,4);return b;
  }''',
        POLICY: '''     Need(apply,Marshal.SizeOf(typeof(Unicode))==16&&Marshal.SizeOf(typeof(Attributes))==48,"X64_LSA_LAYOUT");
     InitializeInput(apply);InitializeInput(restore);
     Open(apply,0x811);''',
        TESTS: '''   Assert(BitConverter.ToUInt32(b,8)==21&&BitConverter.ToUInt32(b,12)==2826996052u
    &&BitConverter.ToUInt32(b,16)==4136963862u&&BitConverter.ToUInt32(b,20)==3109888007u&&BitConverter.ToUInt32(b,24)==1000);''',
    }


class FixedPolicyDeploymentTests(unittest.TestCase):
    def test_observed_split_is_reported_and_native_encoder_has_one_identity(self):
        result, proof = rebind_policy_sources(inconsistent_sources(), vm_id=VM, account_sid=NEW)
        self.assertEqual(proof["original_encoded_sid"], OLD)
        self.assertEqual(proof["original_declared_sid"], NEW)
        self.assertFalse(proof["original_identity_consistent"])
        self.assertNotIn("2826996052", "".join(result.values()))
        self.assertIn("FixedTarget.Sid.Split('-')", result[PURE])
        self.assertLess(result[POLICY].index("TargetBinarySidMatches"), result[POLICY].index("InitializeInput"))
        self.assertIn("b[12]^=1", result[TESTS])
        self.assertEqual(proof["vm_calls"], 0)
        self.assertEqual(proof["lsa_calls"], 0)

    def test_new_domain_and_rid_bind_all_representations(self):
        sid = "S-1-5-21-4294967295-0-2147483648-1001"
        result, proof = rebind_policy_sources(inconsistent_sources(), vm_id=VM, account_sid=sid)
        self.assertIn(f'public const string Sid="{sid}";', result[PURE])
        self.assertIn("==4294967295u", result[TESTS])
        self.assertIn("==1001);", result[TESTS])
        self.assertEqual(decode_account_sid(bytes.fromhex(proof["expected_binary_sid_hex"])), sid)

    def test_sid_input_rejects_ambiguous_nonlocal_and_overflow_values(self):
        invalid = ["S-1-5-19", NEW + "-1", NEW.replace("307430596", "0307430596"),
                   NEW.replace("307430596", "4294967296"), NEW.replace("307430596", "+1"),
                   NEW.replace("307430596", "１２"), NEW + "\n", NEW.lower()]
        for sid in invalid:
            with self.subTest(sid=sid), self.assertRaises(ValueError):
                account_sid_bytes(sid)

    def test_binary_decoder_rejects_extra_truncated_and_wrong_header(self):
        valid = account_sid_bytes(NEW)
        for data in (valid[:-1], valid + b"\0", bytes([2]) + valid[1:],
                     valid[:2] + b"\1" + valid[3:], valid[:8] + b"\x16" + valid[9:]):
            with self.subTest(data=data), self.assertRaises(ValueError):
                decode_account_sid(data)

    def test_right_account_and_source_shape_cannot_expand(self):
        for name, old, new in ((PURE, "SeAssignPrimaryTokenPrivilege", "SeDebugPrivilege"),
                               (PURE, "windev2407eval", "different-host"),
                               (POLICY, "X64_LSA_LAYOUT", "CHANGED_LAYOUT"),
                               (PURE, "uint[] sub=", "uint[] changed=")):
            sources = inconsistent_sources()
            sources[name] = sources[name].replace(old, new)
            with self.subTest(name=name, old=old), self.assertRaises(ValueError):
                rebind_policy_sources(sources, vm_id=VM, account_sid=NEW)

    def test_duplicate_constants_and_already_patched_inputs_are_rejected(self):
        sources = inconsistent_sources()
        sources[PURE] += f'\npublic const string Sid="{NEW}";'
        with self.assertRaises(ValueError):
            rebind_policy_sources(sources, vm_id=VM, account_sid=NEW)
        derived, _ = rebind_policy_sources(inconsistent_sources(), vm_id=VM, account_sid=NEW)
        with self.assertRaises(ValueError):
            rebind_policy_sources(derived, vm_id=VM, account_sid=NEW)


if __name__ == "__main__":
    unittest.main()
