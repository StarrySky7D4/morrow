@0xeefcf786d6838bda;
# Private trusted UI/host connection. Native selected paths never reach a guest.
enum Action { read @0; page @1; mutate @2; importFile @3; exportFile @4; service @5; query @6; readPreferences @7; savePreferences @8; capture @9; beginPreferences @10; appendPreferences @11; finishPreferences @12; abortPreferences @13; readPreferencesPart @14; backupProtection @15; backupSnapshot @16; }
struct Request {
 version @0 :UInt16; digest @1 :Data; action @2 :Action;
 id @3 :Text; operation @4 :Text; revision @5 :UInt64;
 payload @6 :Data; selectedPath @7 :Text; name @8 :Text; kind @9 :Text;
 cursor @10 :Text; limit @11 :UInt32; attachment @12 :Text;
 transfer @13 :Text; offset @14 :UInt64; totalLength @15 :UInt64; sha256 @16 :Data;
}
struct Response {
 version @0 :UInt16; digest @1 :Data; payload @2 :Data;
 revision @3 :UInt64; ids @4 :List(Text); cursor @5 :Text;
 readOnly @6 :Bool; error @7 :Text;
 transfer @8 :Text; offset @9 :UInt64; totalLength @10 :UInt64; sha256 @11 :Data;
 maintenanceWarning @12 :Text;
}
