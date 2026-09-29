"""Only explicit known-PID handles. No process enumeration or termination API.

Parentage is supplied by the launch/evidence source and is NOT an independent
OS-parent assertion. Identity combines a held handle, creation time, image and
image-file digest. File digest is not a loaded-image attestation.
"""
import ctypes,hashlib,os,time
from ctypes import wintypes as W
from pathlib import Path

def filetime(value):return (int(value.dwHighDateTime)<<32)|int(value.dwLowDateTime)
def kernel():
    if os.name!='nt':raise RuntimeError('Windows-only observer')
    k=ctypes.WinDLL('kernel32',use_last_error=True)
    declarations={
      'OpenProcess':([W.DWORD,W.BOOL,W.DWORD],W.HANDLE),
      'CloseHandle':([W.HANDLE],W.BOOL),
      'GetProcessId':([W.HANDLE],W.DWORD),
      'QueryFullProcessImageNameW':([W.HANDLE,W.DWORD,W.LPWSTR,ctypes.POINTER(W.DWORD)],W.BOOL),
      'GetProcessTimes':([W.HANDLE]+[ctypes.POINTER(W.FILETIME)]*4,W.BOOL),
      'GetExitCodeProcess':([W.HANDLE,ctypes.POINTER(W.DWORD)],W.BOOL),
      'WaitForSingleObject':([W.HANDLE,W.DWORD],W.DWORD),
    }
    for name,(args,result) in declarations.items():
        fn=getattr(k,name);fn.argtypes=args;fn.restype=result
    return k

class KnownProcess:
    def __init__(self,pid,expected_image,expected_sha256,source,not_before_filetime=None):
        if not isinstance(pid,int) or pid<=0 or not source:raise ValueError('explicit PID and provenance required')
        self.k=kernel();self.pid=pid;self.handle=None;self.source=source;self.first_ns=time.monotonic_ns()
        self.handle=self.k.OpenProcess(0x1000|0x00100000,False,pid)
        if not self.handle:raise ctypes.WinError(ctypes.get_last_error())
        try:
            if self.k.GetProcessId(self.handle)!=pid:raise ValueError('handle PID mismatch')
            size=W.DWORD(32768);buf=ctypes.create_unicode_buffer(size.value)
            if not self.k.QueryFullProcessImageNameW(self.handle,0,buf,ctypes.byref(size)):raise ctypes.WinError(ctypes.get_last_error())
            self.image=Path(buf.value)
            if self.image.resolve()!=Path(expected_image).resolve():raise ValueError('unexpected process image')
            with self.image.open('rb') as f:self.digest=hashlib.file_digest(f,'sha256').hexdigest()
            if self.digest!=expected_sha256:raise ValueError('unexpected image file digest')
            c,e,k,u=(W.FILETIME() for _ in range(4))
            if not self.k.GetProcessTimes(self.handle,*[ctypes.byref(v) for v in [c,e,k,u]]):raise ctypes.WinError(ctypes.get_last_error())
            self.created=filetime(c)
            if not_before_filetime is not None and self.created<not_before_filetime:raise ValueError('process predates this launch')
        except BaseException:self.close();raise
    def snapshot(self):
        if not self.handle:raise ValueError('closed handle')
        wait=self.k.WaitForSingleObject(self.handle,0)
        if wait not in [0,258]:raise ctypes.WinError(ctypes.get_last_error())
        code=W.DWORD()
        if not self.k.GetExitCodeProcess(self.handle,ctypes.byref(code)):raise ctypes.WinError(ctypes.get_last_error())
        return {'pid':self.pid,'image_path':str(self.image),'image_file_sha256':self.digest,'creation_filetime_100ns':self.created,'first_observed_monotonic_ns':self.first_ns,'observed_monotonic_ns':time.monotonic_ns(),'handle_signaled':wait==0,'exit_code':int(code.value) if wait==0 else None,'pid_source':self.source,'OS_parent_independently_observed':False,'scope':'held explicit PID handle; no machine enumeration, no image attestation, no output EOF proof'}
    def close(self):
        if self.handle:self.k.CloseHandle(self.handle);self.handle=None
    def __enter__(self):return self
    def __exit__(self,*args):self.close()

if __name__=='__main__':
    import sys
    from check_review import HERE,sha,write
    run=HERE/'runs/known-process-self-check-001';run.mkdir(parents=True,exist_ok=False)
    with KnownProcess(os.getpid(),sys.executable,sha(sys.executable),'reviewer self process only') as p:
        s=p.snapshot()
        if s['handle_signaled']:raise ValueError('self unexpectedly exited')
        write(run/'result.json',{'status':'read_only_observer_self_check_passed','snapshot':s,'observer_sha256':sha(__file__),'host_client_runtime_cases':0,'product_pass_credit':0})
    print('Read-only known-PID self observation verified; no host/client run.')
