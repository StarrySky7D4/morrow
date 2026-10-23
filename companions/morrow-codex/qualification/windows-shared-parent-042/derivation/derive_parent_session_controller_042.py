"""Fixed 17-attempt controller profile; no runtime actions during derivation."""
from pathlib import Path
import hashlib,json,re
from derive_p2_source_policy_controller_039 import patch_controller as patch_refusal,change

BASE=Path(__file__).parent
SYMBOL='P2_SHARED_PARENT_REFUSAL_SESSION'
FLAG='--run-p2-shared-parent-refusal-session'
STATUS='P2_SEVENTEEN_SHARED_PARENT_REFUSALS_CLOSED_NOT_FULL_P2_OR_RELEASE'

def pin(path):
    raw=path.read_bytes();return dict(path=str(path),bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
def put(path,text):path.write_text(text,encoding='utf-8',newline='\n')
def patch_controller(src,out):
    originals={p.name:pin(p) for p in src.glob('*.cs')}
    per_round=patch_refusal(src,out)
    assert per_round==3557 and 17*per_round<65536
    # The default original47 and the separately qualified single-refusal profile
    # are both still compiled and tested. Shared-session compilation is explicit.
    for path in src.glob('*.cs'):
        raw=path.read_bytes().replace(b'#if P2_SOURCE_POLICY_REFUSAL_QUALIFICATION',
            b'#if P2_SOURCE_POLICY_REFUSAL_QUALIFICATION || P2_SHARED_PARENT_REFUSAL_SESSION')
        path.write_bytes(raw)
    p=src/'RootPPrebirth1701.Protocol.cs';raw=p.read_bytes()
    raw=change(raw,'#if P2_SOURCE_POLICY_REFUSAL_QUALIFICATION || P2_SHARED_PARENT_REFUSAL_SESSION\n  internal const int Count=92,NormalBytes=3557;',
        '#if '+SYMBOL+'\n  internal const int Count=1564,NormalBytes=60469;\n  internal const string RunFlag="'+FLAG+'",TerminalStatus="'+STATUS+'";\n#elif P2_SOURCE_POLICY_REFUSAL_QUALIFICATION\n  internal const int Count=92,NormalBytes=3557;')
    raw=change(raw,'return Lines[index];','return Lines[index%92];')
    raw=change(raw,'return Kinds[index];','return Kinds[index%92];')
    raw=change(raw,'return Steps[index];','return 47*(index/92)+Steps[index%92];')
    raw=change(raw,'  internal static bool Failure(','''#if P2_SHARED_PARENT_REFUSAL_SESSION
  internal const int FrameCapacity=1564;
  internal static bool Terminal(int index,byte[] wire){return index==Count-1&&InputRules1701.Exit(wire);}
#else
  internal const int FrameCapacity=128;
  internal static bool Terminal(int index,byte[] wire){return InputRules1701.Exit(wire);}
#endif
  internal static bool Failure(''')
    raw=change(raw,'string[] markers={','string[] markers={"ORIGINAL_PARENT_SESSION_UNKNOWN_RETAINED_NO_NEXT_ATTEMPT",')
    p.write_bytes(raw)
    p=src/'RootPPrebirth1701.Input.cs';raw=p.read_bytes()
    raw=change(raw,'new InputFrame[128]','new InputFrame[InputProtocol1701.FrameCapacity]')
    raw=change(raw,'Reserved>=128','Reserved>=InputProtocol1701.FrameCapacity')
    raw=change(raw,'Attempts>=128','Attempts>=InputProtocol1701.FrameCapacity')
    raw=change(raw,'f.ExitLine=InputRules1701.Exit(wire);','f.ExitLine=InputProtocol1701.Terminal(Reserved,wire);')
    p.write_bytes(raw)
    p=src/'RootPPrebirth1682.Pure.cs';raw=p.read_bytes()
    raw=change(raw,'  internal string CommandLine() {','''#if P2_SHARED_PARENT_REFUSAL_SESSION
  internal const string ParentSelector="--c28-parent-refusal-session-p1";
#else
  internal const string ParentSelector="--c28-parent-launch-p1";
#endif
  internal string CommandLine() {''')
    raw=change(raw,'+"\\\" --c28-parent-launch-p1";','+"\\\" "+ParentSelector;')
    p.write_bytes(raw)
    p=src/'Root47Controller1725.IO.cs';raw=p.read_bytes()
    raw=change(raw,'   Need(++EventRecords<=512,"EVENT_RECORDS");','''#if P2_SHARED_PARENT_REFUSAL_SESSION
   Need(++EventRecords<=8704,"EVENT_RECORDS");
#else
   Need(++EventRecords<=512,"EVENT_RECORDS");
#endif
''')
    p.write_bytes(raw)
    # Keep the complete original Driver for default/single profiles. A distinct
    # driver retains 17 different gates, journals and file slots, never resetting
    # an old gate or input history between attempts.
    p=src/'Root47Controller1725.cs';raw=p.read_bytes()
    original=raw.decode('utf-8-sig')
    start=original.index(' internal sealed class Driver1725 {')
    driver=original[start:original.rfind('\n}')]
    old=driver
    driver=driver.replace('Namespace,JournalPath,CollectorPath','Namespace,CollectorPath')
    driver=driver.replace('readonly JournalPrefix1725 Prefix=new JournalPrefix1725();','')
    driver=driver.replace('  readonly StepGate1725 Gate=new StepGate1725(RolePins1725.ShellSha);',
        '''  readonly SharedRound042[] Rounds=new SharedRound042[17];int RoundIndex;
  SharedRound042 CurrentRound {get{return Rounds[RoundIndex];}}
  StepGate1725 Gate {get{return CurrentRound.Gate;}}
  JournalPrefix1725 Prefix {get{return CurrentRound.Prefix;}}
  string EvidencePath {get{return Namespace+"\\\\case-attempt-"+(RoundIndex+1).ToString("D2")+".evidence";}}
  string JournalPath {get{return EvidencePath+"\\\\journal.jsonl";}}
  int LineOffset {get{return RoundIndex*92;}}''')
    driver=driver.replace('  readonly FirstPromptGate2169 FirstPrompt;','  RoundPrompt042 FirstPrompt {get{return CurrentRound.Prompt;}}')
    driver=driver.replace(';JournalPath=Namespace+@"\\case.evidence\\journal.jsonl";',';')
    driver=driver.replace('   FirstPrompt=new FirstPromptGate2169(Stdout,OutLength);',
        '   if(mode=="run")for(int i=0;i<17;i++){Rounds[i]=new SharedRound042(i+1);Rounds[i].Register(Io.Resources);}')
    driver=driver.replace('Io.FreshAbsent(Namespace);Io.FreshAbsent(Namespace+@"\\case.evidence");Io.FreshAbsent(JournalPath);InitialAbsent=true;',
        'Io.FreshAbsent(Namespace);for(RoundIndex=0;RoundIndex<17;RoundIndex++){Io.FreshAbsent(EvidencePath);Io.FreshAbsent(JournalPath);}RoundIndex=0;InitialAbsent=true;')
    driver=driver.replace('   for(StepIndex=0;StepIndex<47;StepIndex++){',
        '''   for(RoundIndex=0;RoundIndex<17;RoundIndex++){
    int baseline=RoundIndex==0?0:checked((int)Last.Input.Frames[LineOffset-1].NativeWriteStartStdout);
    CurrentRound.Begin(Stdout,baseline);CriticalStart=-1;ContextStart=-1;
   for(StepIndex=0;StepIndex<47;StepIndex++){''')
    driver=driver.replace('int line=step.FirstLine;line<=step.LastLine','int line=LineOffset+step.FirstLine;line<=LineOffset+step.LastLine')
    driver=driver.replace('StepIndex!=0||line!=0||FirstPrompt.Observe','StepIndex!=0||line!=LineOffset||FirstPrompt.Observe')
    driver=driver.replace('Delivered==step.LastLine+1','Delivered==LineOffset+step.LastLine+1')
    driver=driver.replace('   Io.Need(Delivered==92&&Gate.Completed==47&&Last.Input.DeliveredBytes==InputProtocol1701.NormalBytes,"CURRENT47_TRANSPORT_OR_GATE_INCOMPLETE");',
        '''    StepIndex=46;
    while(!CurrentRound.AttemptClosed||(RoundIndex==16&&!CurrentRound.CommonOwnerClosed)){Tick();Thread.Sleep(5);}
    CurrentRound.Complete();WriteRound();
   }
   RoundIndex=16;
   Io.Need(Delivered==InputProtocol1701.Count&&Gate.Completed==47&&Last.Input.DeliveredBytes==InputProtocol1701.NormalBytes,"CURRENT17_TRANSPORT_OR_GATE_INCOMPLETE");''')
    driver=driver.replace('StepIndex==46&&Last.Input.Delivered==92','RoundIndex==16&&StepIndex==46&&Last.Input.Delivered==InputProtocol1701.Count')
    driver=driver.replace('input.Delivered<=92','input.Delivered<=InputProtocol1701.Count')
    driver=driver.replace('c.Input.Delivered==92','c.Input.Delivered==InputProtocol1701.Count')
    driver=driver.replace('StepIndex<=39','StepIndex<=40')
    driver=driver.replace('Io.Journal.', 'CurrentRound.Journal.').replace('Io.Journal,','CurrentRound.Journal,')
    driver=driver.replace('Io.EvidenceDirectory.', 'CurrentRound.Evidence.').replace('Io.EvidenceDirectory,','CurrentRound.Evidence,')
    driver=driver.replace('Namespace+@"\\case.evidence"','EvidencePath')
    driver=driver.replace('    Gate.Accept(record);','    Gate.Accept(record);CurrentRound.Accept(record);')
    driver=driver.replace('Io.Event(D("event",','Io.Event(D("attempt",RoundIndex+1,"event",')
    driver=driver.replace('"steps",47,"lines",92','"attempts",17,"steps",799,"lines",1564')
    a=driver.index('  void WriteTerminal(){');z=driver.index('  Dictionary<string,object> FinalData(){',a)
    driver=driver[:a]+(BASE/'ParentSessionReports042.cs.part').read_text()+driver[z:]
    a=driver.index('   for(int i=0;i<Final.Input.Delivered;i++){');z=driver.index('   foreach(FilePin pin in Final.OriginalFilePins)',a)
    driver=driver[:a]+'''   foreach(SharedRound042 round in Rounds)frames.Add(round.ReportSummary);
'''+driver[z:]
    driver=driver.replace('"native_thread_signaled",Final.Input.NativeThreadSignaled,"frames",frames',
        '"native_thread_signaled",Final.Input.NativeThreadSignaled,"round_reports",frames')
    assert 'Io.Journal.' not in driver and 'case.evidence' not in driver and 'Delivered==92' not in driver
    assert driver.count('WriteRound();')==1 and driver.count('CurrentRound.Accept(record)')==1
    raw=(original[:start]+'#if !'+SYMBOL+'\n'+old+'\n#else\n'+driver+'\n#endif\n}\n').encode()
    p.write_bytes(raw)
    for name in ('ParentSession042.cs','ParentSessionPure042.cs'):
        (src/name).write_bytes((BASE/name).read_bytes())
    # The new prompt guard uses a unique immutable lower bound supplied by the
    # previous exit's ORIGINAL native-write start, so it cannot reuse round one's
    # identical first prompt and does not miss an already arrived next prompt.
    raw=(src/'Root47FirstPrompt2169.cs').read_bytes()
    raw=raw.replace(b'FirstPromptGate2169',b'RoundPrompt042')
    a=raw.index(b'   if(original==null');z=raw.index(b'   string expected=',a)
    raw=raw[:a]+b'''   if(original==null||original.Length!=StdoutCapacity||initialLength<0||initialLength>original.Length)throw new InvalidOperationException("ROUND_PROMPT_ORIGINAL_BOUND");
   Through=initialLength;LineStart=initialLength;Candidate=initialLength==0||original[initialLength-1]==10;
'''+raw[z:]
    (src/'RoundPrompt042.cs').write_bytes(b'#if P2_SHARED_PARENT_REFUSAL_SESSION\n'+raw+b'\n#endif\n')
    result=dict(schema='FIXED_SEVENTEEN_PARENT_SESSION_CONTROLLER_V1',symbol=SYMBOL,
        original_sources=originals,derived_sources={p.name:pin(p) for p in src.glob('*.cs')},
        original47_preserved=True,single_refusal_profile_preserved=True,
        attempts=17,steps=799,lines=1564,wire_bytes=60469,append_only_input_history=True,
        round_report_cap=131072,final_report_cap=131072,internal_product_IPC_changed=False,
        runtime='NOT_RUN',VM_calls=0,source_accepted=False,release_eligible=False)
    with (out/'parent-session-controller-derivation.json').open('x',encoding='utf-8') as f:json.dump(result,f,indent=2)
    return result
