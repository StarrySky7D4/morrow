@0xb97d2f6a4138e0c5;
# Private trusted UI protocol for versioned Morrow content. Outer host action
# owns card identity, operation, revision and paging. No source body or clock.
enum TaskAction { setCompletion @0; rename @1; reorder @2; setStage @3; completeAllAndSetStage @4; add @5; remove @6; }
struct TaskEdit { version @0 :UInt16; digest @1 :Data; action @2 :TaskAction; taskId @3 :Text; text @4 :Text; complete @5 :Bool; order @6 :List(Text); }
enum CardAction { edit @0; setFavorite @1; setCategory @2; delete @3; restore @4; }
struct Asset { id @0 :Text; name @1 :Text; kind @2 :Text; bytes @3 :UInt64; }
struct CardFields { title @0 :Text; description @1 :Text; hypothesis @2 :Text; conclusion @3 :Text; icon @4 :UInt16; color @5 :UInt32; assets @6 :List(Asset); }
struct CardEdit { version @0 :UInt16; digest @1 :Data; action @2 :CardAction; fields @3 :CardFields; favorite @4 :Bool; category @5 :Text; stage @6 :Text; }
struct Query { version @0 :UInt16; digest @1 :Data; section @2 :Text; filter @3 :Text; text @4 :Text; sort @5 :Text; }
struct Task { id @0 :Text; text @1 :Text; completion @2 :UInt16; legacyCompleted @3 :Bool; legacyDuplicates @4 :UInt32; }
struct Mapping { sourceIndex @0 :UInt32; taskId @1 :Text; }
struct Origin { cardId @0 :Text; sourceRevision @1 :UInt64; sourceSha256 @2 :Data; migratorVersion @3 :UInt32; targetVersion @4 :UInt32; originalProperties @5 :Data; mapping @6 :List(Mapping); historicalProjectStage @7 :Text; originalTitle @8 :Text; }
struct Card { id @0 :Text; title @1 :Text; revision @2 :UInt64; formatVersion @3 :UInt32; description @4 :Text; category @5 :Text; stage @6 :Text; hypothesis @7 :Text; conclusion @8 :Text; favorite @9 :Bool; assets @10 :List(Asset); icon @11 :UInt16; color @12 :UInt32; deleted @13 :Bool; deletedAt @14 :UInt64; todos @15 :List(Text); completed @16 :List(Text); tasks @17 :List(Task); origin @18 :Origin; retiredTaskIds @19 :List(Text); projectedStage @20 :Text; completeCount @21 :UInt32; incompleteCount @22 :UInt32; ambiguousCount @23 :UInt32; }
enum EnvelopeKind { record @0; plan @1; commit @2; }
struct Envelope { version @0 :UInt16; digest @1 :Data; kind @2 :EnvelopeKind; record @3 :Card; id @4 :Text; operation @5 :Text; sourceRevision @6 :UInt64; revision @7 :UInt64; repeated @8 :Bool; }
