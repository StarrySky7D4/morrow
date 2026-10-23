// ROOT QA source only. No static/native initialization or automatic entry point.
// Original implementation for this helper; product/AGY sources are referenced, not copied.
using System;
using System.Collections.Generic;
using System.Text;

namespace C28.RootP1682 {
 public enum ResourceState { Empty, InFlight, Open, Rejected, ProvenAbsent, RetainedFailure, CloseInFlight, CloseFailed, ClosedVerified, MemoryRetired }
 internal sealed class Slot {
  internal readonly string Name; internal IntPtr Raw; internal ResourceState State;
  internal int LastError; internal bool? Returned;
  internal Slot(string name) { Name=name; }
  internal void Begin() { if(State!=ResourceState.Empty)throw new InvalidOperationException("SLOT_REUSE"); State=ResourceState.InFlight; }
  internal void Capture(IntPtr value,bool success,int error) {
   if(State!=ResourceState.InFlight)throw new InvalidOperationException("CAPTURE_STATE");
   Raw=value; Returned=success; LastError=error;
   bool valid=value!=IntPtr.Zero && value!=new IntPtr(-1);
   State=success&&valid?ResourceState.Open:(!success&&!valid?ResourceState.Rejected:ResourceState.RetainedFailure);
  }
  internal void CaptureUnknown(IntPtr value) {
   if(State!=ResourceState.InFlight)throw new InvalidOperationException("UNKNOWN_CAPTURE_STATE");
   Raw=value;Returned=null;State=ResourceState.RetainedFailure;
  }
  internal void Absent(bool success,int error) {
   Capture(Raw,success,error);
   if(!success&&Raw==IntPtr.Zero&&error==1008)State=ResourceState.ProvenAbsent;
   else throw new InvalidOperationException("THREAD_TOKEN_NOT_ABSENT");
  }
  internal void BeginClose() {
   if(State!=ResourceState.Open)throw new InvalidOperationException("NO_CLOSE_OR_RETRY");
   State=ResourceState.CloseInFlight; Returned=null;
  }
  internal bool Closed(bool success,int error) {
   if(State!=ResourceState.CloseInFlight)throw new InvalidOperationException("CLOSE_STATE");
   Returned=success; LastError=error; State=success?ResourceState.ClosedVerified:ResourceState.CloseFailed;
   return success; // Raw is historical even on success; never zeroed or re-operated.
  }
 }
 internal sealed class Completion {
  internal bool OutEof,ErrEof,OutReaderSignal,ErrReaderSignal,ProcessSignal,PrimarySignal;
  internal uint? OutReaderCode,ErrReaderCode,ProcessCode,PrimaryCode;
  internal bool Identity,TokenWindow,ResumedOnce;
  internal bool Ready() {
   return Identity&&TokenWindow&&ResumedOnce&&OutEof&&ErrEof&&OutReaderSignal&&ErrReaderSignal
    &&ProcessSignal&&PrimarySignal&&OutReaderCode==0&&ErrReaderCode==0&&ProcessCode==0&&PrimaryCode==0;
  }
 }
 internal static class Rules {
  internal static bool Budget(long elapsed,long limit) { return elapsed>=0&&limit>=1&&limit<=5500000&&elapsed<limit; }
  internal static bool Pi(bool returned,IntPtr process,IntPtr thread,uint pid,uint tid) {
   return returned&&process!=IntPtr.Zero&&thread!=IntPtr.Zero&&process!=new IntPtr(-1)
    &&thread!=new IntPtr(-1)&&process!=thread&&pid!=0&&tid!=0;
  }
  internal static bool Hex(string s,int count) {
   if(s==null||s.Length!=count)return false;bool nonzero=false;
   foreach(char c in s){if(!((c>='0'&&c<='9')||(c>='a'&&c<='f')))return false;if(c!='0')nonzero=true;}
   return nonzero;
  }
  internal static string LocalPath(string s) {
   if(s==null||s.Length<3||s.Length>1024||!s.StartsWith(@"C:\",StringComparison.Ordinal)
    ||s.IndexOfAny(new char[]{'\0','"','/','*','?','<','>','|'})>=0||s.IndexOf(':',2)>=0)
    throw new InvalidOperationException("LOCAL_DOS_PATH");
   string[] parts=s.Substring(3).Split('\\');
   foreach(string p in parts)if(p=="."||p==".."||(p.Length==0&&s.Length!=3)||p.EndsWith(" ",StringComparison.Ordinal)||p.EndsWith(".",StringComparison.Ordinal))
    throw new InvalidOperationException("PATH_COMPONENT");
   foreach(string p in parts){string stem=p.Split('.')[0].ToUpperInvariant();
    if(String.Equals(p,"user.txt",StringComparison.OrdinalIgnoreCase)||stem=="CON"||stem=="PRN"||stem=="AUX"||stem=="NUL"||stem=="CONIN$"||stem=="CONOUT$"
     ||(stem.Length==4&&(stem.StartsWith("COM",StringComparison.Ordinal)||stem.StartsWith("LPT",StringComparison.Ordinal))&&stem[3]>='1'&&stem[3]<='9'))
     throw new InvalidOperationException("DOS_DEVICE_COMPONENT");}
   return s;
  }
  internal static bool Equal(byte[] a,byte[] b) {
   if(a==null||b==null||a.Length!=b.Length)return false;int n=0;
   for(int i=0;i<a.Length;i++)n|=a[i]^b[i];return n==0;
  }
  internal static string HexBytes(byte[] a) { return BitConverter.ToString(a).Replace("-","").ToLowerInvariant(); }
  internal static uint U32(byte[] b,int at) { if(at<0||at>b.Length-4)throw new InvalidOperationException("U32_RANGE");return BitConverter.ToUInt32(b,at); }
  internal static byte[] Slice(byte[] b,int start,int count) {
   if(start<0||count<0||start>b.Length-count)throw new InvalidOperationException("BYTE_RANGE");
   byte[] r=new byte[count];Buffer.BlockCopy(b,start,r,0,count);return r;
  }
  internal static byte[] Sid(byte[] b,int start) {
   if(start<0||start>b.Length-8||b[start]!=1||b[start+1]>15)throw new InvalidOperationException("SID_RANGE");
   return Slice(b,start,8+4*b[start+1]);
  }
  internal static bool Logon(byte[] s) {
   return s!=null&&s.Length==20&&s[0]==1&&s[1]==3&&s[2]==0&&s[3]==0&&s[4]==0&&s[5]==0&&s[6]==0&&s[7]==5&&U32(s,8)==5;
  }
  internal static readonly byte[] SystemSid={1,1,0,0,0,0,0,5,18,0,0,0};
  internal static readonly byte[] AdminSid={1,2,0,0,0,0,0,5,32,0,0,0,32,2,0,0};
  internal static readonly byte[] OwnerRights={1,1,0,0,0,0,0,3,4,0,0,0};
  // Reads the ACTUAL bounded binary ACL. Unsupported ACEs/flags/null ACL fail closed.
  // Protected install directory: no untrusted child creation, deletion or ACL
  // writer. Read/execute grants may remain; inherited-only ACEs do not apply here.
  internal static bool SafeInstall(byte[] acl,byte[] user,byte[] owner,byte[] logon,bool protectedDacl) {
   try {
    if(!protectedDacl||!(Equal(owner,user)||Equal(owner,SystemSid)||Equal(owner,AdminSid))
     ||acl==null||acl.Length<8||acl.Length>4096||(acl[0]!=2&&acl[0]!=4)||acl[1]!=0||acl[6]!=0||acl[7]!=0
     ||BitConverter.ToUInt16(acl,2)!=acl.Length)return false;
    int count=BitConverter.ToUInt16(acl,4),at=8;if(count<1||count>64)return false;
    for(int i=0;i<count;i++){
     if(at>acl.Length-8||(acl[at]!=0&&acl[at]!=1)||(acl[at+1]&0xe0)!=0)return false;
     int size=BitConverter.ToUInt16(acl,at+2);if(size<16||(size&3)!=0||at>acl.Length-size)return false;
     byte[] sid=Sid(acl,at+8);if(sid.Length+8!=size)return false;
     uint mask=U32(acl,at+4);
     if(acl[at]==0&&(acl[at+1]&8)==0&&(mask&0x530d0156)!=0
      &&!(Equal(sid,user)||Equal(sid,SystemSid)||Equal(sid,AdminSid)||Equal(sid,logon)))return false;
     at+=size;
    }
    return at==acl.Length;
   }catch(ArgumentException){return false;}catch(InvalidOperationException){return false;}
  }
  internal static bool SafeDefault(byte[] acl,byte[] user,byte[] owner,byte[] logon) {
   try {
    if(user==null||owner==null||!Logon(logon)||Logon(user)||Equal(user,SystemSid)||Equal(user,AdminSid)
     ||!(Equal(owner,user)||Equal(owner,SystemSid)||Equal(owner,AdminSid))
     ||acl==null||acl.Length<8||acl.Length>4096||(acl[0]!=2&&acl[0]!=4)||acl[1]!=0||acl[6]!=0||acl[7]!=0
     ||BitConverter.ToUInt16(acl,2)!=acl.Length)return false;
    int count=BitConverter.ToUInt16(acl,4),at=8;bool allow=false;
    if(count<1||count>64)return false;
    for(int i=0;i<count;i++){
     if(at>acl.Length-8||(acl[at]!=0&&acl[at]!=1)||acl[at+1]!=0)return false;
     int size=BitConverter.ToUInt16(acl,at+2);if(size<16||(size&3)!=0||at>acl.Length-size)return false;
     byte[] who=Sid(acl,at+8);if(who.Length+8!=size)return false;
     uint mask=U32(acl,at+4);
     if(acl[at]==0){
      bool trusted=Equal(who,user)||Equal(who,SystemSid)||Equal(who,AdminSid)||Equal(who,logon);
      if(!trusted&&!(Equal(who,OwnerRights)&&mask==0x20000))return false;
      if(trusted&&mask!=0)allow=true;
     }
     at+=size;
    }
    return at==acl.Length&&allow;
   }catch(ArgumentException){return false;}catch(InvalidOperationException){return false;}
  }
 }
 public sealed class FilePin {
  public readonly string Path,Sha256,FileId; public readonly bool Directory;
  public readonly ulong Volume,Bytes,CreationFileTime,ModifiedFileTime;public readonly uint Attributes,Links;
  public FilePin(string path,bool directory,ulong volume,string id,ulong bytes,ulong creation,ulong modified,uint attributes,uint links,string sha) {
   Path=path;Directory=directory;Volume=volume;FileId=id;Bytes=bytes;CreationFileTime=creation;ModifiedFileTime=modified;Attributes=attributes;Links=links;Sha256=sha;
  }
 }
 public sealed class RootSelection {
  public const string VmId="0bc007bf-8f66-4462-ba44-b957459cfd6d";
  public const string FrozenInventory="0ab4de50c69d8ab6a6a25927eba920ba5dc71304f3d30e377c14afe4229651a1";
  internal readonly string Install,Tag,Inventory;internal readonly FilePin[] Pins;internal readonly int DeadlineMs;
  // These are ROOT expected identities/bytes, not boolean Cargo or VM PASS receipts.
  public RootSelection(string install,string tag,string inventory,int deadlineMs,FilePin[] pins) {
   Install=install;Tag=tag;Inventory=inventory;DeadlineMs=deadlineMs;Pins=pins==null?null:(FilePin[])pins.Clone();
  }
  internal List<string> Paths() {
   Rules.LocalPath(Install);
   if(Install.Length<=3)throw new InvalidOperationException("INSTALL_NOT_ROOT");
   List<string> paths=new List<string>();AddAncestors(paths,Install);
   foreach(string role in RootPPrebirth.RoleNames)paths.Add(Install+"\\"+role);
   AddAncestors(paths,@"C:\Windows\System32\WindowsPowerShell\v1.0");
   paths.Add(RootPPrebirth.Shell);
   if(paths.Count>40)throw new InvalidOperationException("PIN_COUNT");
   return paths;
  }
  static void AddAncestors(List<string> list,string p) {
   Rules.LocalPath(p);List<string> reverse=new List<string>();reverse.Add(p);
   while(p.Length>3){int cut=p.LastIndexOf('\\');p=cut==2?p.Substring(0,3):p.Substring(0,cut);reverse.Add(p);}
   for(int i=reverse.Count-1;i>=0;i--)if(!list.Exists(delegate(string v){return String.Equals(v,reverse[i],StringComparison.OrdinalIgnoreCase);}))list.Add(reverse[i]);
  }
  internal void Validate() {
   if(IntPtr.Size!=8||!BitConverter.IsLittleEndian||!Rules.Hex(Tag,32)||Inventory!=FrozenInventory
    ||DeadlineMs<1||DeadlineMs>5500000||Pins==null)throw new InvalidOperationException("UNBOUND_NOT_EXECUTABLE");
   if(Install!=@"C:\Morrow-C28-RootP1682-bin-"+Tag&&Install!=@"C:\Morrow-C28-RootP1701-bin-"+Tag)throw new InvalidOperationException("FIXED_NEW_INSTALL_NAMESPACE");
   List<string> expected=Paths();if(Pins.Length!=expected.Count)throw new InvalidOperationException("EXACT_PIN_SET");
   ulong bytes=0;
   for(int i=0;i<Pins.Length;i++){
    FilePin p=Pins[i];if(p==null||p.Path!=expected[i]||!Rules.Hex(p.FileId,32)||p.Volume==0||p.CreationFileTime==0||p.ModifiedFileTime==0
     ||!FileMetadataRules1718.BoundLinks(p)||(p.Attributes&0x400)!=0||((p.Attributes&16)!=0)!=p.Directory)throw new InvalidOperationException("PIN_IDENTITY");
    Rules.LocalPath(p.Path);
    bool isFile=p.Path==RootPPrebirth.Shell;
    foreach(string role in RootPPrebirth.RoleNames)isFile|=p.Path==Install+"\\"+role;
    if(isFile==p.Directory)throw new InvalidOperationException("PIN_KIND");
    if(!p.Directory){if(p.Bytes<64||p.Bytes>536870912||!Rules.Hex(p.Sha256,64))throw new InvalidOperationException("PE_PIN");bytes+=p.Bytes;}
    else if(p.Sha256!=null&&p.Sha256!="")throw new InvalidOperationException("DIRECTORY_IS_NOT_PE_HASH");
   }
   if(bytes>2147483648UL)throw new InvalidOperationException("TOTAL_IMAGE_BYTES");
   for(int i=0;i<4;i++)for(int j=0;j<i;j++)if(RoleHash(i)==RoleHash(j))throw new InvalidOperationException("DISTINCT_ROLE_PINS");
  }
  internal string RoleHash(int role) {
   string path=role==4?RootPPrebirth.Shell:Install+"\\"+RootPPrebirth.RoleNames[role];
   foreach(FilePin p in Pins)if(p.Path==path)return p.Sha256;throw new InvalidOperationException("ROLE_PIN_ABSENT");
  }
#if P2_SHARED_PARENT_REFUSAL_SESSION
  internal const string ParentSelector="--c28-parent-refusal-session-p1";
#else
  internal const string ParentSelector="--c28-parent-launch-p1";
#endif
  internal string CommandLine() {
   string s="\""+Install+"\\"+RootPPrebirth.RoleNames[0]+"\" "+ParentSelector;
   for(int i=0;i<5;i++)s+=" "+RoleHash(i);return s+" "+Tag;
  }
 }
 public sealed class SlotObservation {
  public string Name;public ResourceState State;public bool OriginalValueCaptured;public bool? ApiReturned;public int Error;
 }
 internal sealed class DwordQuery {
  internal uint Raw;internal bool? Returned;internal int Error;internal bool AfterSignal;
  internal uint? ValidValue {get{return Returned==true&&AfterSignal?(uint?)Raw:null;}}
 }
 public sealed class DwordObservation {
  public string Name;public uint? RawValue;public bool? Returned;public int Error;public bool AfterOriginalSignal;
 }
 public sealed class CaptureObservation {
  public string Status,FirstError,StdoutSha256,StderrSha256;public bool NoReplay;
  public uint? Pid,PrimaryThreadId,ProcessDword,PrimaryThreadDword,OutReaderDword,ErrReaderDword;
  public ulong? CreationFileTime,PrimaryCreationFileTime,OutReaderCreationFileTime,ErrReaderCreationFileTime;
  public bool StdoutEof,StderrEof,OriginalReadersSignaled,CreatorWindowObserved;
  public long StdoutBytes,StderrBytes;public SlotObservation[] Slots;
  public bool CreateWasAttempted,CreateReturned;public bool? CreateSuccess;public int CreateError;public uint? ResumeReturn;
  public DwordObservation[] DwordQueries;
  public InputObservation1701 Input;public uint? CreatorNativeThreadId;
  public FilePin[] OriginalFilePins,FinalFilePins;public int ObservationPasses;
  public string Scope="ROOT_OUTER_P_ONLY_NOT_PRODUCT_OR_VM_ACCEPTANCE";
  public bool SourceAccepted=false,ReleaseEligible=false;
 }
}
