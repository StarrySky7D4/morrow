//! Original transport containers must retain exactly the captured guest modules.
#![cfg(feature = "packages")]
use morrow_core::plugin_package::Package;
#[test]
fn six_original_transport_packages_keep_modules_profiles_and_bounded_budgets() {
    let root = std::env::var_os("MORROW_TRANSPORT_BASELINE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../sdk/compat/transport-v1-rc1")
        });
    for language in ["rust", "c", "cpp"] {
        for kind in ["io", "service"] {
            let stem = format!("{language}-{kind}");
            let original = std::fs::read(root.join(format!("{stem}.mplugin"))).unwrap();
            let package = Package::decode(&original).unwrap();
            assert_eq!(
                package.module(),
                std::fs::read(root.join(format!("{stem}.wasm"))).unwrap()
            );
            let declaration = package.io_declaration().unwrap();
            assert_eq!(declaration.budget.as_ref().unwrap().max_jobs, 1);
            assert_eq!(
                declaration.budget.as_ref().unwrap().max_job_bytes,
                1024 * 1024
            );
            if kind == "service" {
                let run = declaration.service_run.as_ref().unwrap();
                assert_eq!(run.max_duration_ms, 120_000);
                assert_eq!(run.budget.as_ref().unwrap().max_jobs, 16);
                assert_eq!(run.budget.as_ref().unwrap().max_bytes, 4 * 1024 * 1024);
            } else {
                assert!(declaration.service_run.is_none());
                assert!(
                    declaration
                        .handlers
                        .iter()
                        .any(|h| h == "morrow.http.forward.v1")
                );
            }
        }
    }
}
