// SOURCE-only pure tests. Compile with both helper files in one assembly.
// Each public method is an independent ROOT selection; no auto-run or test framework.
// Tests call the production decisions/parsers, never Win32 or an alternative capture.
using System;
using System.Collections.Generic;
namespace C28.RootP1682 {
 public static class PureTests1682 {
  static void Assert(bool p){if(!p)throw new InvalidOperationException("PURE_ASSERTION_FAILED");}
  static void Reject(Action a){bool failed=false;try{a();}catch(InvalidOperationException){failed=true;}Assert(failed);}
  static byte[] Sid(params uint[] sub){
   byte[] b=new byte[8+4*sub.Length];b[0]=1;b[1]=(byte)sub.Length;b[7]=5;
   for(int i=0;i<sub.Length;i++)Buffer.BlockCopy(BitConverter.GetBytes(sub[i]),0,b,8+i*4,4);return b;
  }
  static byte[] User(){return Sid(21,10,20,30,1000);}
  static byte[] Logon(){return Sid(5,10,20);}
  static byte[] Acl(byte[] sid,uint mask,byte type,byte flags) {
   int size=16+sid.Length;byte[] b=new byte[size];b[0]=2;
   Buffer.BlockCopy(BitConverter.GetBytes((ushort)size),0,b,2,2);b[4]=1;b[8]=type;b[9]=flags;
   Buffer.BlockCopy(BitConverter.GetBytes((ushort)(8+sid.Length)),0,b,10,2);
   Buffer.BlockCopy(BitConverter.GetBytes(mask),0,b,12,4);Buffer.BlockCopy(sid,0,b,16,sid.Length);return b;
  }
  static Completion Complete(){
   return new Completion{Identity=true,TokenWindow=true,ResumedOnce=true,OutEof=true,ErrEof=true,
    ProcessSignal=true,PrimarySignal=true,OutReaderSignal=true,ErrReaderSignal=true,
    ProcessCode=0,PrimaryCode=0,OutReaderCode=0,ErrReaderCode=0};
  }
  static RootSelection Selection(){
   string install=@"C:\Morrow-C28-RootP1682-bin-"+new string('1',32);
   RootSelection provisional=new RootSelection(install,new string('1',32),RootSelection.FrozenInventory,1000,new FilePin[0]);
   List<string> paths=provisional.Paths();FilePin[] pins=new FilePin[paths.Count];
   for(int i=0;i<pins.Length;i++){
    bool file=paths[i].EndsWith(".exe",StringComparison.Ordinal);
    int role=0;if(file){for(int j=0;j<4;j++)if(paths[i].EndsWith(RootPPrebirth.RoleNames[j],StringComparison.Ordinal))role=j;if(paths[i]==RootPPrebirth.Shell)role=4;}
    string id=(i+1).ToString("x32");
    bool shell=paths[i]==RootPPrebirth.Shell;
    pins[i]=new FilePin(paths[i],!file,1,id,shell?FileMetadataRules1718.SystemShellBytes:(file?4096UL:0),10,20,
     shell?32u:(file?0u:16u),shell?2u:1u,shell?FileMetadataRules1718.SystemShellSha:(file?new string((char)('a'+role),64):null));
   }
   return new RootSelection(install,new string('1',32),RootSelection.FrozenInventory,1000,pins);
  }
  public static void default_unbound_denies_before_installing_or_attempting_owner() {
   CaptureObservation r=RootPPrebirth.Capture(null);
   Assert(r.Status=="UNBOUND_NOT_EXECUTABLE_78"&&!r.CreateWasAttempted&&!r.CreateReturned&&r.ProcessDword==null&&r.Slots==null);
  }
  static RootSelection ChangedShell(RootSelection s,ulong bytes,uint attrs,uint links,string digest){
   FilePin[] pins=(FilePin[])s.Pins.Clone();int i=pins.Length-1;FilePin p=pins[i];
   pins[i]=new FilePin(p.Path,false,p.Volume,p.FileId,bytes,p.CreationFileTime,p.ModifiedFileTime,attrs,links,digest);
   return new RootSelection(s.Install,s.Tag,s.Inventory,s.DeadlineMs,pins);
  }
  public static void fixed_shell_selector_requires_full_digest_size_attributes_and_two_links(){
   RootSelection s=Selection();s.Validate();
   Reject(delegate{ChangedShell(s,450560,32,1,FileMetadataRules1718.SystemShellSha).Validate();});
   Reject(delegate{ChangedShell(s,450560,32,3,FileMetadataRules1718.SystemShellSha).Validate();});
   Reject(delegate{ChangedShell(s,450561,32,2,FileMetadataRules1718.SystemShellSha).Validate();});
   Reject(delegate{ChangedShell(s,450560,34,2,FileMetadataRules1718.SystemShellSha).Validate();});
   Reject(delegate{ChangedShell(s,450560,32,2,new string('a',64)).Validate();});
  }
  public static void unverified_shell_shape_cannot_become_a_complete_selector(){
   RootSelection s=Selection();
   foreach(string digest in new string[]{null,""}){
    RootSelection raw=ChangedShell(s,450560,32,2,digest);
    Assert(FileMetadataRules1718.ObservedLinks(raw.Pins[raw.Pins.Length-1]));
    Assert(!FileMetadataRules1718.BoundLinks(raw.Pins[raw.Pins.Length-1]));Reject(delegate{raw.Validate();});
   }
  }
  public static void other_role_and_directory_two_links_fail_complete_selector(){
   RootSelection s=Selection();
   foreach(bool directory in new bool[]{true,false}){
    FilePin[] pins=(FilePin[])s.Pins.Clone();int i=0;while(pins[i].Directory!=directory)i++;
    FilePin p=pins[i];pins[i]=new FilePin(p.Path,p.Directory,p.Volume,p.FileId,p.Bytes,p.CreationFileTime,p.ModifiedFileTime,p.Attributes,2,p.Sha256);
    Reject(delegate{new RootSelection(s.Install,s.Tag,s.Inventory,s.DeadlineMs,pins).Validate();});
   }
  }
  public static void false_partial_pi_and_unknown_return_preserve_both_original_values() {
   RootPPrebirth.Native.PI pi=new RootPPrebirth.Native.PI{Process=new IntPtr(101),Thread=new IntPtr(102),Pid=7,Tid=8};
   Assert(!Rules.Pi(false,pi.Process,pi.Thread,pi.Pid,pi.Tid));
   Slot p=new Slot("P"),t=new Slot("T");p.Begin();t.Begin();
   p.Capture(pi.Process,false,5);t.Capture(pi.Thread,false,5);
   Assert(p.State==ResourceState.RetainedFailure&&t.State==ResourceState.RetainedFailure&&p.Raw==pi.Process&&t.Raw==pi.Thread&&p.Returned==false&&p.LastError==5);
   Slot uncertain=new Slot("UNKNOWN");uncertain.Begin();uncertain.CaptureUnknown(IntPtr.Zero);
   Assert(uncertain.State==ResourceState.RetainedFailure&&uncertain.Returned==null&&uncertain.State!=ResourceState.ProvenAbsent);
   Assert(!Rules.Pi(true,pi.Process,IntPtr.Zero,7,8)&&!Rules.Pi(true,pi.Process,pi.Process,7,8));
  }
  public static void failed_alias_close_retains_value_error_and_forbids_second_close() {
   Slot s=new Slot("ORIGINAL");s.Begin();s.Capture(new IntPtr(77),true,0);s.BeginClose();
   Assert(!s.Closed(false,6)&&s.Raw==new IntPtr(77)&&s.State==ResourceState.CloseFailed&&s.LastError==6&&s.Returned==false);
   Reject(delegate{s.BeginClose();});Reject(delegate{s.Begin();});
  }
  public static void successful_close_keeps_historical_value_but_never_reopens_it() {
   Slot s=new Slot("ORIGINAL");s.Begin();s.Capture(new IntPtr(77),true,0);s.BeginClose();
   Assert(s.Closed(true,0)&&s.Raw==new IntPtr(77)&&s.State==ResourceState.ClosedVerified);
   Reject(delegate{s.BeginClose();});Reject(delegate{s.Capture(new IntPtr(88),true,0);});
  }
  public static void thread_token_absence_requires_actual_error_no_token_and_zero_output() {
   Slot none=new Slot("NONE");none.Begin();none.Absent(false,1008);Assert(none.State==ResourceState.ProvenAbsent);
   Slot denied=new Slot("DENIED");denied.Begin();Reject(delegate{denied.Absent(false,5);});Assert(denied.State!=ResourceState.ProvenAbsent);
   Slot partial=new Slot("PARTIAL");partial.Begin();partial.Raw=new IntPtr(99);
   Reject(delegate{partial.Absent(false,1008);});Assert(partial.Raw==new IntPtr(99)&&partial.State==ResourceState.RetainedFailure);
  }
  public static void expiry_is_strict_and_does_not_retire_an_open_original() {
   Assert(Rules.Budget(0,1)&&!Rules.Budget(1,1)&&!Rules.Budget(-1,10)&&!Rules.Budget(0,5500001));
   Slot s=new Slot("OPEN");s.Begin();s.Capture(new IntPtr(42),true,0);
   Assert(!Rules.Budget(5500000,5500000)&&s.State==ResourceState.Open&&s.Raw==new IntPtr(42));
  }
  public static void logical_exit_zero_and_missing_eof_or_native_signal_never_complete() {
   Completion f=new Completion{ProcessCode=0,PrimaryCode=0,OutReaderCode=0,ErrReaderCode=0};Assert(!f.Ready());
   f=Complete();Assert(f.Ready());
   f.OutEof=false;Assert(!f.Ready());f=Complete();f.ErrEof=false;Assert(!f.Ready());
   f=Complete();f.OutReaderSignal=false;Assert(!f.Ready());f=Complete();f.ErrReaderSignal=false;Assert(!f.Ready());
   f=Complete();f.PrimarySignal=false;Assert(!f.Ready());f=Complete();f.ProcessSignal=false;Assert(!f.Ready());
   f=Complete();f.TokenWindow=false;Assert(!f.Ready());f=Complete();f.Identity=false;Assert(!f.Ready());f=Complete();f.ResumedOnce=false;Assert(!f.Ready());
  }
  public static void full_dword_is_unsigned_observed_only_and_never_business_exit() {
   DwordQuery q=new DwordQuery{Raw=0xc0000142,Returned=false,Error=5,AfterSignal=true};
   Assert(q.Raw==3221225794u&&q.ValidValue==null);
   q.Returned=true;Assert(q.ValidValue==3221225794u);q.AfterSignal=false;Assert(q.ValidValue==null);
   Completion f=Complete();f.ProcessCode=0xc0000142;Assert(!f.Ready());f=Complete();f.PrimaryCode=null;Assert(!f.Ready());
   f=Complete();f.OutReaderCode=7;Assert(!f.Ready());f=Complete();f.ErrReaderCode=259;Assert(!f.Ready());
  }
  public static void primary_statistics_or_default_policy_drift_is_not_equal() {
   RootPPrebirth.Subject a=new RootPPrebirth.Subject{Stats=new byte[]{1,2,3},Body=new byte[]{4,5}};
   RootPPrebirth.Subject same=new RootPPrebirth.Subject{Stats=new byte[]{1,2,3},Body=new byte[]{4,5}};
   Assert(a.Same(same));same.Stats[2]=9;Assert(!a.Same(same));same.Stats[2]=3;same.Body[1]=9;Assert(!a.Same(same));
  }
  public static void unsafe_null_broad_inherited_or_owner_rights_default_acl_is_rejected() {
   byte[] user=User(),logon=Logon();Assert(Rules.SafeDefault(Acl(user,0x1fffff,0,0),user,user,logon));
   Assert(!Rules.SafeDefault(null,user,user,logon));
   Assert(!Rules.SafeDefault(Acl(Sid(0),0x1fffff,0,0),user,user,logon));
   byte[] everyone=new byte[]{1,1,0,0,0,0,0,1,0,0,0,0};
   Assert(!Rules.SafeDefault(Acl(everyone,0x1fffff,0,0),user,user,logon));
   Assert(!Rules.SafeDefault(Acl(user,0x1fffff,0,2),user,user,logon));
   Assert(!Rules.SafeDefault(Acl(Rules.OwnerRights,0x1fffff,0,0),user,user,logon));
   Assert(!Rules.SafeDefault(Acl(user,0x1fffff,5,0),user,user,logon));
   Assert(!Rules.SafeDefault(Acl(user,0x1fffff,0,0),user,logon,logon));
  }
  public static void malformed_acl_lengths_sid_ranges_and_ace_counts_fail_closed() {
   byte[] user=User(),a=Acl(user,0x1fffff,0,0);
   a[4]=2;Assert(!Rules.SafeDefault(a,user,user,Logon()));
   a=Acl(user,0x1fffff,0,0);a[10]=4;Assert(!Rules.SafeDefault(a,user,user,Logon()));
   a=Acl(user,0x1fffff,0,0);a[17]=15;Assert(!Rules.SafeDefault(a,user,user,Logon()));
   a=Acl(user,0x1fffff,0,0);a[2]--;Assert(!Rules.SafeDefault(a,user,user,Logon()));
  }
  public static void token_pointer_parser_requires_in_buffer_aligned_complete_sid() {
   byte[] user=User(),b=new byte[16+user.Length];
   Buffer.BlockCopy(BitConverter.GetBytes(1016L),0,b,0,8);Buffer.BlockCopy(user,0,b,16,user.Length);
   RootPPrebirth.Blob blob=new RootPPrebirth.Blob(b,1000);Assert(Rules.Equal(blob.PointerSid(0,16),user));
   Buffer.BlockCopy(BitConverter.GetBytes(1008L),0,b,0,8);Reject(delegate{blob.PointerSid(0,16);});
   Buffer.BlockCopy(BitConverter.GetBytes(1017L),0,b,0,8);Reject(delegate{blob.PointerSid(0,16);});
   Buffer.BlockCopy(BitConverter.GetBytes(100000L),0,b,0,8);Reject(delegate{blob.PointerSid(0,16);});
  }
  public static void canonical_fixed_role_selection_does_not_accept_extra_paths_or_commands() {
   RootSelection s=Selection();s.Validate();string command=s.CommandLine();
   Assert(command.StartsWith("\"C:\\Morrow-C28-RootP1682-bin-"+new string('1',32)+"\\morrow-workbench-supervisor.exe\" --c28-parent-launch-p1 ",StringComparison.Ordinal));
   Assert(command.EndsWith(" "+new string('1',32),StringComparison.Ordinal));
   FilePin[] missing=new FilePin[s.Pins.Length-1];Array.Copy(s.Pins,missing,missing.Length);
   Reject(delegate{new RootSelection(s.Install,s.Tag,s.Inventory,1000,missing).Validate();});
   FilePin[] changed=(FilePin[])s.Pins.Clone();FilePin p=changed[changed.Length-1];
   changed[changed.Length-1]=new FilePin(@"C:\other.exe",false,p.Volume,p.FileId,p.Bytes,p.CreationFileTime,p.ModifiedFileTime,p.Attributes,p.Links,p.Sha256);
   Reject(delegate{new RootSelection(s.Install,s.Tag,s.Inventory,1000,changed).Validate();});
   Reject(delegate{Rules.LocalPath(@"C:\good\..\other");});Reject(delegate{Rules.LocalPath(@"C:\good\NUL.txt");});
   Reject(delegate{Rules.LocalPath(@"C:\good\image.exe:stream");});Reject(delegate{Rules.LocalPath(@"\\server\share");});
  }
  public static void protected_install_directory_rejects_untrusted_child_or_dacl_writer() {
   byte[] user=User(),everyone=new byte[]{1,1,0,0,0,0,0,1,0,0,0,0};
   Assert(Rules.SafeInstall(Acl(user,0x1fffff,0,3),user,user,Logon(),true));
   Assert(!Rules.SafeInstall(Acl(user,0x1fffff,0,3),user,user,Logon(),false));
   Assert(!Rules.SafeInstall(Acl(everyone,2,0,3),user,user,Logon(),true));
   Assert(!Rules.SafeInstall(Acl(everyone,0x40000,0,0),user,user,Logon(),true));
   Assert(Rules.SafeInstall(Acl(everyone,0x1200a9,0,3),user,user,Logon(),true));
   Assert(!Rules.SafeInstall(Acl(user,0x1fffff,0,0),user,everyone,Logon(),true));
  }
  public static void fixed_namespace_and_fresh_nonzero_tag_cannot_select_credential_or_foreign_root() {
   RootSelection s=Selection();
   Reject(delegate{new RootSelection(@"C:\other",s.Tag,s.Inventory,1000,s.Pins).Validate();});
   Reject(delegate{new RootSelection(s.Install,new string('0',32),s.Inventory,1000,s.Pins).Validate();});
   Reject(delegate{Rules.LocalPath(@"C:\Morrow-C28-RootP1682-bin-test\user.txt");});
  }
  public static void first_error_is_sticky_and_empty_resources_are_not_a_native_close_witness() {
   RootSelection p=new RootSelection(@"C:\Morrow-C28-RootP1682-bin-"+new string('1',32),new string('1',32),RootSelection.FrozenInventory,1000,new FilePin[0]);
   RootPPrebirth.Owner owner=new RootPPrebirth.Owner(p); // allocation-only; NOT Run
   owner.Fail("ORIGINAL_FIRST");owner.Fail("LATER");
   CaptureObservation r=owner.Snapshot("UNKNOWN_ORIGINALS_RETAINED_NO_REPLAY");
   Assert(r.FirstError=="ORIGINAL_FIRST"&&r.NoReplay&&r.ProcessDword==null&&!r.OriginalReadersSignaled&&!r.CreateWasAttempted);
  }
 }
}
