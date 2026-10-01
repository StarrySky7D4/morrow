import contextlib
import copy
import io
from pathlib import Path
import sys
import tempfile
import unittest
ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'tool'))
import morrow_plugin as tool
import sdk_profiles

class ChannelProject(unittest.TestCase):
    def test_new_all_languages_pins_and_typed_no_authority(self):
        with tempfile.TemporaryDirectory(prefix='morrow-channel-project-014-') as temporary:
            for language in tool.LANGUAGES:
                path=Path(temporary)/language
                with contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(tool.main(['new',str(path),'--language',language,'--kind','channel','--id','channel.'+language,'--lock-sdk']),0)
                root,config,source=tool.project(path)
                self.assertEqual(config['build']['kind'],'channel')
                self.assertEqual(config['plugin']['capabilities'],[])
                self.assertFalse(config['plugin']['dependency_calls'])
                self.assertEqual(config['handlers'][0]['max_input_bytes'],65)
                self.assertEqual(config['handlers'][0]['max_output_bytes'],64)
                self.assertTrue(source.is_file())
                arguments=tool.package_arguments(config)
                self.assertIn('--channel-budget',arguments)
                self.assertEqual(arguments[arguments.index('--channel-handler')+1],'channel.exercise')
                captured=io.StringIO()
                with contextlib.redirect_stdout(captured):
                    self.assertEqual(tool.main(['validate',str(path),'--require-sdk-lock']),0)
                self.assertIn('channel.capnp',captured.getvalue())
                self.assertIn('permissions_granted = false',captured.getvalue())

    def test_explicit_directory_starters_keep_public_input_and_source_lock(self):
        with tempfile.TemporaryDirectory(prefix='morrow-channel-directory-') as temporary:
            for language in tool.LANGUAGES:
                path=Path(temporary)/language
                with contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(tool.main(['new',str(path),'--language',language,'--kind','channel',
                        '--channel-input','directory','--id','channel.directory.'+language,'--lock-sdk']),0)
                root,config,source=tool.project(path)
                self.assertEqual(config['handlers'][0]['name'],'channel.directory.consume')
                self.assertEqual(config['handlers'][0]['input_type'],'morrow.channel.directory.v1')
                self.assertEqual(config['handlers'][0]['max_input_bytes'],65536)
                self.assertEqual(config['handlers'][0]['max_output_bytes'],64)
                self.assertEqual(config['plugin']['capabilities'],[])
                self.assertEqual(config['channel']['max_requests'],128)
                self.assertEqual(config['channel']['max_messages'],32)
                arguments=tool.package_arguments(config)
                self.assertEqual(arguments[arguments.index('--channel-handler')+1],'channel.directory.consume')
                with contextlib.redirect_stdout(io.StringIO()):
                    self.assertEqual(tool.main(['validate',str(path),'--require-sdk-lock']),0)
                expected=tool.ROOT/'sdk/examples'/f'{language}-channel-directory'/('src/lib.rs' if language=='rust' else 'plugin.cpp' if language=='cpp' else 'plugin.c')
                self.assertEqual(source.read_bytes(),expected.read_bytes())

    def test_directory_option_does_not_create_non_channel_project(self):
        with tempfile.TemporaryDirectory(prefix='morrow-channel-option-') as temporary:
            path=Path(temporary)/'project'
            with contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(tool.main(['new',str(path),'--language','c','--kind','transform',
                    '--channel-input','directory','--id','channel.option']),1)
            self.assertFalse(path.exists())

    def test_invalid_ceiling_not_normalized(self):
        for key,value in [('max_channels',0),('max_frame_bytes',65537),('max_bytes',67108865),
                          ('max_messages',True),('max_requests',1000001),('max_duration_ms',3600001)]:
            budget=dict(tool.CHANNEL_DEFAULTS);budget[key]=value
            with self.subTest(key=key),self.assertRaises(tool.ToolError):tool.channel_budget(budget)
        budget=dict(tool.CHANNEL_DEFAULTS);budget['max_bytes']=1
        with self.assertRaises(tool.ToolError):tool.channel_budget(budget)

    def test_channel_no_io_dependencies_or_content_mix(self):
        with tempfile.TemporaryDirectory(prefix='morrow-channel-metadata-014-') as temporary:
            path=Path(temporary)/'project'
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(tool.main(['new',str(path),'--language','c','--kind','channel','--id','channel.negative']),0)
            metadata=path/'plugin.toml';original=metadata.read_text(encoding='utf-8')
            for bad in (original.replace('capabilities = []','capabilities = ["read-content"]'),
                        original.replace('dependency_calls = false','dependency_calls = true'),
                        original.replace('kind = "channel"','kind = "io"'),
                        original.split('[[handlers]]')[0]):
                metadata.write_text(bad,encoding='utf-8')
                with self.assertRaises(tool.ToolError):tool.project(path)

if __name__=='__main__':unittest.main()
