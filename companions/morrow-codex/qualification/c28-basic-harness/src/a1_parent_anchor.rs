//! C-only build input. ROOT builds/pins existing P FIRST, then fills this one
//! digest in C's isolated source, then freezes C/R, then fills S's C/R table.
//! This file is not part of P/R library graphs. Zero denies before native use.
//! No runtime argv/JSON/hash is an alternative source of parent authority.
pub(crate) const P_IMAGE_SHA256:[u8;32]=[0;32];
