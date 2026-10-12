@0xe82c6fb457dc901b;
# Private pure observation on the current original Workbench only.
# No caller-selected task, grant, HostBinding, endpoint, Start or Attach.
# runningAgent means a worker is retained, NOT proof of native execution/join.
enum Kind {
 none @0;
 runningAgent @1;
 retainedAgentExit @2;
 otherTask @3;
 unavailable @4;
}
struct Request {
 version @0 :UInt16;
 digest @1 :Data;
 id @2 :Data;
}
struct Reply {
 version @0 :UInt16;
 digest @1 :Data;
 id @2 :Data;
 requestSha256 @3 :Data;
 kind @4 :Kind;
 # Exact random StateSlot key only for runningAgent/retainedAgentExit.
 task @5 :Data;
}
