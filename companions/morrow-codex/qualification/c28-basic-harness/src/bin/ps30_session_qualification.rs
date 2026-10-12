//! SOURCE ONLY. This target does not open a Store or enter a VM workflow.
//!
//! Existing live::Session owns its GuestLifecycle/Journal privately and routes
//! only the historical sealed workflow. Reusing begin()->interact() here would
//! run that historical lane, not the independent ProductSessionFlow. A new
//! retained carrier binding requires separate source review; no argument,
//! environment variable, receipt JSON or bool enables execution in this target.
#![forbid(unsafe_code)]

fn main() {
    eprintln!("{}", serde_json::json!({
        "schema":"STAGE39_PS30_SOURCE_ONLY_DISABLED_DRIVER_V1",
        "status":"DISABLED_EXIT78",
        "cargo_target":"ps30_session_qualification",
        "flow":"ProductSessionFlow",
        "carrier":"NOT_BOUND_TO_ACTUAL_GUEST_IDENTITY_JOURNAL_LIFECYCLE",
        "body_execution":"NOT_RUN",
        "sdk_gate":"SDK26_G04_OPEN",
        "release_eligible":false,
        "production_qualified":false
    }));
    std::process::exit(78);
}
