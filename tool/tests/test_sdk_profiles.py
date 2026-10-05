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


def detailed_descriptor(os='windows'):
    """Schema fixture; real compiled output and old-validator compatibility are
    exercised separately by the CLI qualification, not inferred from this data.
    """
    value = descriptor()
    value['platform']['os'] = os
    details = {'schema_version': 1, 'status': 'compiled_metadata_only', 'authority': 'none',
               'package_preflight_required': True, 'policy': 'Exported ceilings; no grants.',
               'current_host_requirements': {key: os == 'windows' for key in (
                   'protected_owner_backend_compiled', 'stored_http_credentials_compiled',
                   'protected_tls_identity_compiled')}, 'profiles': []}
    for name, layout in sdk.EXTENSION_LAYOUTS.items():
        mutation = name == 'morrow.mutation.v1'
        profile = {'id': name, 'status': 'experimental', 'guest_abi_version': 2,
                   'package_schema_version': 1, 'production_public_binding_available': False,
                   'combined_dependency_import': False, 'combined_channel_import': False,
                   'qualification': 'not_established_by_discovery',
                   'contracts': {key: {'version': 1, 'sha256': ('0' if key in ('runtime', 'task') else 'a')*64} for key in layout['contracts']},
                   'required_features': list(layout['required']), 'optional_features': list(layout['optional']),
                   'declaration_versions': {key: 1 for key in layout['versions']},
                   'hard_byte_limits': {key: 1024 for key in layout['bytes']},
                   'count_limits': {key: 8 for key in layout['counts']},
                   'duration_limits_ms': {key: 30000 for key in layout['durations']},
                   'unsupported_operations': sorted(layout['unsupported']),
                   'explicit_trusted_routes': ['trusted native route'], 'host_prerequisites': ['current grant'],
                   'implemented_operations': [
                       {'name': operation, 'route': route,
                        'implementation_compiled': os == 'windows' if mutation else True,
                        'platform_requirement': 'windows' if mutation else 'native'}
                       for operation, route in layout['operations'].items()]}
        details['profiles'].append(profile)
    details['profiles'][0]['declared_capabilities'] = [
        {'name': name, 'number': number} for number, name in enumerate((
            'FileRead', 'FileList', 'FileCreate', 'FileReplace', 'FileDelete', 'HttpRequest',
            'HttpListen', 'HttpPublish', 'CredentialUse', 'WebSocketConnect'), 1)]
    details['profiles'][0]['operation_history'] = {
        'capability': 'HttpRequest', 'single_exact_operation': True, 'fresh_grant_required': True,
        'status_only': True, 'body_available': False, 'dispatch_authority': False,
        'automatic_replay': False, 'general_recovery': False, 'raw_or_brokered_query_route': False}
    details['profiles'][2]['header'] = 'morrow-service-resources-v1'
    value['experimental_extensions']['discovery'] = details
    return value


def payload_descriptor(os='windows', arch='x86_64'):
    value = detailed_descriptor(os)
    value['platform']['arch'] = arch
    channel = copy.deepcopy(value['profiles'][0])
    channel.update(id='morrow.channel.v1', status='experimental', workbench_routes=[],
                   runtime_supported_required_features=['transform-handlers-v1', 'channel-v1'])
    channel['contracts'] = {k: channel['contracts'][k] for k in ('runtime', 'task')}
    channel['contracts']['channel'] = {'version': 1, 'sha256': 'c'*64}
    channel['workbench_constraints'].update(channel_binding=False)
    channel['channel_scope'] = {'trusted_local_sources': True, 'managed_binding_required': True,
                                'workbench_binding': False, 'network_backend': False,
                                'cloud_account': False, 'automatic_replay': False}
    channel['payload_discovery'] = {'schema_version': 1, 'status': 'compiled_metadata_only',
        'authority': 'none', 'package_preflight_required': True, 'profiles': [{
        'id': 'morrow.changes-metadata.v1', 'status': 'experimental',
        'required_features': ['channel-v1', 'changes-metadata-v1'],
        'payload_contract': {'version': 1, 'sha256': '07fc0dc4c48eb17f85309207a441d6a07f3fb8e06aab5e2ce337a4b3c98f9089'},
        'wire_limits': {'header_bytes': 150, 'payload_bytes': 662, 'card_id_bytes': 256,
                        'operation_id_bytes': 256, 'cursor_bytes': 32}, 'count_limits': {'cards': 32},
        'channel_kind': 'Events', 'duplex': False, 'finite_window': True, 'metadata_only': True,
        'runtime_static_preparation_supported': True, 'native_source_required': True,
        'native_source_adapter_compiled': arch != 'wasm32',
        'native_prerequisites': ['managed_channel_broker', 'fresh_receiver_specific_changes_approval', 'exact_package_binding', 'live_store_binding'],
        'workbench_routes': [], 'production_public_binding_available': False,
        'automatic_run_available': False, 'qualification': 'not_established_by_discovery'}]}
    value['profiles'].append(channel)
    return value


def discovery(value):
    return value['experimental_extensions']['discovery']


class Profiles(unittest.TestCase):
    def test_valid_descriptor_preserves_no_authority_and_candidate(self):
        value=sdk.validate_descriptor(descriptor())
        self.assertEqual(value['authority'],'none')
        self.assertFalse(value['profiles'][0]['workbench_constraints']['dependency_calls'])

    def test_channel_profile_cannot_claim_unbound_workbench_network_cloud_or_replay(self):
        item=descriptor();channel=copy.deepcopy(item['profiles'][0]);channel['id']='morrow.channel.v1'
        channel['status']='experimental';channel['contracts']['channel']={'version':1,'sha256':'1'*64}
        channel['workbench_routes']=[]
        channel['workbench_constraints']['channel_binding']=False
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

    def test_channel_constraints_are_present_and_exactly_false(self):
        item=descriptor();channel=copy.deepcopy(item['profiles'][0]);channel['id']='morrow.channel.v1'
        channel['status']='experimental';channel['contracts']['channel']={'version':1,'sha256':'1'*64}
        channel['workbench_routes']=[]
        channel['workbench_constraints']['channel_binding']=False
        channel['channel_scope']={'trusted_local_sources':True,'managed_binding_required':True,
                                 'workbench_binding':False,'network_backend':False,'cloud_account':False,'automatic_replay':False}
        item['profiles'].append(channel);sdk.validate_descriptor(item)
        for field in ('content_task','dependency_calls','required_dependencies','channel_binding'):
            for bad in ('missing',True,0,'false',None):
                invalid=copy.deepcopy(item)
                if bad=='missing':del invalid['profiles'][1]['workbench_constraints'][field]
                else:invalid['profiles'][1]['workbench_constraints'][field]=bad
                with self.subTest(field=field,value=bad),self.assertRaises(ValueError):
                    sdk.validate_descriptor(invalid)

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

    def test_optional_discovery_preserves_legacy_descriptor_and_status(self):
        old = descriptor()
        self.assertIs(sdk.validate_descriptor(old), old)
        current = detailed_descriptor()
        self.assertIs(sdk.validate_descriptor(current), current)
        self.assertEqual(current['experimental_extensions']['status'], old['experimental_extensions']['status'])
        self.assertEqual(current['profiles'], old['profiles'])

    def test_extension_metadata_does_not_claim_linux_or_macos_owner_readiness(self):
        for os in ('windows', 'linux', 'macos'):
            with self.subTest(os=os):
                item = detailed_descriptor(os)
                sdk.validate_descriptor(item)
                profiles = discovery(item)['profiles']
                self.assertTrue(profiles[0]['implemented_operations'][0]['implementation_compiled'])
                self.assertEqual(profiles[3]['implemented_operations'][0]['implementation_compiled'], os == 'windows')
                invalid = copy.deepcopy(item)
                discovery(invalid)['current_host_requirements']['protected_owner_backend_compiled'] = os != 'windows'
                with self.assertRaises(ValueError): sdk.validate_descriptor(invalid)

    def test_extension_discovery_version_status_and_authority_are_exact(self):
        for key, bad in (('schema_version', True), ('schema_version', 2), ('schema_version', None),
                         ('status', 'stable'), ('authority', 'granted'), ('package_preflight_required', 1)):
            item = detailed_descriptor(); discovery(item)[key] = bad
            with self.subTest(key=key, bad=bad), self.assertRaises(ValueError): sdk.validate_descriptor(item)
        for bad in (None, [], 'unknown'):
            item = detailed_descriptor(); item['experimental_extensions']['discovery'] = bad
            with self.subTest(bad=bad), self.assertRaises(ValueError): sdk.validate_descriptor(item)

    def test_extension_profiles_are_complete_unique_and_versioned(self):
        for action in ('missing', 'duplicate', 'unknown', 'abi_bool', 'package_bool', 'contract_missing', 'contract_version', 'bad_digest'):
            item = detailed_descriptor(); profiles = discovery(item)['profiles']
            if action == 'missing': profiles.pop()
            elif action == 'duplicate': profiles[1] = copy.deepcopy(profiles[0])
            elif action == 'unknown': profiles[0]['id'] = 'morrow.unknown.v1'
            elif action == 'abi_bool': profiles[0]['guest_abi_version'] = True
            elif action == 'package_bool': profiles[0]['package_schema_version'] = True
            elif action == 'contract_missing': del profiles[0]['contracts']['io']
            elif action == 'contract_version': profiles[0]['contracts']['io']['version'] = 2
            else: profiles[0]['contracts']['io']['sha256'] = 'A'*64
            with self.subTest(action=action), self.assertRaises(ValueError): sdk.validate_descriptor(item)

    def test_extension_public_scope_and_import_composition_cannot_be_promoted(self):
        for index in range(4):
            for key in ('production_public_binding_available', 'combined_dependency_import', 'combined_channel_import'):
                for bad in (True, 0, 'false', None):
                    item = detailed_descriptor(); discovery(item)['profiles'][index][key] = bad
                    with self.subTest(index=index, key=key, bad=bad), self.assertRaises(ValueError): sdk.validate_descriptor(item)

    def test_extension_limits_are_typed_bounded_and_in_the_right_units(self):
        for field in ('hard_byte_limits', 'count_limits', 'duration_limits_ms', 'declaration_versions'):
            key = next(iter(discovery(detailed_descriptor())['profiles'][0][field]))
            for bad in (0, -1, True, 1.5, '1', 1 << 63, None):
                item = detailed_descriptor(); discovery(item)['profiles'][0][field][key] = bad
                with self.subTest(field=field, bad=bad), self.assertRaises(ValueError): sdk.validate_descriptor(item)
            item = detailed_descriptor(); del discovery(item)['profiles'][0][field][key]
            with self.subTest(field=field), self.assertRaises(ValueError): sdk.validate_descriptor(item)
        item = detailed_descriptor(); discovery(item)['profiles'][2]['duration_limits_ms']['timeout'] = 300000
        with self.assertRaises(ValueError): sdk.validate_descriptor(item)

    def test_io_declarations_are_not_implemented_public_operations(self):
        item = detailed_descriptor(); io = discovery(item)['profiles'][0]
        self.assertIn('WebSocketConnect', [c['name'] for c in io['declared_capabilities']])
        self.assertNotIn('WebSocketConnect', [o['name'] for o in io['implemented_operations']])
        for operation in ('SubmitWebSocketConnect', 'SubmitFileList', 'Poll', 'Write'):
            invalid = copy.deepcopy(item)
            discovery(invalid)['profiles'][0]['unsupported_operations'].remove(operation)
            with self.subTest(operation=operation), self.assertRaises(ValueError): sdk.validate_descriptor(invalid)
        for field, bad in (('number', True), ('number', 99), ('name', 'Exec')):
            invalid = copy.deepcopy(item); discovery(invalid)['profiles'][0]['declared_capabilities'][0][field] = bad
            with self.subTest(field=field), self.assertRaises(ValueError): sdk.validate_descriptor(invalid)

    def test_implemented_operation_routes_and_platforms_are_not_interchangeable(self):
        for index in range(4):
            for key, bad in (('route', 'raw'), ('platform_requirement', 'any'), ('implementation_compiled', 1), ('name', 'Exec')):
                item = detailed_descriptor('linux'); discovery(item)['profiles'][index]['implemented_operations'][0][key] = bad
                with self.subTest(index=index, key=key), self.assertRaises(ValueError): sdk.validate_descriptor(item)
        item = detailed_descriptor(); operations = discovery(item)['profiles'][0]['implemented_operations']
        operations[1] = copy.deepcopy(operations[0])
        with self.assertRaises(ValueError): sdk.validate_descriptor(item)

    def test_http_history_cannot_be_recast_as_replay_or_general_recovery(self):
        original = discovery(detailed_descriptor())['profiles'][0]['operation_history']
        for key, value in original.items():
            for bad in (('FileCreate',) if key == 'capability' else (not value, 1 if value else 0, None)):
                item = detailed_descriptor(); discovery(item)['profiles'][0]['operation_history'][key] = bad
                with self.subTest(key=key, bad=bad), self.assertRaises(ValueError): sdk.validate_descriptor(item)

    def test_service_and_mutation_opt_in_features_are_not_blanket_requirements(self):
        item = detailed_descriptor(); profiles = discovery(item)['profiles']
        self.assertEqual(profiles[1]['required_features'], ['io-v1'])
        self.assertEqual(profiles[1]['optional_features'], ['service-run-v1', 'service-run-budget-v1'])
        self.assertEqual(profiles[3]['optional_features'], ['mutation-budget-v1'])
        for index in (1, 3):
            invalid = copy.deepcopy(item); p = discovery(invalid)['profiles'][index]
            p['required_features'].extend(p['optional_features']); p['optional_features'] = []
            with self.subTest(index=index), self.assertRaises(ValueError): sdk.validate_descriptor(invalid)

    def test_resource_header_and_required_prerequisites_are_bounded(self):
        for field, bad in (('header', 'dynamic-discovery'), ('host_prerequisites', []),
                           ('host_prerequisites', ['x'*257]), ('explicit_trusted_routes', ['same', 'same'])):
            item = detailed_descriptor(); discovery(item)['profiles'][2][field] = bad
            with self.subTest(field=field), self.assertRaises(ValueError): sdk.validate_descriptor(item)

    def test_extension_compiled_contract_identities_must_agree(self):
        for index, contract, field, bad in ((0, 'runtime', 'sha256', 'b'*64),
                                          (0, 'task', 'version', 4),
                                          (1, 'io', 'sha256', 'b'*64),
                                          (2, 'service', 'sha256', 'b'*64)):
            item = detailed_descriptor(); discovery(item)['profiles'][index]['contracts'][contract][field] = bad
            with self.subTest(index=index, contract=contract, field=field), self.assertRaisesRegex(ValueError, 'inconsistent'):
                sdk.validate_descriptor(item)

class PayloadDiscovery(unittest.TestCase):
    def test_optional_payload_detail_preserves_the_entire_old_projection(self):
        item = payload_descriptor(); legacy = copy.deepcopy(item)
        del legacy['profiles'][1]['payload_discovery']
        self.assertIs(sdk.validate_descriptor(legacy), legacy)
        self.assertIs(sdk.validate_descriptor(item), item)
        projected = copy.deepcopy(item); del projected['profiles'][1]['payload_discovery']
        self.assertEqual(projected, legacy)
        self.assertEqual(len(discovery(item)['profiles']), 4)
        self.assertEqual(item['profiles'][1]['runtime_supported_required_features'], ['transform-handlers-v1', 'channel-v1'])

    def test_payload_identity_matches_exact_wire_bytes_and_refuses_unknown_contracts(self):
        import hashlib
        self.assertEqual(sdk.CHANGES_PAYLOAD_SHA256, hashlib.sha256((ROOT/'core/schemas/changes_metadata_v1.wire').read_bytes()).hexdigest())
        for field, bad in [('version', True), ('version', 1.0), ('version', 2), ('sha256', 'a'*64), ('sha256', None)]:
            item=payload_descriptor(); item['profiles'][1]['payload_discovery']['profiles'][0]['payload_contract'][field]=bad
            with self.subTest(field=field,bad=bad), self.assertRaises(ValueError): sdk.validate_descriptor(item)

    def test_payload_envelope_and_record_shape_are_bounded_and_versioned(self):
        original=payload_descriptor()['profiles'][1]['payload_discovery']
        changes=[None, [], {}, {**original,'schema_version':True}, {**original,'schema_version':2},
                 {**original,'profiles':[]}, {**original,'profiles':original['profiles']*2},
                 {**original,'authority':'granted'}, {**original,'package_preflight_required':1}]
        for bad in changes:
            item=payload_descriptor(); item['profiles'][1]['payload_discovery']=bad
            with self.subTest(bad=bad), self.assertRaises(ValueError): sdk.validate_descriptor(item)
        for key in original['profiles'][0]:
            item=payload_descriptor(); del item['profiles'][1]['payload_discovery']['profiles'][0][key]
            with self.subTest(missing=key), self.assertRaises(ValueError): sdk.validate_descriptor(item)

    def test_payload_bounds_reject_numeric_lookalikes_and_enlargement(self):
        profile=payload_descriptor()['profiles'][1]['payload_discovery']['profiles'][0]
        for group in ('wire_limits','count_limits'):
            for key, value in profile[group].items():
                for bad in (True,float(value),value+1,0):
                    item=payload_descriptor(); item['profiles'][1]['payload_discovery']['profiles'][0][group][key]=bad
                    with self.subTest(group=group,key=key,bad=bad), self.assertRaises(ValueError): sdk.validate_descriptor(item)

    def test_payload_requires_the_exact_feature_and_native_approval_prerequisites(self):
        for key,bad in [('required_features',['changes-metadata-v1']), ('required_features',['channel-v1','io-v1']),
                        ('native_prerequisites',[]),('native_prerequisites',['managed_channel_broker']),
                        ('channel_kind','ByteStream'),('id','morrow.unknown.v1')]:
            item=payload_descriptor(); item['profiles'][1]['payload_discovery']['profiles'][0][key]=bad
            with self.subTest(key=key,bad=bad), self.assertRaises(ValueError): sdk.validate_descriptor(item)

    def test_payload_platform_metadata_never_promotes_routes_or_automatic_execution(self):
        for os,arch in [('windows','x86_64'),('linux','x86_64'),('unknown','wasm32')]:
            item=payload_descriptor(os,arch); sdk.validate_descriptor(item)
            p=item['profiles'][1]['payload_discovery']['profiles'][0]
            for key in ('duplex','finite_window','metadata_only','runtime_static_preparation_supported',
                        'native_source_required','native_source_adapter_compiled','production_public_binding_available','automatic_run_available'):
                for bad in (not p[key], int(p[key])):
                    invalid=copy.deepcopy(item); invalid['profiles'][1]['payload_discovery']['profiles'][0][key]=bad
                    with self.subTest(os=os,arch=arch,key=key,bad=bad), self.assertRaises(ValueError): sdk.validate_descriptor(invalid)
            invalid=copy.deepcopy(item); invalid['profiles'][1]['payload_discovery']['profiles'][0]['workbench_routes']=['changes']
            with self.assertRaises(ValueError): sdk.validate_descriptor(invalid)


if __name__=='__main__':unittest.main()
