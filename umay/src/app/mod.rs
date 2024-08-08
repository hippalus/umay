use tokio::signal::unix;
use tokio::signal::unix::SignalKind;
use tokio::sync::oneshot;

pub mod config;
pub mod server;

pub async fn signal_handler(tx: oneshot::Sender<()>) {
    let mut sigquit = unix::signal(unix::SignalKind::quit()).unwrap();
    let mut sigterm = unix::signal(SignalKind::terminate()).unwrap();
    let mut sigint = unix::signal(SignalKind::interrupt()).unwrap();

    tokio::select! {
        _ = sigterm.recv() => {
            tracing::info!("Received SIGTERM");
        }
        _ = sigint.recv() => {
            tracing::info!("Received SIGINT");
        }
    _ = sigquit.recv() => {
            tracing::info!("Received SIGQUIT");
        }
    }

    let _ = tx.send(());
}
