// SOURCE1718 pure comparisons of real production metadata rules.
// Fixtures are data, not native observations/handle ownership or runtime PASS.
using System;
namespace C28.RootP1682 {
 public static class MetadataPureTests1718 {
  static void Assert(bool condition){if(!condition)throw new InvalidOperationException("METADATA_PURE_ASSERTION");}
  static FilePin Dir(ulong bytes,ulong modified){
   return new FilePin(@"C:\",true,3768066395919363068UL,"05000000000005000000000000000000",bytes,
    132963742424911263UL,modified,22,1,null);
  }
  static FilePin Pe(){
   return new FilePin(@"C:\Morrow-C28-RootP1701-bin-test\morrow-workbench-supervisor.exe",false,7,
    "01000000000000000000000000000000",4096,100,200,32,1,new string('a',64));
  }
  static FilePin Change(FilePin p,string path,bool directory,ulong volume,string id,ulong bytes,ulong created,ulong modified,uint attrs,uint links){
   return new FilePin(path,directory,volume,id,bytes,created,modified,attrs,links,p.Sha256);
  }
  static FilePin Shell(string digest){return new FilePin(RootPPrebirth.Shell,false,7,"01000000000000000000000000000000",450560,100,200,32,2,digest);}
  public static void fixed_shell_two_links_is_metadata_not_digest_admission(){
   FilePin raw=Shell(null);Assert(FileMetadataRules1718.Same(raw,Shell(null))&&FileMetadataRules1718.ObservedLinks(raw));
   Assert(!FileMetadataRules1718.BoundLinks(raw));
   FilePin verified=FileMetadataRules1718.WithDigest(raw,FileMetadataRules1718.SystemShellSha);
   Assert(FileMetadataRules1718.BoundLinks(verified)&&raw.Sha256==null&&verified.Links==2);
   Assert(FileMetadataRules1718.ReadOnlyShellOpen(raw.Path,false,0x80000000,1,3,false));
   Assert(!FileMetadataRules1718.ReadOnlyShellOpen(raw.Path,false,0xc0000000,1,3,false));
   Assert(!FileMetadataRules1718.ReadOnlyShellOpen(raw.Path,false,0x80000000,3,3,false));
   Assert(!FileMetadataRules1718.ReadOnlyShellOpen(raw.Path,false,0x80000000,1,1,false));
   Assert(!FileMetadataRules1718.ReadOnlyShellOpen(raw.Path,false,0x80000000,1,3,true));
  }
  public static void fixed_shell_wrong_digest_path_size_or_attributes_is_rejected(){
   FilePin p=Shell(FileMetadataRules1718.SystemShellSha),wrong=Shell(new string('a',64));
   Assert(!FileMetadataRules1718.Same(p,wrong)&&!FileMetadataRules1718.BoundLinks(wrong));
   Assert(!FileMetadataRules1718.Same(p,Change(p,p.Path+".other",false,p.Volume,p.FileId,p.Bytes,100,200,32,2)));
   Assert(!FileMetadataRules1718.Same(p,Change(p,p.Path,false,p.Volume,p.FileId,p.Bytes+1,100,200,32,2)));
   Assert(!FileMetadataRules1718.Same(p,Change(p,p.Path,false,p.Volume,p.FileId,p.Bytes,100,200,34,2)));
  }
  public static void fixed_shell_requires_exactly_two_links_and_rejects_link_drift(){
   FilePin p=Shell(FileMetadataRules1718.SystemShellSha);
   foreach(uint links in new uint[]{0,1,3,UInt32.MaxValue}){
    FilePin drift=Change(p,p.Path,false,p.Volume,p.FileId,p.Bytes,100,200,32,links);
    Assert(!FileMetadataRules1718.ObservedLinks(drift)&&!FileMetadataRules1718.Same(p,drift));
   }
  }
  public static void other_pe_and_directory_two_links_are_still_rejected(){
   foreach(FilePin p in new FilePin[]{Pe(),Dir(0,1)}){
    FilePin two=Change(p,p.Path,p.Directory,p.Volume,p.FileId,p.Bytes,p.CreationFileTime,p.ModifiedFileTime,p.Attributes,2);
    Assert(!FileMetadataRules1718.ObservedLinks(two)&&!FileMetadataRules1718.BoundLinks(two)&&!FileMetadataRules1718.Same(p,two));
   }
   FilePin shell=Shell(null);Assert(!FileMetadataRules1718.ObservedLinks(Change(shell,shell.Path,true,7,shell.FileId,450560,100,200,48,2)));
   FilePin journal=new FilePin(@"C:\Morrow-C28-A1-047-test\case.evidence\journal.jsonl",false,7,shell.FileId,32,100,200,32,2,null);
   Assert(!FileMetadataRules1718.ObservedLinks(journal)&&!FileMetadataRules1718.Same(journal,journal));
  }
  public static void fixed_shell_still_rejects_stable_identity_or_mtime_drift(){
   FilePin p=Shell(null);
   Assert(!FileMetadataRules1718.Same(p,Change(p,p.Path,false,8,p.FileId,450560,100,200,32,2)));
   Assert(!FileMetadataRules1718.Same(p,Change(p,p.Path,false,7,new string('2',32),450560,100,200,32,2)));
   Assert(!FileMetadataRules1718.Same(p,Change(p,p.Path,false,7,p.FileId,450560,101,200,32,2)));
   Assert(!FileMetadataRules1718.Same(p,Change(p,p.Path,false,7,p.FileId,450560,100,201,32,2)));
  }
  public static void shell_digest_attachment_preserves_actual_metadata_without_normalization(){
   FilePin raw=Shell(null),p=FileMetadataRules1718.WithDigest(raw,FileMetadataRules1718.SystemShellSha);
   Assert(p.Path==raw.Path&&p.Volume==raw.Volume&&p.FileId==raw.FileId&&p.Bytes==raw.Bytes
    &&p.CreationFileTime==raw.CreationFileTime&&p.ModifiedFileTime==raw.ModifiedFileTime&&p.Attributes==raw.Attributes&&p.Links==2);
   Assert(raw.Sha256==null&&FileMetadataRules1718.Same(raw,p));
  }
  public static void actual1717_directory_lastwrite_change_and_size_change_preserve_identity(){
   FilePin before=Dir(0,134367031539635747UL),after=Dir(4096,134367031542615190UL);
   Assert(FileMetadataRules1718.Same(before,after));
   Assert(before.ModifiedFileTime==134367031539635747UL&&after.ModifiedFileTime==134367031542615190UL);
   Assert(before.Bytes==0&&after.Bytes==4096);
  }
  public static void directory_content_fields_are_observed_not_normalized_or_identity_keys(){
   FilePin before=Dir(512,9),after=Dir(UInt64.MaxValue,0);
   Assert(FileMetadataRules1718.Same(before,after));
   Assert(after.Bytes==UInt64.MaxValue&&after.ModifiedFileTime==0&&before.Bytes==512&&before.ModifiedFileTime==9);
  }
  public static void ordinary_file_lastwrite_change_is_still_rejected(){
   FilePin p=Pe();FilePin changed=Change(p,p.Path,false,p.Volume,p.FileId,p.Bytes,p.CreationFileTime,p.ModifiedFileTime+1,p.Attributes,p.Links);
   Assert(!FileMetadataRules1718.Same(p,changed));
  }
  public static void ordinary_file_size_change_is_still_rejected(){
   FilePin p=Pe();Assert(!FileMetadataRules1718.Same(p,Change(p,p.Path,false,p.Volume,p.FileId,p.Bytes+1,p.CreationFileTime,p.ModifiedFileTime,p.Attributes,p.Links)));
  }
  public static void id_volume_or_creation_drift_is_rejected_for_directory_and_file(){
   foreach(FilePin p in new FilePin[]{Dir(0,1),Pe()}){
    Assert(!FileMetadataRules1718.Same(p,Change(p,p.Path,p.Directory,p.Volume+1,p.FileId,p.Bytes,p.CreationFileTime,p.ModifiedFileTime,p.Attributes,p.Links)));
    Assert(!FileMetadataRules1718.Same(p,Change(p,p.Path,p.Directory,p.Volume,"02000000000000000000000000000000",p.Bytes,p.CreationFileTime,p.ModifiedFileTime,p.Attributes,p.Links)));
    Assert(!FileMetadataRules1718.Same(p,Change(p,p.Path,p.Directory,p.Volume,p.FileId,p.Bytes,p.CreationFileTime+1,p.ModifiedFileTime,p.Attributes,p.Links)));
   }
  }
  public static void reparse_type_link_and_other_attribute_changes_are_rejected(){
   FilePin d=Dir(0,1),p=Pe();
   Assert(!FileMetadataRules1718.Same(d,Change(d,d.Path,true,d.Volume,d.FileId,d.Bytes,d.CreationFileTime,d.ModifiedFileTime,d.Attributes|0x400,1)));
   Assert(!FileMetadataRules1718.Same(d,Change(d,d.Path,false,d.Volume,d.FileId,d.Bytes,d.CreationFileTime,d.ModifiedFileTime,d.Attributes&~16u,1)));
   Assert(!FileMetadataRules1718.Same(d,Change(d,d.Path,true,d.Volume,d.FileId,d.Bytes,d.CreationFileTime,d.ModifiedFileTime,d.Attributes,2)));
   Assert(!FileMetadataRules1718.Same(d,Change(d,d.Path,true,d.Volume,d.FileId,d.Bytes,d.CreationFileTime,d.ModifiedFileTime,d.Attributes^2u,1)));
   Assert(!FileMetadataRules1718.Same(p,Change(p,p.Path,true,p.Volume,p.FileId,p.Bytes,p.CreationFileTime,p.ModifiedFileTime,p.Attributes|16,1)));
  }
  public static void path_change_rejects_and_windows_case_comparison_does_not_rewrite_observation(){
   FilePin p=Pe();FilePin other=Change(p,p.Path+".other",false,p.Volume,p.FileId,p.Bytes,p.CreationFileTime,p.ModifiedFileTime,p.Attributes,p.Links);
   Assert(!FileMetadataRules1718.Same(p,other));
   FilePin casing=Change(p,p.Path.ToLowerInvariant(),false,p.Volume,p.FileId,p.Bytes,p.CreationFileTime,p.ModifiedFileTime,p.Attributes,p.Links);
   Assert(FileMetadataRules1718.Same(p,casing)&&casing.Path==p.Path.ToLowerInvariant());
  }
  public static void null_or_unobserved_path_and_invalid_stable_fields_fail_closed(){
   FilePin d=Dir(0,1);Assert(!FileMetadataRules1718.Same(d,null)&&!FileMetadataRules1718.Same(null,d));
   Assert(!FileMetadataRules1718.Same(d,Change(d,null,true,d.Volume,d.FileId,0,d.CreationFileTime,2,d.Attributes,1)));
   Assert(!FileMetadataRules1718.Same(d,Change(d,d.Path,true,0,d.FileId,0,d.CreationFileTime,2,d.Attributes,1)));
   Assert(!FileMetadataRules1718.Same(d,Change(d,d.Path,true,d.Volume,new string('0',32),0,d.CreationFileTime,2,d.Attributes,1)));
   Assert(!FileMetadataRules1718.Same(d,Change(d,d.Path,true,d.Volume,d.FileId,0,0,2,d.Attributes,1)));
  }
  public static void equal_file_metadata_is_only_metadata_and_preserves_original_values(){
   FilePin p=Pe(),copy=Change(p,p.Path,false,p.Volume,p.FileId,p.Bytes,p.CreationFileTime,p.ModifiedFileTime,p.Attributes,p.Links);
   Assert(FileMetadataRules1718.Same(p,copy));Assert(p.Bytes==4096&&copy.ModifiedFileTime==200&&p.Sha256==new string('a',64));
   // Native caller must still hash/check exact PE bytes; no hash acceptance is returned here.
  }
  public static void final_digest_attachment_keeps_all_actual_final_metadata_not_initial_data(){
   FilePin final=Dir(8192,134367031542615190UL);
   FilePin data=FileMetadataRules1718.WithDigest(final,new string('b',64));
   Assert(data.Bytes==8192&&data.ModifiedFileTime==134367031542615190UL&&data.CreationFileTime==final.CreationFileTime
    &&data.Path==final.Path&&data.Volume==final.Volume&&data.FileId==final.FileId&&data.Attributes==final.Attributes&&data.Links==final.Links&&data.Sha256==new string('b',64));
   Assert(final.Sha256==null); // no mutation/replacement of the observed record
  }
 }
}
