fn main() {
    #[cfg(feature = "managed-channel")]
    {
        println!("cargo:rerun-if-changed=schemas/sse_event.capnp");
        capnpc::CompilerCommand::new()
            .src_prefix("schemas")
            .file("schemas/sse_event.capnp")
            .run()
            .expect("managed-channel event schema compiler");
        #[cfg(feature = "managed-websocket")]
        {
            println!("cargo:rerun-if-changed=schemas/ws_message.capnp");
            capnpc::CompilerCommand::new()
                .src_prefix("schemas")
                .file("schemas/ws_message.capnp")
                .run()
                .expect("managed-websocket message schema compiler");
        }
    }
}
