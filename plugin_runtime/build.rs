fn main() {
    println!("cargo:rerun-if-changed=src/directory_io/native_picker.cpp");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var_os("CARGO_FEATURE_PACKAGES").is_some()
    {
        cc::Build::new()
            .cpp(true)
            .file("src/directory_io/native_picker.cpp")
            .define("UNICODE", None)
            .define("_UNICODE", None)
            .define("WIN32_LEAN_AND_MEAN", None)
            .flag_if_supported("/std:c++17")
            .flag_if_supported("/EHsc")
            .warnings_into_errors(true)
            .compile("morrow_native_directory_picker");
        for library in ["ole32", "shell32", "uuid", "user32"] {
            println!("cargo:rustc-link-lib={library}");
        }
    }
}
