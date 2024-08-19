use tokio::sync::oneshot::Receiver;

pub async fn shutdown() -> Receiver<()> {
    imp::shutdown().await
}

mod imp {
    use tokio::signal::unix;
    use tokio::signal::unix::SignalKind;
    use tokio::sync::oneshot;
    use tracing::{info, warn};

    pub(super) async fn shutdown() -> oneshot::Receiver<()> {
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        tokio::spawn(async move {
            let mut sigquit = unix::signal(SignalKind::quit()).unwrap();
            let mut sigterm = unix::signal(SignalKind::terminate()).unwrap();
            let mut sigint = unix::signal(SignalKind::interrupt()).unwrap();

            tokio::select! {
                _ = sigterm.recv() => {
                    info!("Received SIGTERM");
                }
                _ = sigint.recv() => {
                    info!("Received SIGINT");
                }
                _ = sigquit.recv() => {
                    info!("Received SIGQUIT");
                }
            }

            warn!("Shutdown signal received!");
            let _ = shutdown_tx.send(());
        });

        shutdown_rx
    }
}
