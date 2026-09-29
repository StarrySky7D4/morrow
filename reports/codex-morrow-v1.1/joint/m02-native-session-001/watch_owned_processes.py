"""Read-only discovery limited to descendants of the reviewer-launched host."""
import ctypes,threading,time
from pathlib import Path
from process_witness import ProcessEntry,ProcessWitness,kernel
from check_review import sha

class OwnedWatcher:
    def __init__(self,host_pid,client_path,host_created):
        self.host_pid=host_pid;self.client_path=Path(client_path).resolve();self.host_created=host_created
        self.parents={host_pid};self.witnesses={};self.initial={};self.errors=[]
        self.lock=threading.Lock();self.stop_event=threading.Event()
        self.thread=threading.Thread(target=self._watch,daemon=True);self.thread.start()
    def add_reported_child(self,pid):
        # Enables discovery of an output holder even if its parent exited before
        # the first snapshot. Parent relationship provenance remains explicit.
        with self.lock:self.parents.add(pid)
    def _watch(self):
        k=kernel()
        while not self.stop_event.is_set():
            snap=k.CreateToolhelp32Snapshot(2,0)
            if snap==ctypes.c_void_p(-1).value:
                self.errors.append('process_snapshot_failed');return
            try:
                entry=ProcessEntry();entry.dwSize=ctypes.sizeof(entry)
                ok=k.Process32FirstW(snap,ctypes.byref(entry))
                candidates=[]
                with self.lock:parents=set(self.parents)
                while ok:
                    pid=int(entry.th32ProcessID);parent=int(entry.th32ParentProcessID)
                    if parent in parents and pid not in self.witnesses:candidates.append((pid,parent))
                    ok=k.Process32NextW(snap,ctypes.byref(entry))
                for pid,parent in candidates:
                    try:
                        w=ProcessWitness(pid,parent);s=w.snapshot()
                        if Path(s['image_path']).resolve()!=self.client_path or s['creation_filetime_100ns']<self.host_created:
                            w.close();self.errors.append('unexpected_descendant_image_or_creation');continue
                        s['image_path_sha256']=sha(s['image_path'])
                        self.witnesses[pid]=w;self.initial[pid]=s
                        with self.lock:self.parents.add(pid)
                    except OSError:pass # A fast exit is a missed observation, never fabricated proof.
            finally:k.CloseHandle(snap)
            self.stop_event.wait(.005)
    def finish(self):
        self.stop_event.set();self.thread.join(timeout=1)
        if self.thread.is_alive():return {'status':'observer_unconfirmed','errors':['watcher_thread_not_joined']}
        result=[]
        for pid,w in self.witnesses.items():
            result.append({'initial':self.initial[pid],'final':w.snapshot()});w.close()
        return {'status':'observed','processes':result,'errors':self.errors,
          'scope':'OS descendant PID/parent/image/times only; no process-tree containment proof'}
