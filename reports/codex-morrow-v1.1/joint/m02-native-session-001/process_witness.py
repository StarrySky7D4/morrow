"""Read-only OS observations for explicitly launched host/child PIDs.

Keeps an OS process handle so later observations do not follow PID reuse.
Does not discover personal configuration, launch processes, or terminate them.
"""
import ctypes
from ctypes import wintypes as W
import os
import time

class ProcessEntry(ctypes.Structure):
    _fields_=[('dwSize',W.DWORD),('cntUsage',W.DWORD),('th32ProcessID',W.DWORD),
      ('th32DefaultHeapID',ctypes.c_size_t),('th32ModuleID',W.DWORD),('cntThreads',W.DWORD),
      ('th32ParentProcessID',W.DWORD),('pcPriClassBase',W.LONG),('dwFlags',W.DWORD),
      ('szExeFile',W.WCHAR*260)]

def kernel():
    if os.name!='nt': raise RuntimeError('Windows observation only; other platforms not qualified')
    k=ctypes.WinDLL('kernel32',use_last_error=True)
    declarations={
      'OpenProcess':([W.DWORD,W.BOOL,W.DWORD],W.HANDLE),
      'CloseHandle':([W.HANDLE],W.BOOL),
      'QueryFullProcessImageNameW':([W.HANDLE,W.DWORD,W.LPWSTR,ctypes.POINTER(W.DWORD)],W.BOOL),
      'GetProcessTimes':([W.HANDLE]+[ctypes.POINTER(W.FILETIME)]*4,W.BOOL),
      'GetExitCodeProcess':([W.HANDLE,ctypes.POINTER(W.DWORD)],W.BOOL),
      'WaitForSingleObject':([W.HANDLE,W.DWORD],W.DWORD),
      'CreateToolhelp32Snapshot':([W.DWORD,W.DWORD],W.HANDLE),
      'Process32FirstW':([W.HANDLE,ctypes.POINTER(ProcessEntry)],W.BOOL),
      'Process32NextW':([W.HANDLE,ctypes.POINTER(ProcessEntry)],W.BOOL),
    }
    for name,(args,result) in declarations.items():
        fn=getattr(k,name); fn.argtypes=args; fn.restype=result
    return k

def parent_pid(k,pid):
    snap=k.CreateToolhelp32Snapshot(2,0)
    if snap==ctypes.c_void_p(-1).value: raise ctypes.WinError(ctypes.get_last_error())
    try:
        entry=ProcessEntry(); entry.dwSize=ctypes.sizeof(entry)
        ok=k.Process32FirstW(snap,ctypes.byref(entry))
        while ok:
            if entry.th32ProcessID==pid:
                return int(entry.th32ParentProcessID)
            ok=k.Process32NextW(snap,ctypes.byref(entry))
        raise ProcessLookupError(pid)
    finally: k.CloseHandle(snap)

def filetime(v): return (int(v.dwHighDateTime)<<32)|int(v.dwLowDateTime)

class ProcessWitness:
    def __init__(self,pid,expected_parent=None):
        if not isinstance(pid,int) or pid<=0: raise ValueError('invalid explicit PID')
        self.k=kernel(); self.pid=pid; self.handle=None
        self.observed_at_ns=time.monotonic_ns()
        # QUERY_LIMITED_INFORMATION | SYNCHRONIZE; no process mutation rights.
        self.handle=self.k.OpenProcess(0x1000|0x00100000,False,pid)
        if not self.handle: raise ctypes.WinError(ctypes.get_last_error())
        try:
            self.parent=parent_pid(self.k,pid)
            if expected_parent is not None and self.parent!=expected_parent:
                raise ValueError('PID not a child of the launched host')
            buf=ctypes.create_unicode_buffer(32768); size=W.DWORD(len(buf))
            if not self.k.QueryFullProcessImageNameW(self.handle,0,buf,ctypes.byref(size)):
                raise ctypes.WinError(ctypes.get_last_error())
            self.image=buf.value
            c,e,kt,ut=(W.FILETIME() for _ in range(4))
            if not self.k.GetProcessTimes(self.handle,ctypes.byref(c),ctypes.byref(e),ctypes.byref(kt),ctypes.byref(ut)):
                raise ctypes.WinError(ctypes.get_last_error())
            self.creation_filetime=filetime(c)
        except BaseException:
            self.close(); raise

    def snapshot(self):
        if not self.handle: raise ValueError('closed witness')
        wait=self.k.WaitForSingleObject(self.handle,0)
        if wait not in (0,258): raise ctypes.WinError(ctypes.get_last_error())
        code=W.DWORD()
        if not self.k.GetExitCodeProcess(self.handle,ctypes.byref(code)):
            raise ctypes.WinError(ctypes.get_last_error())
        return {'pid':self.pid,'parent_pid':self.parent,'image_path':self.image,
                'creation_filetime_100ns':self.creation_filetime,
                'first_observed_monotonic_ns':self.observed_at_ns,
                'observed_monotonic_ns':time.monotonic_ns(),
                'process_handle_signaled':wait==0,'exit_code':int(code.value) if wait==0 else None,
                'scope':'OS_handle_and_parent_snapshot_not_image_attestation_or_output_EOF'}

    def close(self):
        if self.handle:
            self.k.CloseHandle(self.handle); self.handle=None

    def __enter__(self): return self
    def __exit__(self,*exc): self.close()
