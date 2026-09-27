# Mutation v1 codec vectors

Experimental independent extension, not a frozen compatibility profile.
Frames were produced with the Cap'n Proto command-line encoder from the exact
schema digest in manifest.json; bad-trailing appends one unused word.
Both independent Core and Rust SDK codecs must consume these same bytes.
Every response is correlated to execute.bin. Valid frames are not authority,
platform support, owner routing, Wasm import or permission to execute.

Common values: call 7, reference 32 bytes 0x11, submission 32 bytes 0x22,
operation mutation-vector-1, deadline 30000 ms. Chunk is 00 01 ff;
create-content binds its length and SHA-256. create-empty binds SHA-256(empty).
The manifest identifies positive and rejected vectors and pins exact bytes.
