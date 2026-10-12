"""Ordinary synthetic supervisor protocol peer. Never opens a real library/host."""
import json, pathlib, queue, struct, sys, threading, time, os, traceback
root=pathlib.Path(__file__).parent
commands=queue.Queue()
state={'version':2,'phase':'Running','owner_phase':'Retained','profile':'a'*64,
 'incarnation':'b'*64,'host_sha256':'c'*64,'package_sha256':'d'*64,
 'supervisor_sha256':'e'*64,'generation':1,'child_pid':os.getpid()+1,
 'supervisor_pid':os.getpid(),'supervisor_creation_filetime':1,'child_creation_filetime':2,
 'protocol_ack':'not_applicable','business_outcome':'Unknown','normal_shutdown':False,
 'durable_proof':False,'child_exit_code':None,
 **{k:False for k in ('resource_reclaimed','tree_empty','child_exit','stdout_eof','stderr_eof','control_reaped','gate_closed')}}
summary={'ordinary_synthetic_only':True,'real_native_host':False,'heartbeats':0,
 'graceful_close_requested':False,'request_count':0,'kind1_chunks':0,'max_kind1_payload':0}

def write_json(name,value):
 with (root/name).open('x',encoding='utf-8') as f:json.dump(value,f)

def exception_detail(error,stage,request_index=None):
 # Diagnostic data only: no retry, state change, or protocol success inference.
 truncated=[]
 def text(name,value,limit):
  if value is None:return None
  raw=str(value).encode('utf-8',errors='replace')
  if len(raw)>limit:
   truncated.append(name);raw=raw[:limit]
  return raw.decode('utf-8',errors='ignore')
 value={'ordinary_synthetic_only':True,'type':type(error).__name__,'stage':stage,'request_index':request_index,
  'errno':getattr(error,'errno',None),'winerror':getattr(error,'winerror',None),
  'message':text('message',error,2048),
  'filename':text('filename',getattr(error,'filename',None),2048),
  'filename2':text('filename2',getattr(error,'filename2',None),2048),
  'traceback':text('traceback',''.join(traceback.format_exception(error)),16384),
  'truncated_fields':truncated,'diagnostic_cap_bytes':32768}
 # JSON escaping can expand control characters; retain an explicit bounded tail.
 while len(json.dumps(value).encode('utf-8'))>32768:
  field=max(('traceback','message','filename','filename2'),key=lambda k:len(value[k] or ''))
  value[field]=value[field][:len(value[field])//2]
  if field not in truncated:truncated.append(field)
 return value

def emit(kind,payload):
 # Splitting the outer header is supplementary; each inner header below is
 # deterministically split across separate outer kind1 frames.
 data=struct.pack('<I',len(payload)+1)+bytes([kind])+payload
 sys.stdout.buffer.write(data[:2]);sys.stdout.buffer.flush()
 sys.stdout.buffer.write(data[2:]);sys.stdout.buffer.flush()

def status():
 emit(2,json.dumps(state,separators=(',',':')).encode())

def host_bytes(data):
 frame=struct.pack('<I',len(data))+data
 for start,end in [(0,2)]+[(i,min(i+16384,len(frame))) for i in range(2,len(frame),16384)]:
  block=frame[start:end]
  summary['kind1_chunks']+=1
  summary['max_kind1_payload']=max(summary['max_kind1_payload'],len(block))
  emit(1,block)

def exact(n):
 out=b''
 while len(out)<n:
  block=sys.stdin.buffer.raw.read(n-len(out))
  if not block:raise EOFError()
  out+=block
 return out

def reader():
 try:
  while True:
   length=struct.unpack('<I',exact(4))[0]
   if not 1<=length<=131073:raise ValueError('fixed original outer input cap')
   packet=exact(length);commands.put((packet[0],packet[1:]))
 except EOFError:commands.put(('EOF',b''))
 except BaseException as e:
  try:write_json('reader-error',exception_detail(e,'stdin-reader'))
  finally:commands.put(('ERROR',type(e).__name__.encode()))

threading.Thread(target=reader,daemon=True).start()
pending=None;index=0;normal=False;omit_deadline=None;stage='initial-status'
try:
 status()
 while True:
  stage='command-wait'
  try:kind,payload=commands.get(timeout=.002)
  except queue.Empty:kind,payload=None,None
  if kind=='EOF':break
  if kind=='ERROR':raise ValueError(payload.decode())
  if kind==2:
   if payload:raise ValueError('heartbeat body')
   summary['heartbeats']+=1
  elif kind==3:
   if payload:raise ValueError('close body')
   summary['graceful_close_requested']=True
   state['phase']='Closing';state['gate_closed']=True;status()
  elif kind==1:
   if pending is not None or not payload or len(payload)>131072:raise ValueError('single ordinary RPC slot')
   stage='request-write'
   with (root/('request.%d.tmp'%index)).open('xb') as f:f.write(payload)
   stage='request-publish'
   (root/('request.%d.tmp'%index)).rename(root/('request.%d.bin'%index))
   summary['request_count']+=1;pending=index
  elif kind is not None:raise ValueError('unexpected outer kind')
  stage='status-command-probe'
  change=root/'status-command.json'
  if change.exists() and not (root/'status-command-observed').exists():
   stage='status-command-read-and-emit'
   update=json.loads(change.read_text());state.update(update);status()
   write_json('status-command-observed',{'synthetic':True})
  stage='reply-control-probe'
  if pending is not None and (root/('reply.%d.json'%pending)).exists():
   stage='reply-control-read'
   control=json.loads((root/('reply.%d.json'%pending)).read_text())
   stage='reply-data-read'
   data=(root/('reply.%d.bin'%pending)).read_bytes()
   if len(data)>393217:raise ValueError('synthetic output evidence bound')
   if control.get('drift'):
    state['generation']+=1;status()
   if control.get('gate'):
    state['gate_closed']=True;status()
   if control.get('drop'):
    write_json('deliberate-eof',{'synthetic_missing_read_reply':True});break
   stage='reply-emit'
   if control.get('outer_bad'):
    sys.stdout.buffer.write(struct.pack('<I',131074));sys.stdout.buffer.flush()
    write_json('outer-budget-probe',{'declared_outer_bytes':131074})
   else:host_bytes(data)
   pending=None;index+=1
  stage='graceful-release-probe'
  if summary['graceful_close_requested'] and pending is None:
   if (root/'omit-release-proof').exists():
    if omit_deadline is None:omit_deadline=time.monotonic()+10
    if not (root/'allow-exit-without-proof').exists():
     if time.monotonic()>=omit_deadline:raise TimeoutError('synthetic omitted-proof hold deadline')
     continue
    summary['release_proof_deliberately_omitted']=True
    break
   state.update({'phase':'Released','owner_phase':'Released','normal_shutdown':True,
    'durable_proof':True,'child_exit_code':0,
    **{k:True for k in ('resource_reclaimed','tree_empty','child_exit','stdout_eof','stderr_eof','control_reaped','gate_closed')}})
   stage='graceful-release-emit'
   status();normal=True;break
except BrokenPipeError:
 summary['synthetic_output_pipe_closed']=True
except BaseException as e:
 write_json('child-error',exception_detail(e,stage,pending));raise
finally:
 summary['synthetic_protocol_released']=normal
 write_json('transport-summary.json',summary)
 write_json('drain-complete',{'synthetic_process_finally':True})
