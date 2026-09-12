use morrow_core::plugin_package::{
    Package,
    proto::{Capability, TransformHandler},
};
use morrow_workbench_plugin::Idea;
pub fn package() -> Package {
    let path = std::env::var("MORROW_WORKBENCH_WASM")
        .expect("set MORROW_WORKBENCH_WASM to the actual compiled Rust guest");
    let module = std::fs::read(path).unwrap();
    let mut manifest = Package::manifest_for_transform(
        "org.morrow.workbench",
        "0.1.9-test.10",
        &module,
        vec![
            TransformHandler {
                handler: "studio.command".into(),
                input_type: "morrow.studio.request.v1".into(),
                output_type: "morrow.studio.response.v1".into(),
                max_input_bytes: 65536,
                max_output_bytes: 65536,
            },
            TransformHandler {
                handler: "workbench.command".into(),
                input_type: "morrow.workbench.request.v1".into(),
                output_type: "morrow.workbench.response.v1".into(),
                max_input_bytes: 65536,
                max_output_bytes: 65536,
            },
        ],
    );
    manifest.requested_capabilities = vec![
        Capability::CreateContent as i32,
        Capability::EditContent as i32,
        Capability::ReadContent as i32,
    ];
    Package::build(manifest, &module).unwrap()
}
pub fn idea(id: &str) -> Idea {
    Idea {
        id: id.into(),
        title: format!("记录 {id}"),
        description: "原样 **Markdown**".into(),
        category: "灵感".into(),
        stage: "待整理".into(),
        ..Default::default()
    }
}
