//! Internal command: host-side tool AppContainer + named-pipe IPC for exec when the daemon runs in app sandbox.

#[cfg(target_os = "windows")]
pub async fn cmd_tool_sandbox_serve(pipe: String, auth: String) -> anyhow::Result<()> {
    use std::io::Write;

    std::env::set_var(crate::sandbox::tool_sandbox_ipc::ENV_TOOL_SANDBOX_HELPER, "1");

    let cfg = crate::config::load_config(None)?;
    let tool_cfg = cfg
        .tool_sandbox
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("tool_sandbox is not configured"))?;
    let st = tool_cfg.sandbox_type.as_deref().unwrap_or("gvisor-docker");
    if !st.eq_ignore_ascii_case("appcontainer") {
        anyhow::bail!(
            "tool-sandbox serve requires toolSandbox.sandboxType \"appcontainer\" (got {:?})",
            st
        );
    }

    let workspace_path = crate::config::effective_workspace_path(&cfg);
    let skills_dir = crate::config::skills_dir();
    let sandbox_config = crate::config::build_tool_sandbox_config(
        tool_cfg,
        &cfg.sandbox_monitoring,
        &workspace_path,
        &skills_dir,
    )?;

    let sandbox_id = sandbox_config.sandbox_id.clone();

    let manager = std::sync::Arc::new(crate::sandbox::SandboxManager::with_defaults());
    manager
        .create_tool_sandbox(sandbox_config)
        .await
        .map_err(|e| anyhow::anyhow!("tool sandbox create: {}", e))?;
    manager
        .start_sandbox(&sandbox_id)
        .await
        .map_err(|e| anyhow::anyhow!("tool sandbox start: {}", e))?;

    println!("READY");
    std::io::stdout().flush()?;

    let mut first_instance = true;
    loop {
        // Create a fresh pipe instance per request. Calling DisconnectNamedPipe immediately after
        // writing can race with the client reading, causing ERROR_PIPE_NOT_CONNECTED (233).
        let mut server = crate::sandbox::tool_sandbox_ipc::create_tool_sandbox_pipe(&pipe, first_instance)
            .map_err(|e| anyhow::anyhow!("named pipe create: {}", e))?;
        first_instance = false;

        server
            .connect()
            .await
            .map_err(|e| anyhow::anyhow!("pipe connect: {}", e))?;
        if let Err(e) = crate::sandbox::tool_sandbox_ipc::dispatch_connected_request(
            &mut server,
            &auth,
            &sandbox_id,
            &manager,
        )
        .await
        {
            eprintln!("[synbot tool-sandbox] IPC dispatch error: {}", e);
        }
    }
}
