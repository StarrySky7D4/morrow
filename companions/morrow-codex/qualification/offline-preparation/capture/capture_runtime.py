"""Capture-only definitions. Import never creates files, threads or processes."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import threading
import time

def write_new(path, value):
    raw=(json.dumps(value,sort_keys=True,separators=(',',':'))+'\n').encode('utf-8')
    if len(raw)>16384: raise ValueError('Fixed capture record bound')
    with Path(path).open('xb') as sink:
        sink.write(raw); sink.flush(); os.fsync(sink.fileno())

class CaptureState:
    """Owns the one returned child, pipes, readers and sinks on every outcome."""
    def __init__(self, root, maximum, writer, sync):
        self.root=Path(root); self.maximum=maximum; self.writer=writer; self.sync=sync
        self.child=None; self.child_pid=None; self.pipes=[None,None]; self.sinks=[]
        self.threads=[]; self.started=[]; self.ready=[threading.Event(),threading.Event()]
        self.handoff=threading.Event(); self.cancel=False; self.faults=[[],[]]
        self.read_bytes=[0,0]; self.written_known=[0,0]; self.eof=[False,False]
        self.actual_exit=None; self.terminal_attempted=False; self.terminal_saved=False
        self.validation_attempted=False; self.validation_saved=False
        self.unknown_history=False; self.post_faults=[]; self.outcome='PREPARING_NO_SPAWN'

    def observe_same_child(self):
        # An observation of the same retained object, not a new terminal witness.
        observed=None if self.child is None else self.child.poll()
        return {'pid':self.child_pid,'actual_exit_if_observed':observed,
                'terminal_saved':self.terminal_saved,'readers_eof':list(self.eof),
                'unknown_history':self.unknown_history,'outcome':self.outcome}

    def wait_same_child(self, timeout, join_timeout=2):
        if self.child is None: raise ValueError('No returned child object to wait')
        try: result=self.child.wait(timeout=timeout)
        except subprocess.TimeoutExpired: return self
        return finish_known_exit(self,result,join_timeout)

    def retain_forever(self):
        # This object strongly owns child/pipes/threads/sinks; no kill or respawn.
        while True: time.sleep(60)

def default_sync(sink,index):
    sink.flush(); os.fsync(sink.fileno())

def _fault(state,index,label):
    if label not in state.faults[index]: state.faults[index].append(label)

def _drain(state,index):
    state.ready[index].set()
    state.handoff.wait()
    sink=state.sinks[index]; pipe=state.pipes[index]
    try:
        if state.cancel: return
        if pipe is None: _fault(state,index,'PIPE_HANDOFF_FAILED'); return
        discard=False
        while True:
            try: chunk=pipe.read(8192)
            except Exception:
                _fault(state,index,'PIPE_READ_FAILED_SAME_CHILD_RETAINED'); break
            if not chunk:
                state.eof[index]=True; break
            state.read_bytes[index]+=len(chunk)
            if state.read_bytes[index]>state.maximum[index]:
                _fault(state,index,'LOG_BOUND_EXCEEDED_DRAIN_ONLY'); discard=True
            if discard: continue
            try:
                written=sink.write(chunk)
                if type(written) is not int or written<0 or written>len(chunk):
                    _fault(state,index,'LOG_WRITE_COUNT_UNKNOWN'); discard=True; continue
                state.written_known[index]+=written
                if written!=len(chunk):
                    _fault(state,index,'LOG_SHORT_WRITE_DRAIN_ONLY'); discard=True; continue
            except Exception:
                _fault(state,index,'LOG_WRITE_FAILED_PARTIAL_BYTES_UNKNOWN_DRAIN_ONLY'); discard=True; continue
            try: state.sync(sink,index)
            except Exception:
                _fault(state,index,'LOG_FLUSH_OR_FSYNC_FAILED_DRAIN_ONLY'); discard=True
        # No post-failure write/fsync retries. Reading continues to actual EOF.
    finally:
        try: sink.close()
        except Exception: _fault(state,index,'LOG_CLOSE_FAILED')
        # A pipe with unknown EOF remains owned by CaptureState, not closed early.
        if pipe is not None and state.eof[index]:
            try: pipe.close()
            except Exception: _fault(state,index,'PIPE_CLOSE_FAILED')

def _cancel_before_child(state):
    state.cancel=True; state.handoff.set()
    alive_indices=[]
    for index,thread in enumerate(state.threads):
        try:
            alive=thread.is_alive()
            if alive or getattr(thread,'ident',None) is not None:
                if thread not in state.started: state.started.append(thread)
                thread.join(timeout=2)
            if thread.is_alive(): alive_indices.append(index)
        except Exception:
            alive_indices.append(index); state.post_faults.append('PRESPAWN_READER_JOIN_FAILED_NO_SPAWN')
    for index,sink in enumerate(state.sinks):
        if index not in alive_indices:
            try: sink.close()
            except Exception: _fault(state,index,'PRESPAWN_CLOSE_FAILED')

def finish_known_exit(state, actual_exit, join_timeout):
    if state.child is None or state.child.pid!=state.child_pid or type(actual_exit) is not int:
        raise ValueError('Exact same returned child and actual integer exit required')
    if state.actual_exit is not None and state.actual_exit!=actual_exit:
        raise ValueError('Frozen actual exit differs')
    if not state.terminal_attempted:
        state.actual_exit=actual_exit; state.terminal_attempted=True
        # FIRST post-wait operation: one minimal immutable true-terminal attempt.
        try:
            state.writer(state.root/'actual-child-exit.json',{
                'schema':'FIXED_CAPTURE_ACTUAL_CHILD_EXIT_V1','pid':state.child_pid,
                'actual_exit_code':actual_exit,'scope':'REAL_CHILD_EXIT_NOT_VM_ORIGINAL_JOIN'})
            state.terminal_saved=True
        except Exception: state.post_faults.append('TERMINAL_RECORD_WRITE_FAILED_KNOWN_EXIT_RETAINED')
    # Same-exit observations may complete late EOF once; never rewrite a terminal
    # or repeat a validation already attempted (successful or failed).
    if state.validation_attempted: return state
    state.outcome='ACTUAL_EXIT_KNOWN_CAPTURE_VALIDATION_PENDING'
    for thread in state.started:
        try: thread.join(timeout=join_timeout)
        except Exception: state.post_faults.append('READER_JOIN_POSTVALIDATION_FAILED')
    alive=any(thread.is_alive() for thread in state.started)
    if alive or not all(state.eof):
        state.unknown_history=True; state.outcome='ACTUAL_EXIT_KNOWN_READERS_UNKNOWN_SAME_RESOURCES_RETAINED'
        return state
    state.validation_attempted=True
    logs=[]
    for index,name in enumerate(('stdout.log','stderr.log')):
        try:
            path=state.root/name; h=hashlib.sha256(); size=0
            with path.open('rb') as source:
                for chunk in iter(lambda:source.read(8192),b''):
                    size+=len(chunk)
                    if size>state.maximum[index]: raise ValueError('Readback log bound')
                    h.update(chunk)
            logs.append({'stream':name,'bytes_read':state.read_bytes[index],
                         'bytes_saved_observed':size,'sha256':h.hexdigest(),
                         'faults':list(state.faults[index]),'actual_EOF':state.eof[index]})
        except Exception:
            logs.append({'stream':name,'readback':'NOT_OBSERVED','faults':list(state.faults[index])})
            state.post_faults.append('LOG_HASH_POSTVALIDATION_FAILED')
    value={'schema':'FIXED_CAPTURE_VALIDATION_V1','pid':state.child_pid,
           'actual_exit_code':state.actual_exit,'terminal_record_saved':state.terminal_saved,
           'streams':logs,'post_faults':list(state.post_faults),'unknown_history':state.unknown_history,
           'actual_capture_EOF_complete':True,
           'scope':'CAPTURE_ONLY_NO_VM_GUEST_CONTENT_OR_ORIGINAL_JOIN_PROOF'}
    try: state.writer(state.root/'capture-validation.json',value); state.validation_saved=True
    except Exception: state.post_faults.append('VALIDATION_RECORD_POSTVALIDATION_FAILED')
    failed=bool(any(state.faults) or state.post_faults or state.unknown_history)
    state.outcome='ACTUAL_EXIT_PRESERVED_CAPTURE_REJECTED' if failed else 'ACTUAL_EXIT_AND_RAW_CAPTURE_BOUND'
    return state

def capture_once(spawn, root, maximum=(131072,131072), wait_timeout=300,
                 join_timeout=10, thread_factory=threading.Thread,
                 sink_factory=None, record_writer=write_new, sync=default_sync):
    """Caller owns authority; this function never constructs/retries a process."""
    state=CaptureState(root,maximum,record_writer,sync)
    if sink_factory is None: sink_factory=lambda path,index:path.open('xb',buffering=0)
    try:
        for index,name in enumerate(('stdout.log','stderr.log')):
            state.sinks.append(sink_factory(state.root/name,index))
        for index in range(2):
            thread=thread_factory(target=_drain,args=(state,index),daemon=False)
            state.threads.append(thread)
            thread.start(); state.started.append(thread)
        if not all(event.wait(timeout=2) for event in state.ready):
            raise RuntimeError('Both drain threads must be ready before spawn')
    except Exception:
        _cancel_before_child(state); state.outcome='PRESPAWN_FAILED_NO_SPAWN_CALL'
        return state
    try: child=spawn()  # Exactly one call, only after both reader starts succeeded.
    except Exception:
        _cancel_before_child(state); state.unknown_history=True
        state.outcome='SPAWN_RAISED_NO_RETURNED_HANDLE_NO_RETRY'
        try: state.writer(state.root/'spawn-attempt-unknown.json',{'status':state.outcome,'OS_effect':'UNKNOWN_NO_RETURNED_HANDLE','retry':False})
        except Exception: state.post_faults.append('SPAWN_UNKNOWN_RECORD_WRITE_FAILED')
        return state
    state.child=child; state.child_pid=child.pid
    try:
        state.pipes=[child.stdout,child.stderr]; state.handoff.set()
        result=child.wait(timeout=wait_timeout)
    except subprocess.TimeoutExpired:
        state.unknown_history=True; state.outcome='WAIT_TIMEOUT_SAME_CHILD_RESOURCES_RETAINED_NO_KILL'
        try: state.writer(state.root/'capture-unknown-retained.json',{'pid':state.child_pid,'actual_exit':None,'status':state.outcome,'retry':False,'kill':False})
        except Exception: state.post_faults.append('UNKNOWN_RECORD_WRITE_FAILED_RESOURCES_RETAINED')
        return state
    except Exception:
        state.handoff.set(); state.unknown_history=True; state.outcome='POSTSPAWN_WAIT_FAILURE_SAME_CHILD_RESOURCES_RETAINED'
        return state
    return finish_known_exit(state,result,join_timeout)
