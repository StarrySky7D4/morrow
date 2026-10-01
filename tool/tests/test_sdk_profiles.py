import copy
import importlib.util
import json
from pathlib import Path
import sys
import time
import unittest

ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('sdk_profiles',ROOT/'tool/sdk_profiles.py')
sdk=importlib.util.module_from_spec(spec);spec.loader.exec_module(sdk)

def descriptor():
    return {'schema_version':1,'host_version':'0.1.9-test.50','platform':{'os':'windows','arch':'x86_64'},'backend':'wasmi','authority':'none','package_preflight_required':True,'profiles':[{'id':'morrow.guest-task.v3','status':'candidate','guest_abi_version':2,'package_schema_version':1,'contracts':{name:{'sha256':'0'*64,**({'version':1} if name!='content' else {})} for name in ('runtime','content','task','ui','dependency_call')},'runtime_task_modes':['content','transform'],'workbench_routes':['external_transform','external_ui'],'runtime_supported_required_features':['transform-handlers-v1'],'workbench_constraints':{'content_task':False,'dependency_calls':False,'required_dependencies':False},'hard_byte_limits':{'task_frame':131072},'runtime_defaults':{'fuel':20000000,'memory_bytes':16777216,'host_calls':16}}],'unsupported_requirements':['public_streaming_session'],'experimental_extensions':{'status':'recognized_experimental_not_fully_discovered','feature_names':['io-v1']}}

class Profiles(unittest.TestCase):
    def test_valid_descriptor_preserves_no_authority_and_candidate(self):
        value=sdk.validate_descriptor(descriptor())
        self.assertEqual(value['authority'],'none')
        self.assertFalse(value['profiles'][0]['workbench_constraints']['dependency_calls'])

    def test_channel_profile_cannot_claim_unbound_workbench_network_cloud_or_replay(self):
        item=descriptor();channel=copy.deepcopy(item['profiles'][0]);channel['id']='morrow.channel.v1'
        channel['status']='experimental';channel['contracts']['channel']={'version':1,'sha256':'1'*64}
        channel['workbench_routes']=[]
        channel['channel_scope']={'trusted_local_sources':True,'managed_binding_required':True,
                                 'workbench_binding':False,'network_backend':False,'cloud_account':False,'automatic_replay':False}
        item['profiles'].append(channel);sdk.validate_descriptor(item)
        for key in channel['channel_scope']:
            invalid=copy.deepcopy(item);invalid['profiles'][1]['channel_scope'][key]=not channel['channel_scope'][key]
            with self.subTest(key=key),self.assertRaisesRegex(ValueError,'scope'):sdk.validate_descriptor(invalid)
        invalid=copy.deepcopy(item);invalid['profiles'][1]['workbench_routes']=['external_transform']
        with self.assertRaisesRegex(ValueError,'scope'):sdk.validate_descriptor(invalid)
        invalid=copy.deepcopy(item);del invalid['profiles'][1]['contracts']['channel']
        with self.assertRaisesRegex(ValueError,'contracts'):sdk.validate_descriptor(invalid)

    def test_bad_versions_and_missing_contract_are_refused(self):
        for field,value in (('guest_abi_version','wrong'),('package_schema_version',True),('guest_abi_version',0)):
            item=descriptor();item['profiles'][0][field]=value
            with self.subTest(field=field,value=value),self.assertRaises(ValueError):sdk.validate_descriptor(item)
        item=descriptor();del item['profiles'][0]['contracts']['task']
        with self.assertRaises(ValueError):sdk.validate_descriptor(item)

    def test_bad_authority_routes_and_limits_are_refused(self):
        items=[]
        item=descriptor();item['authority']='granted';items.append(item)
        item=descriptor();item['profiles'][0]['runtime_defaults']['fuel']=False;items.append(item)
        item=descriptor();item['profiles'][0]['runtime_supported_required_features']*=2;items.append(item)
        item=descriptor();item['profiles'][0]['workbench_constraints']['dependency_calls']='false';items.append(item)
        item=descriptor();item['profiles'][0]['hard_byte_limits']['task_frame']=-1;items.append(item)
        for item in items:
            with self.subTest(item=item),self.assertRaises(ValueError):sdk.validate_descriptor(item)

    def test_duplicate_json_keys_are_refused_at_any_depth(self):
        for raw in ('{"schema_version":1,"schema_version":1}','{"nested":{"a":1,"a":2}}'):
            with self.assertRaises(ValueError):json.loads(raw,object_pairs_hook=sdk.unique_object)

    def test_actual_process_exact_output_limit_and_oversize(self):
        for stream in ('stdout','stderr'):
            with self.subTest(stream=stream):
                raw=sdk.bounded_process([sys.executable,'-c',f'import sys;sys.{stream}.buffer.write(b"x"*65536)'])
                self.assertEqual(len(raw),65536 if stream=='stdout' else 0)
                with self.assertRaisesRegex(ValueError,'exceeds'):
                    sdk.bounded_process([sys.executable,'-c',f'import sys;sys.{stream}.buffer.write(b"x"*65537)'])

    def test_actual_process_timeout_and_nonzero_are_refused(self):
        start=time.monotonic()
        with self.assertRaisesRegex(ValueError,'timed out'):
            sdk.bounded_process([sys.executable,'-c','import time;time.sleep(30)'])
        self.assertLess(time.monotonic()-start,8)
        with self.assertRaisesRegex(ValueError,'exit 7'):
            sdk.bounded_process([sys.executable,'-c','raise SystemExit(7)'])

if __name__=='__main__':unittest.main()
