// SOURCE1725 controller resources. No managed owning wrappers/finalizers,
// PID lookup, process creation, network, credential input, ACL mutation or cleanup.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using N=C28.RootP1682.RootPPrebirth.Native;
namespace C28.RootP1682 {
 internal static class RolePins1725 {
  internal static readonly string[] Sha={"6ae6a195d1c5e66a3b13ed9c12ac5602574cb315f14da41914924502dcdfdd35","42852d98c3d3f1ca38a7fd0c2d4f40728444cdf8f19ffed70bda6a9e4b085120","5b4a058439475516b34240014086e6510e28c846c1a2eb3bf1f8dc11f4b9d83e","5593463e5c97ec23c1db7dca6ac728fcb29a9180a38acb38bf1ac0b35e37d4c2"};
  internal static readonly ulong[] Bytes={30112768,108230656,27000320,17356288};
  // Filled only from ROOT's exact public current-system-shell byte pin before delivery.
  internal const string ShellSha=FileMetadataRules1718.SystemShellSha;
  internal const ulong ShellBytes=FileMetadataRules1718.SystemShellBytes;
  internal static void Check(RootSelection selection){
   if(!Rules.Hex(ShellSha,64))throw new InvalidOperationException("ROOT_FULL_SHELL_PIN_UNBOUND");
   selection.Validate();
   for(int i=0;i<5;i++){
    string expected=i==4?ShellSha:Sha[i];ulong bytes=i==4?ShellBytes:Bytes[i];
    string path=i==4?RootPPrebirth.Shell:selection.Install+"\\"+RootPPrebirth.RoleNames[i];FilePin found=null;
    foreach(FilePin p in selection.Pins)if(p.Path==path)found=p;
    if(found==null||found.Sha256!=expected||found.Bytes!=bytes)throw new InvalidOperationException("ACTUAL1693_ROLE_PIN");
   }
  }
 }
 internal sealed class FileImage1725 {
  internal FilePin First,Last;internal readonly Slot Handle;internal uint? OpenError,CloseError;internal uint LengthReturn;
  internal bool? OpenReturned;
  internal int OpenAttempts,KnownAbsenceProbes;
  internal FileImage1725(string name){Handle=new Slot(name);}
 }
 internal sealed class Io1725 {
  internal string Stage="NOT_STARTED";internal int TargetRole=-1;
  string NativeApi,NativeStage,FailureStage;int NativeRole=-1,FailureRole=-1;
  bool? NativeReturned;int? NativeError;uint? NativeValue;
  Dictionary<string,object> FailureNative;
  readonly List<object> ExceptionData=new List<object>(4);bool ExceptionCaptured,ExceptionTruncated;
  internal static string SafeLabel(string text,bool type){
   if(text==null)return null;if(text.Length<1||text.Length>120)return "REDACTED";
   foreach(char c in text)if(!((c>='A'&&c<='Z')||(c>='0'&&c<='9')||c=='_'||(type&&((c>='a'&&c<='z')||c=='`'))))return "REDACTED";
   return text;
  }
  internal void At(string stage,string path){
   Stage=stage;TargetRole=-1;
   if(FileMetadataRules1718.SystemShellPath(path)){TargetRole=4;return;}
   for(int i=0;i<RootPPrebirth.RoleNames.Length;i++)if(path!=null&&path.EndsWith("\\"+RootPPrebirth.RoleNames[i],StringComparison.OrdinalIgnoreCase)){TargetRole=i;break;}
  }
  internal void NativeBegin(string api){NativeApi=api;NativeStage=Stage;NativeRole=TargetRole;NativeReturned=null;NativeError=null;NativeValue=null;}
  internal void NativeResult(bool returned,int error){NativeReturned=returned;NativeError=error;}
  internal void NativeNumber(uint value,int? error){NativeValue=value;NativeError=error;}
  internal void CaptureException(Exception error){
   if(ExceptionCaptured)return;ExceptionCaptured=true;
   for(int depth=0;error!=null&&depth<4;depth++){
    ExceptionData.Add(new Dictionary<string,object>{{"type",SafeLabel(error.GetType().Name,true)},{"hresult",error.HResult}});
    error=error.InnerException;
   }
   ExceptionTruncated=error!=null;
  }
  internal Dictionary<string,object> DiagnosticData(string mode){
   return new Dictionary<string,object>{{"schema","ROOT_CACHED_FAILURE1832_V1"},{"mode",mode=="gather"||mode=="run"?mode:"REDACTED"},
    {"unknown",Unknown},{"no_replay",true},{"first_guard",FirstError=="ROOT_CONTROLLER_UNCERTAINTY_RETAINED"?null:SafeLabel(FirstError,false)},
    {"retention_guard",FirstError=="ROOT_CONTROLLER_UNCERTAINTY_RETAINED"?FirstError:null},{"source_stage",SafeLabel(FailureStage,false)},
    {"target_role_index",FailureRole},{"last_native",FailureNative},{"exception_captured",ExceptionCaptured},
    {"exceptions",ExceptionData.ToArray()},{"exception_chain_truncated",ExceptionTruncated},{"exception_body","NOT_EXPORTED"},
    {"resource_release_or_native_exit_proven",false}};
  }
  internal readonly Stopwatch Clock=new Stopwatch();internal uint Creator;internal string FirstError;internal bool Unknown;
  internal readonly FileImage1725 Root=new FileImage1725("C_ROOT"),Binding=new FileImage1725("BINDING"),Collector=new FileImage1725("COLLECTOR"),
   Events=new FileImage1725("EVENTS"),Terminal=new FileImage1725("TERMINAL"),Gather=new FileImage1725("GATHERED_BINDING"),
   JournalNamespace=new FileImage1725("JOURNAL_NAMESPACE"),EvidenceDirectory=new FileImage1725("EVIDENCE_DIRECTORY"),Journal=new FileImage1725("JOURNAL");
  internal readonly FileImage1725[] Pins=new FileImage1725[40];
  internal readonly byte[] Scratch=new byte[65536],BindingBytes=new byte[131072],OriginalWriteBuffer=new byte[131073];
  internal bool WriteInFlight;internal int OriginalWriteLength;
  internal uint LastRead,LastWrite;internal bool? LastReadReturned,LastWriteReturned;internal int LastReadError,LastWriteError;
  internal int OpenJournalAttempts,EventRecords;internal long EventBytes;internal readonly List<FileImage1725> Resources;
  internal Io1725(){
   Resources=new List<FileImage1725>(49);Resources.AddRange(new FileImage1725[]{Root,Binding,Collector,Events,Terminal,Gather,JournalNamespace,EvidenceDirectory,Journal});
   for(int i=0;i<40;i++){Pins[i]=new FileImage1725("GATHER_PIN_"+i);Resources.Add(Pins[i]);}
  }
  internal void Fail(string code){
   Unknown=true;if(FirstError!=null)return;FirstError=code;
   try{FailureStage=Stage;FailureRole=TargetRole;FailureNative=new Dictionary<string,object>{
    {"api",SafeLabel(NativeApi,false)},{"source_stage",SafeLabel(NativeStage,false)},{"target_role_index",NativeRole},
    {"returned",NativeReturned},{"value_returned",NativeValue},{"error",NativeError}};}catch{}
  }
  internal void Need(bool condition,string code){if(!condition){Fail(code);throw new InvalidOperationException(code);}}
  internal void Begin(){Clock.Start();Creator=N.GetCurrentThreadId();Check();}
  internal void Check(){Need(!Unknown&&Clock.ElapsedMilliseconds<5500000&&N.GetCurrentThreadId()==Creator,"CONTROLLER_THREAD_OR_DEADLINE");}
  void Unique(FileImage1725 item){
   foreach(FileImage1725 other in Resources)if(!Object.ReferenceEquals(item,other)&&other.Handle.State==ResourceState.Open&&other.Handle.Raw==item.Handle.Raw)
    Need(false,"CONTROLLER_ALIAS_COLLISION");
  }
  internal bool Open(FileImage1725 item,string path,bool directory,uint access,uint share,uint disposition,bool notYet) {
   At("FILE_OPEN",path);
   Check();Need(FileMetadataRules1718.ReadOnlyShellOpen(path,directory,access,share,disposition,false),"READONLY_FIXED_SHELL_OPEN");Slot slot=item.Handle;
   Need(slot.State==ResourceState.Empty&&++item.OpenAttempts<=1200000,"ORIGINAL_OPEN_REUSE_OR_PROBE_CAP");slot.Begin();
   N.SA sa=new N.SA();sa.Length=Marshal.SizeOf(typeof(N.SA));
   NativeBegin("CREATE_FILE");IntPtr raw=N.CreateFileW(path,access,share,ref sa,disposition,0x00200000u|(directory?0x02000000u:0u),IntPtr.Zero);
   int error=raw==IntPtr.Zero||raw==new IntPtr(-1)?Marshal.GetLastWin32Error():0;item.OpenError=(uint)error;
   NativeResult(error==0,error);item.OpenReturned=error==0;slot.Capture(raw,error==0,error);
   if(notYet&&error!=0&&(error==2||error==3)&&slot.State==ResourceState.Rejected){
    // No handle returned. This single reserved probe may be tried again only
    // for known absence; no real allocation/Unknown is replayed.
    item.KnownAbsenceProbes++;slot.Raw=raw;slot.State=ResourceState.Empty;return false;
   }
   Need(slot.State==ResourceState.Open,"CONTROLLER_FILE_OPEN");Unique(item);
   item.First=Identity(item,path,directory);Check();return true;
  }
  internal FilePin Identity(FileImage1725 item,string expected,bool directory) {
   At("FILE_IDENTITY_BASIC",expected);Check();N.BYINFO b;N.IDINFO id;
   NativeBegin("FILE_BASIC");bool basic=N.GetFileInformationByHandle(item.Handle.Raw,out b);int basicError=basic?0:Marshal.GetLastWin32Error();
   NativeResult(basic,basicError);Need(basic,"CONTROLLER_FILE_BASIC");
   At("FILE_IDENTITY_ID",expected);NativeBegin("FILE_ID");bool identity=N.GetFileInformationByHandleEx(item.Handle.Raw,18,out id,24);int idError=identity?0:Marshal.GetLastWin32Error();
   NativeResult(identity,idError);Need(identity,"CONTROLLER_FILE_ID");
   At("FILE_IDENTITY_PATH",expected);NativeBegin("FINAL_PATH");
   StringBuilder actual=new StringBuilder(2048);uint n=N.GetFinalPathNameByHandleW(item.Handle.Raw,actual,2048,0);int? pathError=n==0?(int?)Marshal.GetLastWin32Error():null;
   NativeNumber(n,pathError);item.LengthReturn=n;
   string path=actual.ToString();if(path.StartsWith(@"\\?\",StringComparison.Ordinal))path=path.Substring(4);
   FilePin observation=new FilePin(path,(b.Attributes&16)!=0,id.Volume,Rules.HexBytes(id.Id),((ulong)b.SizeHigh<<32)|b.SizeLow,
    b.Created.Value,b.Modified.Value,b.Attributes,b.Links,null);item.Last=observation;
   Need(n>4&&n<2048&&actual.ToString().StartsWith(@"\\?\",StringComparison.Ordinal)&&String.Equals(path,expected,StringComparison.OrdinalIgnoreCase)
    &&observation.Directory==directory&&(b.Attributes&0x400)==0&&FileMetadataRules1718.ObservedLinks(observation)&&id.Volume!=0&&observation.CreationFileTime!=0&&Rules.Hex(observation.FileId,32),"CONTROLLER_PLAIN_HELD_PATH");
   return observation;
  }
  internal void Close(FileImage1725 item) {
   At("FILE_CLOSE",item.First==null?null:item.First.Path);
   Check();item.Handle.BeginClose();NativeBegin("CLOSE_HANDLE");bool ok=N.CloseHandle(item.Handle.Raw);int error=ok?0:Marshal.GetLastWin32Error();NativeResult(ok,error);item.CloseError=(uint)error;
   Need(item.Handle.Closed(ok,error),"CONTROLLER_CHECKED_CLOSE_NO_RETRY");Check();
  }
  internal void Seek(FileImage1725 file,long at){
   At("FILE_SEEK",file.First==null?null:file.First.Path);
   Check();long returned;NativeBegin("SEEK_FILE");bool ok=N.SetFilePointerEx(file.Handle.Raw,at,out returned,0);int error=ok?0:Marshal.GetLastWin32Error();NativeResult(ok,error);
   Need(ok&&returned==at,"CONTROLLER_ORIGINAL_SEEK");
  }
  internal int Read(FileImage1725 file,byte[] buffer,int wanted) {
   At("FILE_READ",file.First==null?null:file.First.Path);
   Check();Need(wanted>=0&&wanted<=buffer.Length,"CONTROLLER_READ_CAP");LastRead=0;LastReadReturned=null;
   NativeBegin("READ_FILE");bool ok=N.ReadFile(file.Handle.Raw,buffer,(uint)wanted,ref LastRead,IntPtr.Zero);
   LastReadError=ok?0:Marshal.GetLastWin32Error();LastReadReturned=ok;NativeResult(ok,LastReadError);
   Need(ok&&LastRead<=(uint)wanted,"CONTROLLER_ORIGINAL_READ");Check();return (int)LastRead;
  }
  internal void Write(FileImage1725 file,byte[] bytes,int cap){
   At("FILE_WRITE",file.First==null?null:file.First.Path);
   Check();Need(!WriteInFlight&&bytes.Length<=cap&&bytes.Length<=OriginalWriteBuffer.Length,"CONTROLLER_WRITE_CAP");
   Buffer.BlockCopy(bytes,0,OriginalWriteBuffer,0,bytes.Length);OriginalWriteLength=bytes.Length;WriteInFlight=true;LastWrite=0;LastWriteReturned=null;
   NativeBegin("WRITE_FILE");bool ok=N.WriteFile(file.Handle.Raw,OriginalWriteBuffer,(uint)OriginalWriteLength,ref LastWrite,IntPtr.Zero);
   LastWriteError=ok?0:Marshal.GetLastWin32Error();LastWriteReturned=ok;NativeResult(ok,LastWriteError);
   Need(ok&&LastWrite==(uint)OriginalWriteLength,"CONTROLLER_WRITE_PARTIAL_NO_APPEND");WriteInFlight=false;Check();
  }
  internal string Hash(FileImage1725 file,ulong count,bool pe) {
   At("FILE_HASH",file.First==null?null:file.First.Path);
   Need(count<=536870912,"CONTROLLER_HASH_CAP");Seek(file,0);
   if(pe){
    Need(Read(file,Scratch,64)==64&&Scratch[0]==77&&Scratch[1]==90,"CONTROLLER_DOS");
    uint offset=BitConverter.ToUInt32(Scratch,60);Need(offset>=64&&(ulong)offset+26<=count,"CONTROLLER_PE_OFFSET");Seek(file,offset);
    Need(Read(file,Scratch,26)==26&&Scratch[0]==80&&Scratch[1]==69&&Scratch[2]==0&&Scratch[3]==0&&Scratch[4]==0x64&&Scratch[5]==0x86&&Scratch[24]==11&&Scratch[25]==2,"CONTROLLER_AMD64_PE");
   }
   Seek(file,0);ulong total=0;
   using(SHA256Managed sha=new SHA256Managed()){
    for(;;){int n=Read(file,Scratch,Scratch.Length);if(n==0)break;total+=(uint)n;Need(total<=count,"CONTROLLER_HASH_LENGTH");sha.TransformBlock(Scratch,0,n,Scratch,0);}
    Need(total==count,"CONTROLLER_HASH_EOF");sha.TransformFinalBlock(new byte[0],0,0);return Rules.HexBytes(sha.Hash);
   }
  }
  internal void FreshAbsent(string path){
   Check();uint attrs=ExtraNative.GetFileAttributesW(path);int error=attrs==0xffffffff?Marshal.GetLastWin32Error():0;
   Need(attrs==0xffffffff&&(error==2||error==3),"FRESH_CASE_PATH_NOT_ABSENT");
  }
  internal void CreateCollector(string path){
   Check();Need(N.CreateDirectoryW(path,IntPtr.Zero),"FRESH_COLLECTOR_CREATENEW");
   Open(Collector,path,true,0x80000000,1,3,false);
   Open(Events,path+@"\events.jsonl",false,0x40000000,1,1,false);
  }
  internal void Event(Dictionary<string,object> data){
#if P2_SHARED_PARENT_REFUSAL_SESSION
   Need(++EventRecords<=8704,"EVENT_RECORDS");
#else
   Need(++EventRecords<=512,"EVENT_RECORDS");
#endif
byte[] raw=new UTF8Encoding(false,true).GetBytes(Json1725.Encode(data)+"\n");
   Need(EventBytes+raw.Length<=4194304,"EVENT_TOTAL_BYTES");Write(Events,raw,131073);EventBytes+=raw.Length;
   Need(N.FlushFileBuffers(Events.Handle.Raw),"EVENT_FLUSH");
  }
  internal void NormalCloseAll(){
   // This method is invoked ONLY after actual helper normal closure (run) or
   // completed readonly gather. No finally/Dispose calls it on uncertainty.
   for(int i=Resources.Count-1;i>=0;i--)if(Resources[i].Handle.State==ResourceState.Open)Close(Resources[i]);
  }
  internal List<object> ResourceData(){
   List<object> list=new List<object>();
   foreach(FileImage1725 f in Resources)list.Add(new Dictionary<string,object>{{"name",f.Handle.Name},{"state",f.Handle.State.ToString()},
    {"open_error",f.OpenError.HasValue?(object)f.OpenError.Value:null},{"close_error",f.CloseError.HasValue?(object)f.CloseError.Value:null},
    {"open_attempts",f.OpenAttempts},{"known_absence_probes",f.KnownAbsenceProbes},
    {"last_open_returned",f.OpenReturned},{"last_operation_returned",f.Handle.Returned},{"last_operation_error",f.Handle.LastError}});
   return list;
  }
  internal static class ExtraNative {
   [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)]internal static extern uint GetFileAttributesW(string path);
  }
 }
}
