import datetime
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RECEIPTS = ROOT / 'receipts/p02-source-batch-001'

def read(path):
    return json.loads(path.read_text(encoding='utf-8'))

def reference(path):
    return {'path':path.relative_to(ROOT).as_posix(), 'sha256':hashlib.sha256(path.read_bytes()).hexdigest()}

target = RECEIPTS/'source-inventory.json'
if target.exists():
    raise ValueError('Refuse to overwrite a completed source inventory')
sources = []
for source_id in ['codex','cc-switch']:
    verification_path = RECEIPTS/(source_id+'-source-verification.json')
    verified = read(verification_path)
    if verified['status'] != 'complete_fixed_source_verified' or not verified['complete_commit_tree_verified']:
        raise ValueError('Source is not fully verified: '+source_id)
    archive_path = RECEIPTS/(source_id+'-archive-attempt.json')
    archive = read(archive_path)
    source = {'id':source_id, 'kind':'complete_fixed_snapshot', 'url':verified['repository'], 'commit':verified['commit'], 'tree_sha1':verified['tree_sha1'], 'path':verified['materialization'], 'complete_repository_source':True, 'git_checkout':False, 'git_history_complete':False, 'files_manifest':verified['content_manifest'], 'tree_manifest':reference(RECEIPTS/(source_id+'-complete-git-tree.json')), 'commit_api':reference(RECEIPTS/(source_id+'-commit-api.json')), 'verification_receipt':reference(verification_path), 'file_count':verified['file_count'], 'tree_count_including_root':verified['tree_count'], 'source_bytes':verified['source_bytes'], 'license_files':[{'path':f['path'],'sha256':f['sha256'],'git_blob_sha1':f['git_blob_sha1']} for f in verified['license_files']], 'dirty':{'allowed':False, 'basis':verified['dirty_diff']['basis'], 'modified':[], 'missing':[], 'extra':[]}, 'materialization_limits':verified['mode_materialization'], 'archive_transport':{'status':archive['status'], 'bytes_received':archive['bytes_received'], 'sha256_of_received_bytes':archive['sha256_of_received_bytes'], 'receipt':reference(archive_path)}, 'source_method':verified['source_method']}
    sources.append(source)
inventory = {'schema_version':1, 'batch_id':'p02-source-batch-001', 'status':'both_complete_fixed_source_trees_verified', 'sources':sources, 'scope':'Repository source acquisition and byte/tree identity only; external dependency closure, compilation, runtime isolation, and product behavior are separately qualified', 'previous_stage_source_sets_and_locks_modified':False, 'source_audit_executed_upstream_build_or_program':False, 'acquisition_notes':reference(RECEIPTS/'ACQUISITION.md'), 'read_only_recheck_tool':reference(ROOT/'upstream/p02-source-batch-001/verify_materialized_source.py'), 'created_at':datetime.datetime.now(datetime.timezone.utc).isoformat()}
target.write_text(json.dumps(inventory,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
print(json.dumps({'path':target.relative_to(ROOT).as_posix(),'sha256':hashlib.sha256(target.read_bytes()).hexdigest(),'sources':[(s['id'],s['file_count'],s['tree_sha1']) for s in sources]},indent=2))
