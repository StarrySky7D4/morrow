using System;
using System.Collections.Generic;
using System.Reflection;
namespace C28.RootP1682 {
public static class RootPromptAllQA2187 {
 public static int Main(string[] args){
  if(args==null||args.Length!=1||args[0]!="--all-pure-prompt2187")return 78;
  try{
   Type[] types={typeof(PureTests1682),typeof(InputPureTests1701),typeof(MetadataPureTests1718),typeof(ControllerPureTests1725),typeof(FirstPromptPureTests2169)};
   int[] counts={19,17,16,21,12};List<MethodInfo> all=new List<MethodInfo>();
   for(int i=0;i<types.Length;i++){
    MethodInfo[] methods=types[i].GetMethods(BindingFlags.Public|BindingFlags.Static|BindingFlags.DeclaredOnly);
    if(methods.Length!=counts[i])return 78;
    foreach(MethodInfo m in methods){if(m.ReturnType!=typeof(void)||m.GetParameters().Length!=0)return 78;all.Add(m);}
   }
   all.Sort(delegate(MethodInfo a,MethodInfo b){return StringComparer.Ordinal.Compare(a.DeclaringType.Name+"::"+a.Name,b.DeclaringType.Name+"::"+b.Name);});
   int passed=0;
   foreach(MethodInfo m in all){try{m.Invoke(null,null);}catch(Exception){Console.Error.WriteLine("ROOT_PURE2187_ASSERTION_FAILED");return 101;}
    Console.WriteLine("PASS RootPromptAll2187::"+m.DeclaringType.Name+"::"+m.Name);passed++;}
   Console.WriteLine("PURE_PASSED="+passed);return passed==85?0:101;
  }catch(Exception){Console.Error.WriteLine("ROOT_PURE2187_SELECTOR_FAILED");return 78;}
 }
}}
