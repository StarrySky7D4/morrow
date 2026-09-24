@0xe6adc9648f88a297;
struct Appearance {
 theme @0 :Text; glass @1 :Text; background @2 :Text; solidTint @3 :UInt16;
 opacity @4 :Float64; cornerRadius @5 :Float64; windowRadius @6 :Float64;
 grayscale @7 :Float64; lightness @8 :Float64; hasLightness @9 :Bool;
 customColor @10 :UInt32; hasCustomColor @11 :Bool;
 themeColor @12 :UInt32; hasThemeColor @13 :Bool;
 liquidCanvas @14 :Bool; mediaPlaying @15 :Bool;
 sidebarExpanded @16 :Bool; appearanceExpanded @17 :Bool;
 canvasBlur @18 :Float64;
 canvasOpacity @19 :Float64;
 canvasColor @20 :UInt32;
 hasCanvasColor @21 :Bool;
 componentCustom @22 :Bool;
 componentBlur @23 :Float64 = 22;
 componentOpacity @24 :Float64 = 0.76;
 componentColor @25 :UInt32;
 hasComponentColor @26 :Bool;
 visualStyle @27 :Text;
 styleDepth @28 :Float64 = 1;
}
struct Playback {
 ids @0 :List(Text); index @1 :Int32; playing @2 :Bool; blocked @3 :Bool;
 positionMs @4 :UInt64; durationMs @5 :UInt64;
}
enum MusicAction { restore @0; select @1; next @2; previous @3; remove @4; toggle @5; block @6; seek @7; }
struct LyricLine { timeMs @0 :UInt64; text @1 :Text; }
struct LyricCandidate { title @0 :Text; artist @1 :Text; duration @2 :Float64; synced @3 :Bool; hasLyrics @4 :Bool; }
enum ServiceAction { appearance @0; lyrics @1; playback @2; importPolicy @3; lyricMatch @4; markdown @5; preferences @6; }
struct ServiceRequest {
 version @0 :UInt16; digest @1 :Data; action @2 :ServiceAction;
 appearance @3 :Appearance; playback @4 :Playback; musicAction @5 :MusicAction;
 value @6 :Int64; flag @7 :Bool; text @8 :Text; kind @9 :Text;
 size @10 :UInt64; title @11 :Text; artist @12 :Text; duration @13 :Float64;
 candidates @14 :List(LyricCandidate);
}
struct ServiceResponse {
 version @0 :UInt16; digest @1 :Data; appearance @2 :Appearance;
 playback @3 :Playback; lines @4 :List(LyricLine); text @5 :Text;
 matchIndex @6 :Int32; effect @7 :Text;
}

struct Source { location @0 :Text; name @1 :Text; kind @2 :Text; local @3 :Bool; }
struct Track { source @0 :Source; cover @1 :Source; coverPresent @2 :Bool; lyrics @3 :Text; lyricSource @4 :Text; title @5 :Text; artist @6 :Text; duration @7 :Float64; metadataRead @8 :Bool; }
struct Preferences {
 version @0 :UInt16; appearance @1 :Appearance; texture @2 :Source; texturePresent @3 :Bool;
 tracks @4 :List(Track); index @5 :Int32; showLyrics @6 :Bool; onlineLyrics @7 :Bool;
 completed @8 :List(Text); digest @9 :Data;
 components @10 :List(ComponentMaterial);
}

struct ComponentMaterial { id @0 :Text; enabled @1 :Bool; blur @2 :Float64; opacity @3 :Float64; color @4 :UInt32; hasColor @5 :Bool; mode @6 :Text; cornerRadius @7 :Float64; hasCornerRadius @8 :Bool; followComponent @9 :Text; styleDepth @10 :Float64; hasStyleDepth @11 :Bool; }
