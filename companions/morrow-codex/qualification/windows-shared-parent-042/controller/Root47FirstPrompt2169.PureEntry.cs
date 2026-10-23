// SOURCE2169 independent exact pure selector. No operational controller call.
// Original Root47-QA1855.cs and its73 selectors/assertions remain unchanged.
using System;
using System.Collections.Generic;
using System.Reflection;
namespace C28.RootP1682 {
 public static class FirstPromptPureEntry2169 {
  static string Name(MethodInfo method){return "RootFirstPrompt2169::FirstPromptPureTests2169::"+method.Name;}
  static List<MethodInfo> Methods(){
   List<MethodInfo> methods=new List<MethodInfo>(typeof(FirstPromptPureTests2169).GetMethods(
    BindingFlags.Public|BindingFlags.Static|BindingFlags.DeclaredOnly));
   if(methods.Count!=12)throw new InvalidOperationException("FIRST_PROMPT_EXACT_COUNT");
   foreach(MethodInfo method in methods)if(method.ReturnType!=typeof(void)||method.GetParameters().Length!=0)
    throw new InvalidOperationException("FIRST_PROMPT_PURE_SIGNATURE");
   methods.Sort(delegate(MethodInfo a,MethodInfo b){return StringComparer.Ordinal.Compare(Name(a),Name(b));});
   for(int i=1;i<methods.Count;i++)if(Name(methods[i-1])==Name(methods[i]))
    throw new InvalidOperationException("FIRST_PROMPT_DUPLICATE_CASE");
   return methods;
  }
  public static int Main(string[] args){
   if(args==null)return 78;
   bool list=args.Length==3&&args[0]=="--list"&&args[1]=="--format"&&args[2]=="terse";
   bool exact=args.Length==4&&args[1]=="--exact"&&args[2]=="--nocapture"&&args[3]=="--test-threads=1";
   if(!list&&!exact)return 78;
   try{
    List<MethodInfo> methods=Methods();
    if(list){foreach(MethodInfo method in methods)Console.WriteLine(Name(method)+": test");return 0;}
    MethodInfo selected=null;foreach(MethodInfo method in methods)if(args[0]==Name(method))selected=method;
    if(selected==null)return 78;
    Console.WriteLine("running 1 test");bool pass=false;
    try{selected.Invoke(null,null);pass=true;}
    catch(TargetInvocationException){Console.Error.WriteLine("FIRST_PROMPT_PURE_FAILURE");}
    catch(Exception){Console.Error.WriteLine("FIRST_PROMPT_PURE_FAILURE");}
    Console.WriteLine("test "+args[0]+" ... "+(pass?"ok":"FAILED"));
    Console.WriteLine("test result: "+(pass?"ok. 1 passed; 0 failed":"FAILED. 0 passed; 1 failed")
     +"; 0 ignored; 0 measured; "+(methods.Count-1)+" filtered out;");
    return pass?0:101;
   }catch(Exception){Console.Error.WriteLine("FIRST_PROMPT_SELECTOR_FAILURE");return 78;}
  }
 }
}
