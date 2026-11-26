//! The `mock-agent` program: the scripted ACP agent of `lib.rs` on stdio, or with `replay` a
//! recorded session played back (`replay.rs`).

#[tokio::main]
async fn main() -> agent_client_protocol::Result<()> {
    // `mock-agent replay <trace.jsonl>` plays back a recorded session (`replay.rs`).
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("replay") {
        let trace = args.next().expect("replay needs the trace file");
        return mock_agent::replay(std::path::Path::new(&trace)).await.map_err(agent_client_protocol::Error::into_internal_error);
    }
    mock_agent::serve(false).await
}
