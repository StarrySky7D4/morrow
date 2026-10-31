# Finite directory observations: experimental source

Version 1 uses Cap'n Proto schema `0xf114ca43af66b51f`, raw SHA256
`3312e334f5d298d3c56df9c06ed3ac940bbf8fc9a058d113ac30b0d6854f346f`.
The codec preserves raw single-component UTF-16, including unpaired surrogates.
Rename-old/new events are separate observations, never business mutation proof.
Gap, revoked, cancelled and failed terminals carry no partial changes or replay.

The local consumer charges one accepted frame before its exact sequence/digest
ACK. A wrong ACK retains pending state; malformed/correlation/budget/transport
failure retains Unknown and prior charges. ACK and disposal never close or
refund original native resources, create approval, or prove a worker join.

Related sources: `../fs-watch-ffi-v1` owns C/C++ interfaces;
`../fs-watch-native-v1` owns finite Windows observation resources;
`qualification` contains three raw-task Wasm consumers and a default Runner.

Pre-integration evidence is retained under review1418 (18 pure methods),
review1436 (six Windows fixtures, ten original resource closes), review1445
(MSVC/Clang C and C++ consumers), and review1455 (three-language Wasm execution).
Those receipts keep their original source hashes. This new repository layout
requires its own build and runtime evidence; it does not inherit a whole-source
or release pass merely because the implementations were copied.

No product package feature, FileWatch approval, guest watch import, protected
selection, original shared clock/budget, or formal UI route is enabled. Existing
FileList is not watch permission. Product cancellation, close and worker join
must be tied to the original owner before enabling a route. Actual native
overflow/close-failure and complete SDK/P2/P3 acceptance remain open.
Conditional Replace remains Unsupported; no business rename is implemented.
