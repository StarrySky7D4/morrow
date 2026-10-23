// SOURCE ONLY; ROOT must compile/review and bind before invoking.
// All unmanaged values belong to one permanently rooted, at-most-once owner.
// No finalizer/SafeHandle/Process/Thread.Join/kill/PID reopen/token mutation.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Threading;

namespace C28.RootP1682 {
 public static partial class RootPPrebirth {
  internal static readonly string[] RoleNames={"morrow-workbench-supervisor.exe","morrow-protected-checked-vm-harness.exe","codex-command-runner.exe","codex-windows-sandbox-setup.exe"};
  internal const string Shell=@"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe";
  static Owner retained;static int attempted;
  // Invalid/default selection is rejected before installing an owner or native effect.
  // This method never releases an unresolved owner. ROOT MUST retain its host/module
  // after UNKNOWN; do not exit/unload/dispose that controller or replay this method.
  public static CaptureObservation Capture(RootSelection selection) {
   if(Volatile.Read(ref attempted)!=0)return new CaptureObservation{Status="NO_REPLAY_78",FirstError="ALREADY_ATTEMPTED",NoReplay=true};
   try { if(selection==null)throw new InvalidOperationException();selection.Validate(); }
   catch { return new CaptureObservation{Status="UNBOUND_NOT_EXECUTABLE_78",FirstError="ROOT_SELECTION_INVALID",NoReplay=false}; }
   return new CaptureObservation{Status="CONTROLLED_INPUT_REQUIRED_78",FirstError="USE_BEGIN_CONTROLLED1701",NoReplay=false};
  }
  // Explicit keeper for a ROOT controller that received UNKNOWN. No native queries,
  // cleanup, reconnect, replay or alternative handle lookup. ROOT decides its use.
  public static void PreserveUnknownHost() {
   if(retained==null||Volatile.Read(ref retained.Unknown)==0)throw new InvalidOperationException("NO_UNKNOWN_OWNER");
   for(;;)Thread.Sleep(1000);
  }
  internal sealed class Blob {
   internal readonly byte[] Bytes;internal readonly long Base;
   internal Blob(byte[] bytes,long address){Bytes=bytes;Base=address;}
   internal byte[] PointerSid(int field,int minimum) {
    long offset=BitConverter.ToInt64(Bytes,field)-Base;
    if(offset<minimum||offset>Bytes.Length-8||(offset&3)!=0)throw new InvalidOperationException("TOKEN_SID_POINTER");
    return Rules.Sid(Bytes,(int)offset);
   }
  }
  internal sealed class Subject {
   internal byte[] Stats,User,Owner,Logon,Acl,Body;
   internal bool Same(Subject b) { return b!=null&&Rules.Equal(Stats,b.Stats)&&Rules.Equal(Body,b.Body); }
  }
  internal sealed class Reader {
   internal readonly Slot Pipe,Sink,Thread;internal readonly byte[] Buffer=new byte[4096];
   internal readonly byte[] Evidence=new byte[2097152];internal long Published;
   internal readonly Native.ThreadStart Callback;
   internal int Eof,Finished;internal uint Id;internal ulong? Created;internal uint? Exit;
   internal long Bytes;internal string Hash;internal uint LastReadCount,LastWriteCount;internal bool? LastReadReturned,LastWriteReturned;internal int LastReadError,LastWriteError;
   internal Reader(Owner owner,string prefix) {
    Pipe=owner.NewSlot(prefix+"_READ");Sink=owner.NewSlot(prefix+"_LOG");Thread=owner.NewSlot(prefix+"_NATIVE_THREAD");
    Callback=delegate(IntPtr unused){return owner.Drain(this);};
   }
  }
  internal sealed partial class Owner {
   internal readonly RootSelection Plan;internal readonly List<Slot> Slots=new List<Slot>(65);
   readonly Slot[] Pins;readonly FilePin[] Actual,FinalPins;
   readonly Slot InputRead,InputWrite,OutWrite,ErrWrite,Process,Primary,Token0,Token1,Token2,ChildToken;
   readonly Slot[] NoThread;readonly Slot Scratch,AttributeMemory,HandleListMemory,EnvironmentMemory,OutputDirectory;
   readonly DwordQuery ProcessQuery=new DwordQuery(),PrimaryQuery=new DwordQuery(),OutQuery=new DwordQuery(),ErrQuery=new DwordQuery();
   internal readonly Reader Out,Err;readonly Stopwatch Clock=new Stopwatch();
   internal Native.PI Pi; // original outputs live HERE, even if native return is false or managed unwind occurs
   internal int Unknown;internal string First;
   readonly Completion Facts=new Completion();
   internal uint CallingThread;bool CreateStarted,CreateReturned,CreateSuccess,ResumeStarted,AttributeInitialized,NamespaceAttempted,NamespaceCreated;
   uint ResumeReturn;int CreateError;string OutputRoot;Subject Initial;
   internal Owner(RootSelection plan) {
    Plan=plan;Pins=new Slot[plan.Pins.Length];Actual=new FilePin[plan.Pins.Length];FinalPins=new FilePin[plan.Pins.Length];
    for(int i=0;i<Pins.Length;i++)Pins[i]=NewSlot("PIN_"+i);
    InputRead=NewSlot("STDIN_READ");InputWrite=NewSlot("STDIN_WRITE");
    OutWrite=NewSlot("STDOUT_WRITE");ErrWrite=NewSlot("STDERR_WRITE");
    Process=NewSlot("P_ORIGINAL_PROCESS");Primary=NewSlot("P_ORIGINAL_PRIMARY_THREAD");
    Token0=NewSlot("CREATOR_PRIMARY_INITIAL");Token1=NewSlot("CREATOR_PRIMARY_PRECREATE");Token2=NewSlot("CREATOR_PRIMARY_POSTCREATE");
    ChildToken=NewSlot("P_PRIMARY_OBSERVATION");
    NoThread=new Slot[]{NewSlot("NO_THREAD_TOKEN_INITIAL"),NewSlot("NO_THREAD_TOKEN_PRECREATE"),NewSlot("NO_THREAD_TOKEN_POSTCREATE")};
    Scratch=NewSlot("QUERY_MEMORY");AttributeMemory=NewSlot("ATTRIBUTE_MEMORY");HandleListMemory=NewSlot("HANDLE_LIST_MEMORY");
    EnvironmentMemory=NewSlot("ENVIRONMENT_MEMORY");
    OutputDirectory=NewSlot("OUTPUT_DIRECTORY");
    Out=new Reader(this,"STDOUT");Err=new Reader(this,"STDERR");Input=new InputPump1701(this);
    OutputRoot=@"C:\Morrow-C28-RootP1701-"+plan.Tag;
    if(Slots.Count>65)throw new InvalidOperationException("SLOT_LIMIT");
   }
   internal Slot NewSlot(string name){Slot slot=new Slot(name);Slots.Add(slot);return slot;}
   internal void Fail(string why){Interlocked.CompareExchange(ref First,why,null);Interlocked.Exchange(ref Unknown,1);}
   void Need(bool value,string why){if(!value){Fail(why);throw new InvalidOperationException(why);}}
   internal void Check() {
    Need(Volatile.Read(ref Unknown)==0,"UNKNOWN_NO_FURTHER_OPERATIONS");
    Need(Rules.Budget(Clock.ElapsedMilliseconds,Plan.DeadlineMs),"OBSERVER_DEADLINE_NO_REPLAY");
    Need(Native.GetCurrentThreadId()==CallingThread,"CREATING_THREAD_CHANGED");
   }
   void Unique(Slot captured) {
    if(captured.State!=ResourceState.Open)return;
    foreach(Slot slot in Slots)if(!Object.ReferenceEquals(slot,captured)&&slot.State==ResourceState.Open&&slot.Raw==captured.Raw)
     Need(false,"UNEXPECTED_LIVE_ALIAS_COLLISION");
   }
   void Accept(Slot slot,IntPtr raw,bool success,int error) {
    slot.Capture(raw,success,error);Unique(slot);Need(slot.State==ResourceState.Open,"ORIGINAL_ALLOCATION_FAILED_OR_PARTIAL");
   }
   void Close(Slot slot) {
    Check();slot.BeginClose();bool ok=Native.CloseHandle(slot.Raw);int error=ok?0:Marshal.GetLastWin32Error();
    Need(slot.Closed(ok,error),"CHECKED_CLOSE_FAILED_NO_RETRY");Check();
   }
   void Memory(Slot slot,int bytes) {
    Check();Need(bytes>0&&bytes<=65536,"MEMORY_CAP");slot.Begin();
    IntPtr raw=Marshal.AllocHGlobal(bytes);Accept(slot,raw,raw!=IntPtr.Zero,0);
   }
   void Free(Slot slot) {
    Check();Need(slot.State==ResourceState.Open,"MEMORY_STATE");
    slot.State=ResourceState.CloseInFlight;Marshal.FreeHGlobal(slot.Raw);slot.State=ResourceState.MemoryRetired;Check();
   }
   void NoImpersonation(Slot slot) {
    Check();slot.Begin();bool ok=Native.OpenThreadToken(Native.GetCurrentThread(),8,true,ref slot.Raw);
    int error=ok?0:Marshal.GetLastWin32Error();
    try{slot.Absent(ok,error);}catch{Fail("ACTUAL_THREAD_TOKEN_NOT_ABSENT");throw;}
   }
   void OpenToken(Slot slot,IntPtr process) {
    Check();slot.Begin();bool ok=Native.OpenProcessToken(process,8,ref slot.Raw);int error=ok?0:Marshal.GetLastWin32Error();
    Accept(slot,slot.Raw,ok,error);
   }
   Blob Token(IntPtr token,int kind) {
    uint capacity=kind==10?56u:(kind==12?4u:65536u);
    Check();uint n=0;bool ok=Native.GetTokenInformation(token,kind,Scratch.Raw,capacity,ref n);
    int error=ok?0:Marshal.GetLastWin32Error();Need(ok&&n>0&&n<=capacity,"TOKEN_QUERY_BOUNDED");
    byte[] bytes=new byte[n];Marshal.Copy(Scratch.Raw,bytes,0,(int)n);Check();
    return new Blob(bytes,Scratch.Raw.ToInt64());
   }
   byte[] Statistics(IntPtr token) {
    byte[] b=Token(token,10).Bytes;Need(b.Length==56&&Rules.U32(b,24)==1,"PRIMARY_TOKEN_STATISTICS");
    List<byte> key=new List<byte>();key.AddRange(Rules.Slice(b,0,24));key.AddRange(Rules.Slice(b,24,4));key.AddRange(Rules.Slice(b,48,8));return key.ToArray();
   }
   Subject ReadSubject(IntPtr token) {
    Subject s=new Subject();s.Stats=Statistics(token);
    Blob user=Token(token,1);s.User=user.PointerSid(0,16);
    Blob groups=Token(token,2);uint count=Rules.U32(groups.Bytes,0);Need(count<=256&&8L+count*16L<=groups.Bytes.Length,"TOKEN_GROUP_BOUND");
    List<byte> body=new List<byte>();AddField(body,s.User);body.AddRange(BitConverter.GetBytes(count));
    for(int i=0;i<(int)count;i++){
     byte[] sid=groups.PointerSid(8+i*16,(int)(8+count*16));uint flags=Rules.U32(groups.Bytes,16+i*16);
     body.AddRange(BitConverter.GetBytes(flags));AddField(body,sid);
     if(Rules.Logon(sid)||(flags&0xc0000000)!=0){
      Need(s.Logon==null&&Rules.Logon(sid)&&(flags&0xc0000004)==0xc0000004&&(flags&16)==0,"ACTUAL_FRESH_LOGON_GROUP");
      s.Logon=sid;
     }
    }
    Need(s.Logon!=null,"LOGON_GROUP_REQUIRED");
    Blob owner=Token(token,4);s.Owner=owner.PointerSid(0,8);
    Blob defaults=Token(token,6);long offset=BitConverter.ToInt64(defaults.Bytes,0)-defaults.Base;
    Need(offset>=8&&offset<=defaults.Bytes.Length-8&&(offset&3)==0,"NULL_OR_UNBOUNDED_DEFAULT_DACL");
    int aclBytes=BitConverter.ToUInt16(defaults.Bytes,(int)offset+2);s.Acl=Rules.Slice(defaults.Bytes,(int)offset,aclBytes);
    Need(Rules.SafeDefault(s.Acl,s.User,s.Owner,s.Logon),"UNSAFE_TOKEN_OWNER_OR_DEFAULT_DACL");
    Blob integrity=Token(token,25);byte[] il=integrity.PointerSid(0,16);
    Need(Rules.Equal(il,new byte[]{1,1,0,0,0,0,0,16,0,48,0,0}),"ACTUAL_HIGH_PRIMARY_TOKEN");
    byte[] session=Token(token,12).Bytes;Need(session.Length==4,"SESSION_LENGTH");
    byte[] privileges=Token(token,3).Bytes;uint pc=Rules.U32(privileges,0);Need(pc<=64&&privileges.Length==4+pc*12,"PRIVILEGE_BOUND");
    AddField(body,s.Owner);AddField(body,s.Logon);AddField(body,s.Acl);AddField(body,il);AddField(body,session);AddField(body,privileges);
    s.Body=body.ToArray();Need(Rules.Equal(s.Stats,Statistics(token)),"PRIMARY_CHANGED_DURING_QUERY");return s;
   }
   static void AddField(List<byte> output,byte[] value){output.AddRange(BitConverter.GetBytes(value.Length));output.AddRange(value);}
   FilePin Identity(IntPtr handle,string path,bool directory,string digest,int finalIndex=-1) {
    Check();Native.BYINFO b;Native.IDINFO id;
    Need(Native.GetFileInformationByHandle(handle,out b),"FILE_BASIC_INFORMATION");
    Need(Native.GetFileInformationByHandleEx(handle,18,out id,24),"FILE_ID_INFORMATION");
    // Record native metadata before final-path/admission checks. Null Path
    // means path not observed yet; it is never substituted with the expected path.
    FilePin observed=new FilePin(null,(b.Attributes&16)!=0,id.Volume,Rules.HexBytes(id.Id),((ulong)b.SizeHigh<<32)|b.SizeLow,
     b.Created.Value,b.Modified.Value,b.Attributes,b.Links,digest);
    if(finalIndex>=0)FinalPins[finalIndex]=observed;
    StringBuilder actual=new StringBuilder(2048);uint length=Native.GetFinalPathNameByHandleW(handle,actual,2048,0);
    string returnedPath=actual.ToString();
    string actualPath=returnedPath.StartsWith(@"\\?\",StringComparison.Ordinal)?returnedPath.Substring(4):returnedPath;
    observed=new FilePin(actualPath,observed.Directory,observed.Volume,observed.FileId,observed.Bytes,
     observed.CreationFileTime,observed.ModifiedFileTime,observed.Attributes,observed.Links,digest);
    if(finalIndex>=0)FinalPins[finalIndex]=observed; // SAME owner, before comparison or close
    Need(length>4&&length<2048&&returnedPath.StartsWith(@"\\?\",StringComparison.Ordinal)
     &&String.Equals(actualPath,path,StringComparison.OrdinalIgnoreCase),"HELD_PATH_IDENTITY");
    Need((b.Attributes&0x400)==0&&((b.Attributes&16)!=0)==directory&&FileMetadataRules1718.ObservedLinks(observed),"NO_REPARSE_SINGLE_LINK");
    return observed;
   }
   void SameFile(FilePin a,FilePin b) {
    Need(FileMetadataRules1718.Same(a,b),"HELD_FILE_IDENTITY_CHANGED");
   }
   void OpenFile(Slot slot,string path,uint access,uint disposition,bool directory,bool inherit) {
    Check();Need(FileMetadataRules1718.ReadOnlyShellOpen(path,directory,access,1,disposition,inherit),"READONLY_FIXED_SHELL_OPEN");
    slot.Begin();Native.SA sa=new Native.SA();sa.Length=Marshal.SizeOf(typeof(Native.SA));sa.Inherit=inherit?1:0;
    IntPtr raw=Native.CreateFileW(path,access,1,ref sa,disposition,0x00200000u|(directory?0x02000000u:0u),IntPtr.Zero);
    int error=raw==new IntPtr(-1)||raw==IntPtr.Zero?Marshal.GetLastWin32Error():0;
    Accept(slot,raw,raw!=IntPtr.Zero&&raw!=new IntPtr(-1),error);
   }
   void Seek(IntPtr h,long position){Check();long actual;Need(Native.SetFilePointerEx(h,position,out actual,0)&&actual==position,"ORIGINAL_FILE_SEEK");}
   byte[] ReadExact(IntPtr h,int length) {
    byte[] b=new byte[length];uint n=0;Check();Need(Native.ReadFile(h,b,(uint)length,ref n,IntPtr.Zero)&&n==(uint)length,"PE_HEADER_READ");return b;
   }
   string Hash(IntPtr h,ulong length,bool pe) {
    Check();Need(length<=536870912,"HASH_CAP");Seek(h,0);
    if(pe){
     byte[] dos=ReadExact(h,64);Need(dos[0]==77&&dos[1]==90,"DOS_HEADER");
     uint offset=Rules.U32(dos,60);Need(offset>=64&&(ulong)offset+26<=length,"PE_OFFSET");
     Seek(h,offset);byte[] header=ReadExact(h,26);
     Need(header[0]==80&&header[1]==69&&header[2]==0&&header[3]==0&&header[4]==0x64&&header[5]==0x86&&header[24]==11&&header[25]==2,"AMD64_PE32_PLUS");
    }
    Seek(h,0);ulong total=0;byte[] buffer=new byte[65536];
    using(SHA256Managed digest=new SHA256Managed()){
     for(;;){
      Check();uint n=0;bool ok=Native.ReadFile(h,buffer,(uint)buffer.Length,ref n,IntPtr.Zero);
      Need(ok&&n<=buffer.Length,"HELD_FILE_HASH_READ");if(n==0)break;
      total+=n;Need(total<=length,"HASH_EXACT_LENGTH");digest.TransformBlock(buffer,0,(int)n,buffer,0);
     }
     Need(total==length,"HASH_EOF_LENGTH");digest.TransformFinalBlock(new byte[0],0,0);Check();return Rules.HexBytes(digest.Hash);
    }
   }
   void PinAll() {
    for(int i=0;i<Pins.Length;i++){
     FilePin expected=Plan.Pins[i];OpenFile(Pins[i],expected.Path,0x80000000,3,expected.Directory,false);
     FilePin before=Identity(Pins[i].Raw,expected.Path,expected.Directory,null);SameFile(before,expected);
     if(expected.Directory&&expected.Path==Plan.Install)InstallSecurity(Pins[i].Raw);
     string digest=expected.Directory?null:Hash(Pins[i].Raw,expected.Bytes,true);
     if(!expected.Directory)Need(digest==expected.Sha256,"FULL_HELD_PE_SHA");
     SameFile(before,Identity(Pins[i].Raw,expected.Path,expected.Directory,null));
     Actual[i]=new FilePin(before.Path,before.Directory,before.Volume,before.FileId,before.Bytes,before.CreationFileTime,before.ModifiedFileTime,before.Attributes,before.Links,digest);
    }
   }
   void Pipe(Slot read,Slot write) {
    Check();read.Begin();write.Begin();Native.SA sa=new Native.SA();sa.Length=Marshal.SizeOf(typeof(Native.SA));sa.Inherit=1;
    bool ok=Native.CreatePipe(ref read.Raw,ref write.Raw,ref sa,4096);int error=ok?0:Marshal.GetLastWin32Error();
    // Capture BOTH outputs before any branch/throw. False with partial outputs is debt, never absence.
    read.Capture(read.Raw,ok,error);write.Capture(write.Raw,ok,error);Unique(read);Unique(write);
    Need(read.State==ResourceState.Open&&write.State==ResourceState.Open,"PIPE_PARTIAL_OR_FAILED");
   }
   void NotInherited(Slot slot){Check();Need(Native.SetHandleInformation(slot.Raw,1,0),"REMOVE_PARENT_END_INHERITANCE");}
   void Attributes(ref Native.STARTUPINFOEX si) {
    Check();UIntPtr size=UIntPtr.Zero;bool queried=Native.InitializeProcThreadAttributeList(IntPtr.Zero,1,0,ref size);
    int error=queried?0:Marshal.GetLastWin32Error();
    Need(!queried&&error==122&&size.ToUInt64()>0&&size.ToUInt64()<=65536,"ATTRIBUTE_SIZE_QUERY");
    Memory(AttributeMemory,(int)size.ToUInt64());Need(Native.InitializeProcThreadAttributeList(AttributeMemory.Raw,1,0,ref size),"ATTRIBUTE_INITIALIZE");
    AttributeInitialized=true;Memory(HandleListMemory,24);
    Marshal.WriteIntPtr(HandleListMemory.Raw,0,InputRead.Raw);Marshal.WriteIntPtr(HandleListMemory.Raw,8,OutWrite.Raw);Marshal.WriteIntPtr(HandleListMemory.Raw,16,ErrWrite.Raw);
    Need(Native.UpdateProcThreadAttribute(AttributeMemory.Raw,0,new IntPtr(0x20002),HandleListMemory.Raw,new UIntPtr(24),IntPtr.Zero,IntPtr.Zero),"EXACT_STDIO_HANDLE_LIST");
    si.Attributes=AttributeMemory.Raw;
   }
   void StartReader(Reader r) {
    Check();r.Thread.Begin();IntPtr raw=Native.CreateThread(IntPtr.Zero,new UIntPtr(1048576),r.Callback,IntPtr.Zero,0,ref r.Id);
    int error=raw==IntPtr.Zero?Marshal.GetLastWin32Error():0;Accept(r.Thread,raw,raw!=IntPtr.Zero,error);
    Need(r.Id!=0&&Native.GetThreadId(raw)==r.Id,"ORIGINAL_READER_THREAD_ID");
    Native.FT created,exit,kernel,user;Need(Native.GetThreadTimes(raw,out created,out exit,out kernel,out user)&&created.Value!=0,"READER_CREATION_FILETIME");r.Created=created.Value;
   }
   internal uint Drain(Reader r) {
    // Runs only on CreateThread's original thread. No shared query buffer, owner
    // Check(), callbacks to ROOT, primary-token changes, closes, locks or retries.
    try {
     for(;;){
      if(Volatile.Read(ref Unknown)!=0)return 1;
      r.LastReadCount=0;r.LastReadReturned=null;bool ok=Native.ReadFile(r.Pipe.Raw,r.Buffer,(uint)r.Buffer.Length,ref r.LastReadCount,IntPtr.Zero);
      int error=ok?0:Marshal.GetLastWin32Error();r.LastReadError=error;r.LastReadReturned=ok;
      if(Volatile.Read(ref Unknown)!=0)return 1;
      if((ok&&r.LastReadCount==0)||(!ok&&error==109&&r.LastReadCount==0)){Volatile.Write(ref r.Eof,1);Volatile.Write(ref r.Finished,1);return 0;}
      if(!ok||r.LastReadCount>r.Buffer.Length){Fail("ORIGINAL_PIPE_READ_ERROR");return 2;}
      if(r.Bytes+r.LastReadCount>2097152){Fail("OUTPUT_CAP_NOT_EOF");return 3;}
      r.LastWriteCount=0;r.LastWriteReturned=null;bool wrote=Native.WriteFile(r.Sink.Raw,r.Buffer,r.LastReadCount,ref r.LastWriteCount,IntPtr.Zero);
      r.LastWriteError=wrote?0:Marshal.GetLastWin32Error();r.LastWriteReturned=wrote;
      if(!wrote||r.LastWriteCount!=r.LastReadCount){Fail("ORIGINAL_LOG_PARTIAL_WRITE");return 4;}
      Buffer.BlockCopy(r.Buffer,0,r.Evidence,(int)r.Bytes,(int)r.LastReadCount);
      r.Bytes+=r.LastReadCount;Volatile.Write(ref r.Published,r.Bytes);
      if(Object.ReferenceEquals(r,Err)&&InputProtocol1701.Failure(r.Evidence,Math.Max(0,(int)(r.Bytes-r.LastReadCount)-128),(int)r.Bytes)){
       Input.Ledger.Poison();Fail("ACTUAL_C_FAILURE_MARKER_RETAINED");return 6;}
     }
    }catch {Fail("ORIGINAL_NATIVE_READER_UNWIND");return 5;}
    finally {Volatile.Write(ref r.Finished,1);}
   }
   void CapturePi() {
    if(Process.State==ResourceState.InFlight){if(CreateReturned)Process.Capture(Pi.Process,CreateSuccess,CreateError);else Process.CaptureUnknown(Pi.Process);}
    if(Primary.State==ResourceState.InFlight){if(CreateReturned)Primary.Capture(Pi.Thread,CreateSuccess,CreateError);else Primary.CaptureUnknown(Pi.Thread);}
   }
   bool Signal(Slot h){Check();uint value=Native.WaitForSingleObject(h.Raw,0);Need(value==0||value==258,"ORIGINAL_WAIT_FAILED");return value==0;}
   uint Code(Slot h,bool process) {
    Check();DwordQuery q=Object.ReferenceEquals(h,Process)?ProcessQuery:Object.ReferenceEquals(h,Primary)?PrimaryQuery:Object.ReferenceEquals(h,Out.Thread)?OutQuery:Object.ReferenceEquals(h,Err.Thread)?ErrQuery:InputQuery;
    q.AfterSignal=true;q.Returned=null;
    bool ok=process?Native.GetExitCodeProcess(h.Raw,ref q.Raw):Native.GetExitCodeThread(h.Raw,ref q.Raw);
    q.Error=ok?0:Marshal.GetLastWin32Error();q.Returned=ok;
    Need(ok,"FULL_NATIVE_DWORD_QUERY");return q.Raw;
   }
   void InstallSecurity(IntPtr handle) {
    Check();uint n=0;Need(Native.GetKernelObjectSecurity(handle,5,Scratch.Raw,65536,ref n)&&n>=20&&n<=65536,"INSTALL_DIRECTORY_SECURITY_QUERY");
    byte[] sd=new byte[n];Marshal.Copy(Scratch.Raw,sd,0,(int)n);
    uint control=BitConverter.ToUInt16(sd,2),owner=Rules.U32(sd,4),dacl=Rules.U32(sd,16);
    Need(sd[0]==1&&(control&0x8004)==0x8004&&owner>=20&&owner<sd.Length&&dacl>=20&&dacl<=sd.Length-8,"INSTALL_RELATIVE_SECURITY");
    byte[] actualOwner=Rules.Sid(sd,(int)owner);int bytes=BitConverter.ToUInt16(sd,(int)dacl+2);
    Need(Rules.SafeInstall(Rules.Slice(sd,(int)dacl,bytes),Initial.User,actualOwner,Initial.Logon,(control&0x1000)!=0),"INSTALL_DACL_UNTRUSTED_WRITER_OR_INHERITANCE");
   }
   void Security(Slot h,Subject expected) {
    Check();uint n=0;Need(Native.GetKernelObjectSecurity(h.Raw,5,Scratch.Raw,65536,ref n)&&n>=20&&n<=65536,"ACTUAL_P_KERNEL_SECURITY");
    byte[] sd=new byte[n];Marshal.Copy(Scratch.Raw,sd,0,(int)n);
    uint control=BitConverter.ToUInt16(sd,2),owner=Rules.U32(sd,4),dacl=Rules.U32(sd,16);
    Need(sd[0]==1&&(control&0x8004)==0x8004&&owner>=20&&owner<sd.Length&&dacl>=20&&dacl<=sd.Length-8,"ACTUAL_SELF_RELATIVE_DACL");
    byte[] actualOwner=Rules.Sid(sd,(int)owner);int bytes=BitConverter.ToUInt16(sd,(int)dacl+2);
    Need(Rules.Equal(actualOwner,expected.Owner)&&Rules.SafeDefault(Rules.Slice(sd,(int)dacl,bytes),expected.User,actualOwner,expected.Logon),"ACTUAL_P_OWNER_DACL_ADMISSION");
   }
   internal void ObserveOnce() {
     Check();Need(++ObservationPasses<=1200000,"FINITE_OBSERVATION_PASS_CAP");
     if(!Facts.ProcessSignal)Facts.ProcessSignal=Signal(Process);if(!Facts.PrimarySignal)Facts.PrimarySignal=Signal(Primary);
     if(!Facts.OutReaderSignal)Facts.OutReaderSignal=Signal(Out.Thread);if(!Facts.ErrReaderSignal)Facts.ErrReaderSignal=Signal(Err.Thread);
     Facts.OutEof=Volatile.Read(ref Out.Eof)!=0;Facts.ErrEof=Volatile.Read(ref Err.Eof)!=0;
     if(Facts.ProcessSignal&&!Facts.ProcessCode.HasValue)Facts.ProcessCode=Code(Process,true);
     if(Facts.PrimarySignal&&!Facts.PrimaryCode.HasValue)Facts.PrimaryCode=Code(Primary,false);
     if(Facts.OutReaderSignal&&!Facts.OutReaderCode.HasValue){Out.Exit=Code(Out.Thread,false);Facts.OutReaderCode=Out.Exit;}
     if(Facts.ErrReaderSignal&&!Facts.ErrReaderCode.HasValue){Err.Exit=Code(Err.Thread,false);Facts.ErrReaderCode=Err.Exit;}
     Need(!Facts.ProcessSignal||Facts.ProcessCode==0,"P_NONZERO_RETAINED");
     Need(!Facts.PrimarySignal||Facts.PrimaryCode==0,"P_PRIMARY_NONZERO_RETAINED");
     Need(!Facts.OutReaderSignal||Facts.OutReaderCode==0,"STDOUT_READER_NONZERO_RETAINED");
     Need(!Facts.ErrReaderSignal||Facts.ErrReaderCode==0,"STDERR_READER_NONZERO_RETAINED");
   }
   void Retire() {
    Check();Need(Facts.Ready(),"REAL_COMPLETION_REQUIRED");RetireInput();
    // Native thread signals + full DWORD0 are observed BEFORE ANY pipe/log/
    // callback/attribute/file-pin retirement. No managed Join is used.
    Close(Out.Thread);Close(Err.Thread);
    Close(Out.Pipe);Close(Err.Pipe);
    Need(Native.FlushFileBuffers(Out.Sink.Raw)&&Native.FlushFileBuffers(Err.Sink.Raw),"ORIGINAL_LOG_FLUSH");
    Out.Hash=Hash(Out.Sink.Raw,(ulong)Out.Bytes,false);Err.Hash=Hash(Err.Sink.Raw,(ulong)Err.Bytes,false);
    Close(Out.Sink);Close(Err.Sink);
    Close(Primary);Close(Process);
    Close(Token0);Close(Token1);Close(Token2);Close(ChildToken);
    for(int i=Pins.Length-1;i>=0;i--){
     FilePin final=Identity(Pins[i].Raw,Plan.Pins[i].Path,Plan.Pins[i].Directory,null,i);
     SameFile(Actual[i],final);
     if(!Plan.Pins[i].Directory){
      string digest=Hash(Pins[i].Raw,Plan.Pins[i].Bytes,true);
      FinalPins[i]=FileMetadataRules1718.WithDigest(final,digest);
      Need(digest==Plan.Pins[i].Sha256,"FINAL_HELD_PE_SHA");
     }
     Close(Pins[i]);
    }
    Close(OutputDirectory);
    Check();Need(AttributeInitialized,"ATTRIBUTE_OWNER");Native.DeleteProcThreadAttributeList(AttributeMemory.Raw);AttributeInitialized=false;
    Free(EnvironmentMemory);Free(HandleListMemory);Free(AttributeMemory);Free(Scratch);
    foreach(Slot s in Slots)Need(s.State==ResourceState.ClosedVerified||s.State==ResourceState.ProvenAbsent||s.State==ResourceState.MemoryRetired,"UNRETIRED_ORIGINAL_SLOT");
   }
   internal void Start() {
    Clock.Start();CallingThread=Native.GetCurrentThreadId();Check();
    try {
     NoImpersonation(NoThread[0]);Memory(Scratch,65536);OpenToken(Token0,Native.GetCurrentProcess());Initial=ReadSubject(Token0.Raw);
     bool inJob;Need(Native.IsProcessInJob(Native.GetCurrentProcess(),IntPtr.Zero,out inJob)&&!inJob,"CREATOR_JOB_NOT_PROVEN_EMPTY");
     StringBuilder system=new StringBuilder(260);uint n=Native.GetWindowsDirectoryW(system,260);
     Need(n>0&&n<260&&system.ToString()==@"C:\Windows","FIXED_GUEST_WINDOWS_ROOT");
     PinAll();Check();
     NamespaceAttempted=true;Need(Native.CreateDirectoryW(OutputRoot,IntPtr.Zero),"FRESH_OUTPUT_NAMESPACE_REQUIRED");NamespaceCreated=true;
     OpenFile(OutputDirectory,OutputRoot,0x80000000,3,true,false);
     Identity(OutputDirectory.Raw,OutputRoot,true,null);
     OpenFile(Out.Sink,OutputRoot+@"\stdout.log",0xc0000000,1,false,false);
     Need(Identity(Out.Sink.Raw,OutputRoot+@"\stdout.log",false,null).Bytes==0,"NEW_STDOUT_EMPTY");
     OpenFile(Err.Sink,OutputRoot+@"\stderr.log",0xc0000000,1,false,false);
     Need(Identity(Err.Sink.Raw,OutputRoot+@"\stderr.log",false,null).Bytes==0,"NEW_STDERR_EMPTY");
     Pipe(InputRead,InputWrite);Pipe(Out.Pipe,OutWrite);Pipe(Err.Pipe,ErrWrite);
     NotInherited(InputWrite);NotInherited(Out.Pipe);NotInherited(Err.Pipe);
     Native.STARTUPINFOEX si=new Native.STARTUPINFOEX();si.Info.Size=Marshal.SizeOf(typeof(Native.STARTUPINFOEX));
     si.Info.Flags=0x100;si.Info.Input=InputRead.Raw;si.Info.Output=OutWrite.Raw;si.Info.Error=ErrWrite.Raw;
     Attributes(ref si);
     byte[] environment=Encoding.Unicode.GetBytes("SystemDrive=C:\0SystemRoot=C:\\Windows\0WINDIR=C:\\Windows\0\0");
     Memory(EnvironmentMemory,environment.Length);Marshal.Copy(environment,0,EnvironmentMemory.Raw,environment.Length);
     StartReader(Out);StartReader(Err);StartInputThread();
     // Dedicated same caller thread: actual no-token/open-primary/query brackets,
     // then one NULL-SA suspended CreateProcess. No token adjustments/repair.
     NoImpersonation(NoThread[1]);OpenToken(Token1,Native.GetCurrentProcess());Subject pre=ReadSubject(Token1.Raw);
     Need(Initial.Same(pre),"PRIMARY_PRECREATE_DRIFT");Check();
     Process.Begin();Primary.Begin();CreateStarted=true;
     StringBuilder command=new StringBuilder(Plan.CommandLine(),4096);
     CreateSuccess=Native.CreateProcessW(Plan.Install+"\\"+RoleNames[0],command,IntPtr.Zero,IntPtr.Zero,true,
      0x00080000u|0x00000400u|0x00000004u|0x08000000u,EnvironmentMemory.Raw,Plan.Install,ref si,ref Pi);
     CreateError=CreateSuccess?0:Marshal.GetLastWin32Error();CreateReturned=true;CapturePi();
     Unique(Process);Unique(Primary);
     Need(Rules.Pi(CreateSuccess,Pi.Process,Pi.Thread,Pi.Pid,Pi.Tid),"P_CREATE_REJECTED_OR_PARTIAL");
     NoImpersonation(NoThread[2]);OpenToken(Token2,Native.GetCurrentProcess());Subject post=ReadSubject(Token2.Raw);
     Need(Initial.Same(post)&&pre.Same(post),"PRIMARY_POSTCREATE_DRIFT");Facts.TokenWindow=true;
     Need(Native.GetProcessId(Process.Raw)==Pi.Pid&&Native.GetThreadId(Primary.Raw)==Pi.Tid&&Native.GetProcessIdOfThread(Primary.Raw)==Pi.Pid,"ORIGINAL_PI_IDENTITIES");
     Native.FT created,exit,kernel,user;
     Need(Native.GetProcessTimes(Process.Raw,out created,out exit,out kernel,out user)&&created.Value!=0,"P_CREATION_FILETIME");
     ProcessCreated=created.Value;
     Need(Native.GetThreadTimes(Primary.Raw,out created,out exit,out kernel,out user)&&created.Value!=0,"P_PRIMARY_FILETIME");PrimaryCreated=created.Value;
     StringBuilder image=new StringBuilder(2048);uint cap=2048;
     Need(Native.QueryFullProcessImageNameW(Process.Raw,0,image,ref cap)&&String.Equals(image.ToString(),Plan.Install+"\\"+RoleNames[0],StringComparison.OrdinalIgnoreCase),"ORIGINAL_P_IMAGE_PATH");
     Need(Native.IsProcessInJob(Process.Raw,IntPtr.Zero,out inJob)&&!inJob,"P_FOREIGN_JOB_UNQUALIFIED");
     OpenToken(ChildToken,Process.Raw);Subject child=ReadSubject(ChildToken.Raw);
     Need(Rules.Equal(child.Body,Initial.Body)&&Rules.Equal(Rules.Slice(child.Stats,8,8),Rules.Slice(Initial.Stats,8,8)),"ACTUAL_P_PRIMARY_SECURITY_CONTEXT");
     Security(Process,Initial);Security(Primary,Initial);Facts.Identity=true;
     Check();ResumeStarted=true;ResumeReturn=Native.ResumeThread(Primary.Raw);
     Need(ResumeReturn==1,"P_RESUME_UNCERTAIN_NO_RETRY");Facts.ResumedOnce=true;
     // Retire only ROOT's child-side stdio aliases. Keep ROOT's input writer
     // open throughout controlled delivery; no EOF substitutes for interaction.
     Close(InputRead);Close(OutWrite);Close(ErrWrite);
     // InputWrite remains owned and open for ROOT's same-instance reviewed lines.
    }catch {
     // Pure preservation only. Even an exception between native return and
     // slot latching retains PI's actual outputs; never closes or retries.
     if(CreateStarted){try{CapturePi();}catch{}}
     Fail("ROOT_P_CAPTURE_UNWIND");throw;
    }
   }
   ulong? ProcessCreated,PrimaryCreated;int ObservationPasses;
   internal CaptureObservation Snapshot(string status) {
    SlotObservation[] slots=new SlotObservation[Slots.Count];
    for(int i=0;i<Slots.Count;i++){Slot s=Slots[i];slots[i]=new SlotObservation{Name=s.Name,State=s.State,OriginalValueCaptured=s.Raw!=IntPtr.Zero,ApiReturned=s.Returned,Error=s.LastError};}
    return new CaptureObservation{Status=status,FirstError=First,NoReplay=true,Pid=Pi.Pid==0?(uint?)null:Pi.Pid,
     PrimaryThreadId=Pi.Tid==0?(uint?)null:Pi.Tid,CreationFileTime=ProcessCreated,PrimaryCreationFileTime=PrimaryCreated,
     ProcessDword=Facts.ProcessCode,PrimaryThreadDword=Facts.PrimaryCode,OutReaderDword=Out.Exit,ErrReaderDword=Err.Exit,
     OutReaderCreationFileTime=Out.Created,ErrReaderCreationFileTime=Err.Created,
     StdoutEof=Volatile.Read(ref Out.Eof)!=0,StderrEof=Volatile.Read(ref Err.Eof)!=0,
     OriginalReadersSignaled=Facts.OutReaderSignal&&Facts.ErrReaderSignal,CreatorWindowObserved=Facts.TokenWindow,
     StdoutBytes=Out.Bytes,StderrBytes=Err.Bytes,StdoutSha256=Out.Hash,StderrSha256=Err.Hash,Slots=slots,
     CreateWasAttempted=CreateStarted,CreateReturned=CreateReturned,CreateSuccess=CreateReturned?(bool?)CreateSuccess:null,CreateError=CreateError,
     ResumeReturn=ResumeStarted?(uint?)ResumeReturn:null,
     OriginalFilePins=(FilePin[])Actual.Clone(),FinalFilePins=(FilePin[])FinalPins.Clone(),ObservationPasses=ObservationPasses,Input=InputSnapshot(),CreatorNativeThreadId=CallingThread==0?(uint?)null:CallingThread,
     DwordQueries=new DwordObservation[]{QuerySnapshot("P",ProcessQuery),QuerySnapshot("P_PRIMARY",PrimaryQuery),QuerySnapshot("STDOUT_NATIVE_READER",OutQuery),QuerySnapshot("STDERR_NATIVE_READER",ErrQuery),QuerySnapshot("ROOT_INPUT_NATIVE_THREAD",InputQuery)}};
   }
  }
  static DwordObservation QuerySnapshot(string name,DwordQuery q){return new DwordObservation{Name=name,RawValue=q.Returned.HasValue?(uint?)q.Raw:null,Returned=q.Returned,Error=q.Error,AfterOriginalSignal=q.AfterSignal};}
  internal static class Native {
   [StructLayout(LayoutKind.Sequential)] internal struct SA {internal int Length;internal IntPtr Descriptor;internal int Inherit;}
   [StructLayout(LayoutKind.Sequential)] internal struct FT {internal uint Low,High;internal ulong Value{get{return ((ulong)High<<32)|Low;}}}
   [StructLayout(LayoutKind.Sequential)] internal struct BYINFO {internal uint Attributes;internal FT Created,Accessed,Modified;internal uint Volume,SizeHigh,SizeLow,Links,IndexHigh,IndexLow;}
   [StructLayout(LayoutKind.Sequential)] internal struct IDINFO {internal ulong Volume;[MarshalAs(UnmanagedType.ByValArray,SizeConst=16)]internal byte[] Id;}
   [StructLayout(LayoutKind.Sequential)] internal struct PI {internal IntPtr Process,Thread;internal uint Pid,Tid;}
   [StructLayout(LayoutKind.Sequential,CharSet=CharSet.Unicode)] internal struct STARTUPINFO {
    internal int Size;internal IntPtr Reserved,Desktop,Title;internal uint X,Y,XSize,YSize,XChars,YChars,Fill,Flags;
    internal ushort Show,ReservedBytes;internal IntPtr Reserved2,Input,Output,Error;
   }
   [StructLayout(LayoutKind.Sequential)] internal struct STARTUPINFOEX {internal STARTUPINFO Info;internal IntPtr Attributes;}
   [UnmanagedFunctionPointer(CallingConvention.Winapi)] internal delegate uint ThreadStart(IntPtr context);
   [DllImport("kernel32.dll")] internal static extern IntPtr GetCurrentProcess();
   [DllImport("kernel32.dll")] internal static extern IntPtr GetCurrentThread();
   [DllImport("kernel32.dll")] internal static extern uint GetCurrentThreadId();
   [DllImport("kernel32.dll",SetLastError=true)] internal static extern uint GetProcessId(IntPtr handle);
   [DllImport("kernel32.dll",SetLastError=true)] internal static extern uint GetThreadId(IntPtr handle);
   [DllImport("kernel32.dll",SetLastError=true)] internal static extern uint GetProcessIdOfThread(IntPtr handle);
   [DllImport("kernel32.dll",SetLastError=true)] internal static extern bool GetProcessTimes(IntPtr h,out FT c,out FT e,out FT k,out FT u);
   [DllImport("kernel32.dll",SetLastError=true)] internal static extern bool GetThreadTimes(IntPtr h,out FT c,out FT e,out FT k,out FT u);
   [DllImport("kernel32.dll",SetLastError=true)] internal static extern bool CloseHandle(IntPtr handle);
   [DllImport("kernel32.dll",SetLastError=true)] internal static extern uint WaitForSingleObject(IntPtr h,uint ms);
   [DllImport("kernel32.dll",SetLastError=true)] internal static extern bool GetExitCodeProcess(IntPtr h,ref uint code);
   [DllImport("kernel32.dll",SetLastError=true)] internal static extern bool GetExitCodeThread(IntPtr h,ref uint code);
   [DllImport("kernel32.dll",SetLastError=true)] internal static extern uint ResumeThread(IntPtr h);
   [DllImport("kernel32.dll",SetLastError=true)] internal static extern IntPtr CreateThread(IntPtr sa,UIntPtr stack,ThreadStart start,IntPtr context,uint flags,ref uint id);
   [DllImport("kernel32.dll",SetLastError=true)] internal static extern bool IsProcessInJob(IntPtr process,IntPtr job,out bool result);
   [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] internal static extern uint GetWindowsDirectoryW(StringBuilder b,uint size);
   [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] internal static extern bool QueryFullProcessImageNameW(IntPtr h,uint flags,StringBuilder b,ref uint size);
   [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] internal static extern IntPtr CreateFileW(string p,uint access,uint share,ref SA sa,uint disposition,uint flags,IntPtr template);
   [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] internal static extern bool CreateDirectoryW(string p,IntPtr sa);
   [DllImport("kernel32.dll",SetLastError=true)] internal static extern bool GetFileInformationByHandle(IntPtr h,out BYINFO info);
   [DllImport("kernel32.dll",SetLastError=true)] internal static extern bool GetFileInformationByHandleEx(IntPtr h,int kind,out IDINFO info,uint size);
   [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] internal static extern uint GetFinalPathNameByHandleW(IntPtr h,StringBuilder b,uint size,uint flags);
   [DllImport("kernel32.dll",SetLastError=true)] internal static extern bool SetFilePointerEx(IntPtr h,long distance,out long actual,uint method);
   [DllImport("kernel32.dll",SetLastError=true)] internal static extern bool ReadFile(IntPtr h,[Out]byte[] buffer,uint size,ref uint read,IntPtr overlapped);
   [DllImport("kernel32.dll",SetLastError=true)] internal static extern bool WriteFile(IntPtr h,byte[] buffer,uint size,ref uint written,IntPtr overlapped);
   [DllImport("kernel32.dll",SetLastError=true)] internal static extern bool FlushFileBuffers(IntPtr h);
   [DllImport("kernel32.dll",SetLastError=true)] internal static extern bool CreatePipe(ref IntPtr read,ref IntPtr write,ref SA sa,uint size);
   [DllImport("kernel32.dll",SetLastError=true)] internal static extern bool SetHandleInformation(IntPtr h,uint mask,uint flags);
   [DllImport("kernel32.dll",SetLastError=true)] internal static extern bool InitializeProcThreadAttributeList(IntPtr list,int count,uint flags,ref UIntPtr size);
   [DllImport("kernel32.dll",SetLastError=true)] internal static extern bool UpdateProcThreadAttribute(IntPtr list,uint flags,IntPtr attribute,IntPtr value,UIntPtr size,IntPtr previous,IntPtr returned);
   [DllImport("kernel32.dll")] internal static extern void DeleteProcThreadAttributeList(IntPtr list);
   [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] internal static extern bool CreateProcessW(string app,StringBuilder command,IntPtr processSA,IntPtr threadSA,bool inherit,uint flags,IntPtr environment,string cwd,ref STARTUPINFOEX startup,ref PI pi);
   [DllImport("advapi32.dll",SetLastError=true)] internal static extern bool OpenThreadToken(IntPtr thread,uint access,bool asSelf,ref IntPtr token);
   [DllImport("advapi32.dll",SetLastError=true)] internal static extern bool OpenProcessToken(IntPtr process,uint access,ref IntPtr token);
   [DllImport("advapi32.dll",SetLastError=true)] internal static extern bool GetTokenInformation(IntPtr token,int kind,IntPtr buffer,uint size,ref uint returned);
   [DllImport("advapi32.dll",SetLastError=true)] internal static extern bool GetKernelObjectSecurity(IntPtr handle,uint security,IntPtr buffer,uint size,ref uint returned);
  }
 }
}
