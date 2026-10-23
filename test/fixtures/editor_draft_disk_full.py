"""Exhaust only a separately prepared, identity-pinned 256 MiB virtual volume.

The plan is produced by tool/prepare_editor_test_volume.ps1. Evidence stays on
the outer D cache, so an expected full-volume error cannot consume host space.
"""
import argparse
import ctypes
from ctypes import wintypes
import hashlib
import json
import os
from pathlib import Path
import sqlite3


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('fill', 'release'))
    parser.add_argument('plan', type=Path)
    args = parser.parse_args()
    plan_path = args.plan.absolute()
    if os.name != 'nt' or plan_path.drive.lower() != 'd:':
        raise ValueError('Requires the prepared private Windows D-cache volume')
    for node in [plan_path, *plan_path.parents]:
        if node.is_symlink() or node.is_junction():
            raise ValueError('Evidence plan cannot traverse links')
    plan = json.loads(plan_path.read_text(encoding='utf-8-sig'))
    base = plan_path.parent
    mount = base/'mount'
    disk = base/'draft-test.vhdx'
    if (plan['schema'] != 'MORROW_PRIVATE_TEST_VOLUME_V1'
            or plan['mount'] != str(mount) or plan['disk'] != str(disk)
            or plan['virtual_bytes'] != 268435456 or not disk.is_file()
            or disk.is_symlink() or disk.is_junction()):
        raise ValueError('Exact prepared 256 MiB VHDX required')
    api = ctypes.WinDLL('kernel32', use_last_error=True)
    api.GetVolumeNameForVolumeMountPointW.argtypes = [wintypes.LPCWSTR, wintypes.LPWSTR, wintypes.DWORD]
    api.GetVolumeNameForVolumeMountPointW.restype = wintypes.BOOL
    api.GetDiskFreeSpaceExW.argtypes = [wintypes.LPCWSTR, ctypes.POINTER(ctypes.c_ulonglong), ctypes.POINTER(ctypes.c_ulonglong), ctypes.POINTER(ctypes.c_ulonglong)]
    api.GetDiskFreeSpaceExW.restype = wintypes.BOOL
    api.CreateFileW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, ctypes.c_void_p, wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
    api.CreateFileW.restype = wintypes.HANDLE
    api.WriteFile.argtypes = [wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(wintypes.DWORD), ctypes.c_void_p]
    api.WriteFile.restype = wintypes.BOOL
    api.FlushFileBuffers.argtypes = [wintypes.HANDLE]
    api.FlushFileBuffers.restype = wintypes.BOOL
    api.CloseHandle.argtypes = [wintypes.HANDLE]
    api.CloseHandle.restype = wintypes.BOOL
    name = ctypes.create_unicode_buffer(1024)
    mount_name = str(mount)+'\\'
    if not api.GetVolumeNameForVolumeMountPointW(mount_name, name, len(name)) or name.value.lower() != plan['volume_guid'].lower():
        raise ValueError('Original private volume is no longer mounted here')

    def capacity():
        free, size, total_free = (ctypes.c_ulonglong() for _ in range(3))
        if not api.GetDiskFreeSpaceExW(mount_name, ctypes.byref(free), ctypes.byref(size), ctypes.byref(total_free)):
            raise ctypes.WinError(ctypes.get_last_error())
        if not 0 < size.value < 268435456:
            raise ValueError('Volume capacity exceeds the exact private fixture')
        return dict(available=free.value, size=size.value, free=total_free.value)

    def save(name, value):
        with (base/name).open('x', encoding='utf-8') as stream:
            json.dump(value, stream, indent=2)

    before = capacity()
    filler = mount/'owned-full-volume.bin'
    if filler.is_symlink() or filler.is_junction():
        raise ValueError('Filler cannot redirect writes')
    save(args.action+'.reserved.json', dict(plan_sha256=hashlib.sha256(plan_path.read_bytes()).hexdigest(), before=before))
    if args.action == 'fill':
        db = mount/'store/workbench.db'
        if not db.is_file() or db.is_symlink() or db.parent.is_symlink() or db.parent.is_junction():
            raise ValueError('Original private store required before exhaustion')
        # Remove reusable WAL allocation while the original host is idle. This
        # changes no records and is evidence of preparation, not a product call.
        with sqlite3.connect(db.as_uri()+'?mode=rw', uri=True) as conn:
            checkpoint = conn.execute('PRAGMA wal_checkpoint(TRUNCATE)').fetchone()
            if checkpoint != (0, 0, 0):
                raise ValueError('Original WAL did not checkpoint completely')
        written = 0
        errors = []
        handle = api.CreateFileW(str(filler), 0x40000000, 0, None, 1, 0x80, None)
        if handle == ctypes.c_void_p(-1).value:
            raise ctypes.WinError(ctypes.get_last_error())
        try:
            # Actual bytes, not a sparse allocation or a synthetic exception.
            for amount in (1048576, 4096):
                block = ctypes.create_string_buffer(b'\x5a'*amount)
                while written < 268435456:
                    count = wintypes.DWORD()
                    if not api.WriteFile(handle, block, amount, ctypes.byref(count), None):
                        code = ctypes.get_last_error()
                        errors.append(dict(winerror=code, amount=amount))
                        if code != 112:
                            raise ctypes.WinError(code)
                        break
                    if not count.value:
                        raise ValueError('Short zero-length filler write')
                    written += count.value
            if not api.FlushFileBuffers(handle):
                raise ctypes.WinError(ctypes.get_last_error())
        finally:
            if not api.CloseHandle(handle):
                raise ctypes.WinError(ctypes.get_last_error())
        identity = filler.stat()
        after = capacity()
        if after['available'] >= 4096 or not errors or errors[-1]['winerror'] != 112:
            raise ValueError('Actual Windows ERROR_DISK_FULL not observed')
        save('fill.json', dict(complete=True, volume_guid=name.value, before=before, after=after,
            checkpoint=checkpoint, filler=str(filler), written=written, errors=errors,
            file_id=identity.st_ino, device=identity.st_dev, bytes=filler.stat().st_size))
    else:
        original = json.loads((base/'fill.json').read_bytes())
        current = filler.stat()
        if (not original['complete'] or original['volume_guid'].lower() != name.value.lower()
                or original['filler'] != str(filler) or current.st_ino != original['file_id']
                or current.st_dev != original['device'] or current.st_size != original['bytes']):
            raise ValueError('Release only the exact original filler file')
        # One verified file on the exact private volume; no recursive cleanup.
        filler.unlink()
        after = capacity()
        if after['available'] <= before['available']:
            raise ValueError('Private capacity was not recovered')
        save('release.json', dict(complete=True, volume_guid=name.value, before=before, after=after,
            removed_files=[str(filler)], database_modified=False))
    print(json.dumps(dict(action=args.action, complete=True, available=capacity()['available'])))


if __name__ == '__main__':
    main()
