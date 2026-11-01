//! SIGINT and SIGTERM for the blocking subcommands (`lirtup run`,
//! `lirtup bench`), which have no tokio runtime of their own.

/// Call `on_signal` once, on a thread of its own, when the process gets
/// SIGINT or SIGTERM. A second signal exits at once with status 130 (tokio
/// keeps its handler installed, so the default action never comes back).
/// Start it only after
/// `lirtup_fg::layer::prepare` (it starts a thread).
pub fn on_terminate(on_signal: impl FnOnce() + Send + 'static) -> std::io::Result<()> {
    use tokio::signal::unix::{SignalKind, signal};

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    // Register on this thread, so a signal that arrives right after this
    // returns is not lost.
    let (mut int, mut term) = runtime.block_on(async {
        Ok::<_, std::io::Error>((signal(SignalKind::interrupt())?, signal(SignalKind::terminate())?))
    })?;
    std::thread::Builder::new()
        .name("lirtup-signals".into())
        .spawn(move || {
            let name = runtime.block_on(async {
                tokio::select! {
                    _ = int.recv() => "SIGINT",
                    _ = term.recv() => "SIGTERM",
                }
            });
            tracing::info!("{name}: stopping");
            // A second signal while `on_signal` runs (a stuck shutdown).
            std::thread::Builder::new()
                .name("lirtup-signals-2".into())
                .spawn(move || {
                    runtime.block_on(async {
                        tokio::select! {
                            _ = int.recv() => {}
                            _ = term.recv() => {}
                        }
                    });
                    std::process::exit(130);
                })
                .ok();
            on_signal();
        })?;
    Ok(())
}
