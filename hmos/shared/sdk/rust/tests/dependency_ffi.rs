use morrow_plugin_sdk::dependency_call::Request;
use std::ffi::c_void;
#[repr(C)]
#[derive(Clone, Copy)]
struct Span {
    data: *const u8,
    length: u32,
}
fn span(bytes: &[u8]) -> Span {
    Span {
        data: bytes.as_ptr(),
        length: bytes.len() as u32,
    }
}
#[repr(C)]
struct Descriptor {
    abi: u32,
    size: u32,
    call_id: Span,
    slot: Span,
    input: Span,
}
#[repr(C)]
struct View {
    output_type: Span,
    bytes: Span,
}
unsafe extern "C" {
    fn mp_dependency_request_encode(
        raw: *const Descriptor,
        out: *mut u8,
        capacity: u32,
        length: *mut u32,
    ) -> u32;
    fn mp_dependency_schema_digest(out: *mut u8, capacity: u32) -> u32;
    fn mp_dependency_response_decode(
        bytes: *const u8,
        length: u32,
        request: *const u8,
        request_length: u32,
        out: *mut *mut c_void,
    ) -> u32;
    fn mp_dependency_output_get(raw: *const c_void, out: *mut View, size: u32) -> u32;
    fn mp_dependency_output_free(raw: *mut c_void);
}
fn descriptor() -> Descriptor {
    Descriptor {
        abi: 1,
        size: size_of::<Descriptor>() as u32,
        call_id: span(b"call1"),
        slot: span(b"reverse"),
        input: span(b"abc"),
    }
}
#[test]
fn c_request_encoding_matches_rust_and_bounds_fail_without_output_mutation() {
    let mut out = vec![0xabu8; 131072];
    let mut length = 99;
    let expected = Request::new("call1", "reverse", b"abc").unwrap();
    unsafe {
        assert_eq!(
            mp_dependency_request_encode(&descriptor(), out.as_mut_ptr(), 131072, &mut length),
            0
        );
        assert_eq!(&out[..length as usize], expected.bytes());
        out.fill(0xab);
        assert_eq!(
            mp_dependency_request_encode(&descriptor(), out.as_mut_ptr(), 1, &mut length),
            17
        );
        assert_eq!(length, 0);
        assert!(out.iter().all(|b| *b == 0xab));
        for case in 0..5 {
            let mut raw = descriptor();
            match case {
                0 => raw.abi = 2,
                1 => raw.size = 0,
                2 => raw.input.length = 65537,
                3 => raw.input.length = 0,
                _ => raw.input.data = std::ptr::null(),
            }
            assert_ne!(
                mp_dependency_request_encode(&raw, out.as_mut_ptr(), 131072, &mut length),
                0
            );
            assert_eq!(length, 0);
            assert!(out.iter().all(|b| *b == 0xab));
        }
    }
}
#[test]
fn c_output_owns_data_and_verifies_request_correlation() {
    let request = Request::new("call1", "reverse", b"abc").unwrap();
    let original = request.encode_response("bytes", b"def").unwrap();
    let mut bytes = original.clone();
    let mut handle = std::ptr::null_mut();
    let mut view = View {
        output_type: span(b""),
        bytes: span(b""),
    };
    unsafe {
        assert_eq!(
            mp_dependency_response_decode(
                bytes.as_ptr(),
                bytes.len() as u32,
                request.bytes().as_ptr(),
                request.bytes().len() as u32,
                &mut handle
            ),
            0
        );
        bytes.fill(0);
        assert_eq!(
            mp_dependency_output_get(handle, &mut view, size_of::<View>() as u32),
            0
        );
        assert_eq!(
            std::slice::from_raw_parts(view.output_type.data, view.output_type.length as usize),
            b"bytes"
        );
        assert_eq!(
            std::slice::from_raw_parts(view.bytes.data, view.bytes.length as usize),
            b"def"
        );
        mp_dependency_output_free(handle);
        for case in 0..3 {
            let raw = match case {
                0 => Request::new("other", "reverse", b"abc"),
                1 => Request::new("call1", "other", b"abc"),
                _ => Request::new("call1", "reverse", b"abd"),
            }
            .unwrap();
            assert_eq!(
                mp_dependency_response_decode(
                    original.as_ptr(),
                    original.len() as u32,
                    raw.bytes().as_ptr(),
                    raw.bytes().len() as u32,
                    &mut handle
                ),
                19
            );
            assert!(handle.is_null());
        }
        assert_ne!(
            mp_dependency_response_decode(
                original.as_ptr(),
                1,
                request.bytes().as_ptr(),
                request.bytes().len() as u32,
                &mut handle
            ),
            0
        );
        assert!(handle.is_null());
        assert_eq!(
            mp_dependency_response_decode(
                original.as_ptr(),
                131073,
                request.bytes().as_ptr(),
                request.bytes().len() as u32,
                &mut handle
            ),
            17
        );
        assert_eq!(
            mp_dependency_output_get(handle, &mut view, size_of::<View>() as u32),
            16
        );
        assert!(view.output_type.data.is_null());
        assert_eq!(view.bytes.length, 0);
        mp_dependency_output_free(handle);
    }
}
#[test]
fn c_empty_result_and_digest_are_supported() {
    let request = Request::new("call1", "reverse", b"abc").unwrap();
    let bytes = request.encode_response("bytes", b"").unwrap();
    let mut handle = std::ptr::null_mut();
    let mut view = View {
        output_type: span(b""),
        bytes: span(b""),
    };
    let mut digest = [0xabu8; 32];
    unsafe {
        assert_eq!(mp_dependency_schema_digest(digest.as_mut_ptr(), 31), 17);
        assert_eq!(digest, [0xab; 32]);
        assert_eq!(mp_dependency_schema_digest(digest.as_mut_ptr(), 32), 0);
        assert_eq!(digest, morrow_plugin_sdk::dependency_call::schema_digest());
        assert_eq!(
            mp_dependency_response_decode(
                bytes.as_ptr(),
                bytes.len() as u32,
                request.bytes().as_ptr(),
                request.bytes().len() as u32,
                &mut handle
            ),
            0
        );
        assert_eq!(
            mp_dependency_output_get(handle, &mut view, size_of::<View>() as u32),
            0
        );
        assert_eq!(view.bytes.length, 0);
        mp_dependency_output_free(handle);
    }
}
#[test]
fn write_explicit_smoke_fixture_when_requested() {
    // Optional generated build artifacts only; does not modify tracked fixtures.
    if let Ok(path) = std::env::var("MORROW_DEPENDENCY_SMOKE_DIR") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        let request = Request::new("call1", "reverse", b"abc").unwrap();
        std::fs::write(path.join("request.capnp"), request.bytes()).unwrap();
        std::fs::write(
            path.join("response.capnp"),
            request.encode_response("bytes", b"def").unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn c_response_binds_exact_original_frame_not_reencoded_fields() {
    let canonical = Request::new("call1", "reverse", b"abc").unwrap();
    let mut original = canonical.bytes().to_vec();
    let words = u32::from_le_bytes(original[4..8].try_into().unwrap());
    original[4..8].copy_from_slice(&(words + 1).to_le_bytes());
    original.extend([0; 8]);
    let request = Request::decode(&original).unwrap();
    let response = request.encode_response("bytes", b"def").unwrap();
    let mut handle = std::ptr::null_mut();
    unsafe {
        assert_eq!(
            mp_dependency_response_decode(
                response.as_ptr(),
                response.len() as u32,
                original.as_ptr(),
                original.len() as u32,
                &mut handle
            ),
            0
        );
        mp_dependency_output_free(handle);
        assert_eq!(
            mp_dependency_response_decode(
                response.as_ptr(),
                response.len() as u32,
                canonical.bytes().as_ptr(),
                canonical.bytes().len() as u32,
                &mut handle
            ),
            19
        );
        assert!(handle.is_null());
        assert_eq!(
            mp_dependency_response_decode(
                response.as_ptr(),
                response.len() as u32,
                original.as_ptr(),
                131073,
                &mut handle
            ),
            17
        );
        assert!(handle.is_null());
    }
}
