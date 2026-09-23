@0xc37bfe813a120b94;
# Runtime task profile v3. Correlation does not confer authority.
enum Kind { contentCommand @0; transform @1; }
struct Transform {
  handler @0 :Text;
  inputType @1 :Text;
  outputType @2 :Text;
  input @3 :Data;
}
enum FailureCode { invalidInput @0; unsupportedInput @1; resourceLimit @2; failed @3; }
struct Failure { code @0 :FailureCode; message @1 :Text; }
struct Output { typeId @0 :Text; bytes @1 :Data; }
struct Invocation {
  version @0 :UInt16;
  schemaDigest @1 :Data;
  taskId @2 :Text;
  command @3 :Data;
  kind @4 :Kind;
  transform @5 :Transform;
}
struct Completion {
  version @0 :UInt16;
  schemaDigest @1 :Data;
  taskId @2 :Text;
  inputDigest @3 :Data;
  response @4 :Data;
  kind @5 :Kind;
  output @6 :Output;
  failure @7 :Failure;
}
