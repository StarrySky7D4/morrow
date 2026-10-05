"""Independent directory discovery refuses authority/identity inflation."""
import copy
from importlib.machinery import SourceFileLoader
from pathlib import Path
import unittest

fixtures = SourceFileLoader('directory_discovery_fixtures', str(Path(__file__).with_name('test_sdk_profiles.py'))).load_module()
sdk = fixtures.sdk

def directory_descriptor(os='windows', arch='x86_64'):
    value = fixtures.detailed_descriptor(os)
    value['platform']['arch'] = arch
    for original in value['experimental_extensions']['discovery']['profiles']:
        if 'io' in original['contracts']: original['contracts']['io']['sha256'] = sdk.DIRECTORY_IO_SCHEMA_SHA256
    native = os == 'windows' and arch != 'wasm32'
    profile = {'id': 'morrow.fs-directory-request.v1', 'status': 'experimental', 'guest_abi_version': 2,
               'package_schema_version': 1, 'qualification': 'not_established_by_discovery',
               'required_features': ['io-v1', 'fs-directory-request-v1'], 'optional_features': [],
               'workbench_routes': [], 'explicit_trusted_routes': ['IoWorker::submit_directory_guest_frame'],
               'production_public_binding_available': False, 'automatic_run_available': False,
               'combined_dependency_import': False, 'combined_channel_import': False,
               'runtime_static_preparation_supported': True, 'native_source_adapter_compiled': native,
               'import': {'module': 'morrow_fs_directory_v1', 'name': 'call', 'parameters': ['i32'] * 4, 'result': 'i32'},
               'contracts': copy.deepcopy(value['profiles'][0]['contracts']),
               'hard_byte_limits': dict(sdk.DIRECTORY_REQUEST_BYTE_LIMITS), 'declaration_versions': {'io': 1},
               'count_limits': {'resident_observations': 8}, 'duration_limits_ms': {},
               'selection_scope': dict(sdk.DIRECTORY_SELECTION_SCOPE),
               'helper_profile': {'version': 1, 'request_schema_sha256': sdk.DIRECTORY_REQUEST_SCHEMA_SHA256,
                                  'response_original_request_sha256': True, 'one_pending_call': True,
                                  'automatic_retry': False, 'authority': 'none',
                                  'input': 'original_open_request_wire', 'completion': 'exact_last_directory_response_wire',
                                  'standard_typed_task_helpers': False, 'task_read_input_capacity': 131072},
               'implemented_operations': [{'name': action, 'route': 'managed_directory_owner', 'implementation_compiled': native,
                                            'platform_requirement': 'windows'} for action in ('Open', 'Next', 'Finish', 'Cancel')],
               'unsupported_operations': ['GuestCapture', 'GuestPath', 'ReadFileContents', 'Watch', 'Rename', 'Delete', 'ConditionalReplace', 'DurableReopen', 'NonWindowsDirectoryOwner'],
               'host_prerequisites': ['live original FileList binding and precaptured selection']}
    profile['contracts'] = {key: profile['contracts'][key] for key in ('runtime', 'task')}
    for name, pin in [('io', sdk.DIRECTORY_IO_SCHEMA_SHA256), ('directory_request', sdk.DIRECTORY_REQUEST_SCHEMA_SHA256), ('directory_page', sdk.DIRECTORY_PAGE_SCHEMA_SHA256)]:
        profile['contracts'][name] = {'version': 1, 'sha256': pin}
    value['experimental_extensions']['directory_request_discovery'] = {'schema_version': 1, 'status': 'compiled_metadata_only',
        'authority': 'none', 'package_preflight_required': True, 'profiles': [profile]}
    return value

def profile(value):
    return value['experimental_extensions']['directory_request_discovery']['profiles'][0]

class DirectoryDiscovery(unittest.TestCase):
    def test_old_descriptor_and_four_records_remain_unchanged(self):
        old = directory_descriptor()
        del old['experimental_extensions']['directory_request_discovery']
        current = directory_descriptor()
        self.assertIs(sdk.validate_descriptor(old), old)
        self.assertIs(sdk.validate_descriptor(current), current)
        reduced = copy.deepcopy(current)
        del reduced['experimental_extensions']['directory_request_discovery']
        self.assertEqual(reduced, old)

    def test_native_availability_is_independent_of_product_or_platform_qualification(self):
        for os, arch in [('windows', 'x86_64'), ('linux', 'x86_64'), ('macos', 'aarch64'), ('android', 'aarch64'), ('web', 'wasm32')]:
            current = directory_descriptor(os, arch)
            self.assertIs(sdk.validate_descriptor(current), current)
            changed = copy.deepcopy(current)
            profile(changed)['native_source_adapter_compiled'] = not profile(changed)['native_source_adapter_compiled']
            with self.subTest(os=os), self.assertRaises(ValueError): sdk.validate_descriptor(changed)

    def test_original_runtime7_task3_are_inherited_without_requiring_directory_version1(self):
        current = directory_descriptor()
        for key, version in [('runtime', 7), ('task', 3)]:
            current['profiles'][0]['contracts'][key]['version'] = version
            profile(current)['contracts'][key]['version'] = version
            for original in current['experimental_extensions']['discovery']['profiles']:
                original['contracts'][key]['version'] = version
        self.assertIs(sdk.validate_descriptor(current), current)
        profile(current)['contracts']['task']['version'] = 1
        with self.assertRaises(ValueError): sdk.validate_descriptor(current)

    def test_all_boolean_authority_and_scope_flags_reject_integer_or_missing_values(self):
        for key in sdk.DIRECTORY_SELECTION_SCOPE:
            for bad in (True, False, 0, 1, None, 'false', 'missing'):
                if type(bad) is bool and bad == sdk.DIRECTORY_SELECTION_SCOPE[key]: continue
                current = directory_descriptor()
                if bad == 'missing': del profile(current)['selection_scope'][key]
                else: profile(current)['selection_scope'][key] = bad
                with self.subTest(key=key, value=bad), self.assertRaises(ValueError): sdk.validate_descriptor(current)

    def test_import_identity_features_schema_and_helper_do_not_fallback(self):
        mutations = [lambda p: p['import'].update(module='morrow_io_v1'),
                     lambda p: p['import'].update(parameters=['i32'] * 3),
                     lambda p: p.update(required_features=['io-v1']),
                     lambda p: p['contracts']['directory_request'].update(sha256='0' * 64),
                     lambda p: p['contracts']['directory_page'].update(version=2),
                     lambda p: p['helper_profile'].update(automatic_retry=True),
                     lambda p: p['helper_profile'].update(standard_typed_task_helpers=True),
                     lambda p: p['helper_profile'].update(task_read_input_capacity=True),
                     lambda p: p.update(workbench_routes=['external_transform'])]
        for change in mutations:
            current = directory_descriptor(); change(profile(current))
            with self.subTest(change=change), self.assertRaises(ValueError): sdk.validate_descriptor(current)

    def test_valid_hex_but_foreign_io_schema_cannot_rebind_the_directory_profile(self):
        current = directory_descriptor()
        profile(current)['contracts']['io']['sha256'] = 'b' * 64
        with self.assertRaises(ValueError): sdk.validate_descriptor(current)
        for original in current['experimental_extensions']['discovery']['profiles']:
            if 'io' in original['contracts']: original['contracts']['io']['sha256'] = 'b' * 64
        with self.assertRaises(ValueError): sdk.validate_descriptor(current)

    def test_caps_and_operations_cannot_be_inflated_or_silently_omitted(self):
        for change in [lambda p: p['hard_byte_limits'].update(response=131072),
                       lambda p: p['hard_byte_limits'].update(request=True),
                       lambda p: p['implemented_operations'].pop(),
                       lambda p: p['implemented_operations'][0].update(implementation_compiled=1),
                       lambda p: p['unsupported_operations'].remove('GuestCapture')]:
            current = directory_descriptor(); change(profile(current))
            with self.subTest(change=change), self.assertRaises(ValueError): sdk.validate_descriptor(current)

if __name__ == '__main__': unittest.main()
