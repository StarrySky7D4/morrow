use super::PipeInputClose;
use super::run_pipe_writer;
use std::io;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;

#[tokio::test]
async fn checked_close_drains_input_with_a_live_sender_clone() {
    let (writer, mut reader) = tokio::io::duplex(1024);
    let (sender, receiver) = mpsc::channel(4);
    let retained_sender = sender.clone();
    let (close, control) = PipeInputClose::channel();
    let task = tokio::spawn(run_pipe_writer(writer, receiver, control));
    sender.send(b"first".to_vec()).await.unwrap();
    sender.send(b"second".to_vec()).await.unwrap();
    close.request();
    close.wait_closed().await.unwrap();
    assert!(retained_sender.send(b"late".to_vec()).await.is_err());
    let mut observed = Vec::new();
    reader.read_to_end(&mut observed).await.unwrap();
    assert_eq!(observed, b"firstsecond");
    task.await.unwrap();
}

#[tokio::test]
async fn checked_close_waits_for_an_outstanding_owned_permit() {
    let (writer, mut reader) = tokio::io::duplex(1024);
    let (sender, receiver) = mpsc::channel(4);
    let permit = sender.clone().reserve_owned().await.unwrap();
    let (close, control) = PipeInputClose::channel();
    let task = tokio::spawn(run_pipe_writer(writer, receiver, control));
    close.request();
    sender.closed().await;
    assert!(
        tokio::time::timeout(Duration::from_millis(10), close.wait_closed())
            .await
            .is_err()
    );
    let retained_sender = permit.send(b"reserved-before-close".to_vec());
    close.wait_closed().await.unwrap();
    assert!(retained_sender.reserve().await.is_err());
    let mut observed = Vec::new();
    reader.read_to_end(&mut observed).await.unwrap();
    assert_eq!(observed, b"reserved-before-close");
    task.await.unwrap();
}

#[tokio::test]
async fn cancelling_a_close_wait_keeps_the_single_sticky_request() {
    let (writer, mut reader) = tokio::io::duplex(1024);
    let (sender, receiver) = mpsc::channel(1);
    let permit = sender.clone().reserve_owned().await.unwrap();
    let (close, control) = PipeInputClose::channel();
    let task = tokio::spawn(run_pipe_writer(writer, receiver, control));
    close.request();
    sender.closed().await;
    assert!(
        tokio::time::timeout(Duration::from_millis(10), close.wait_closed())
            .await
            .is_err()
    );
    close.request();
    assert!(close.requested());
    assert!(sender.reserve().await.is_err());
    drop(permit);
    close.wait_closed().await.unwrap();
    close.wait_closed().await.unwrap();
    let mut observed = Vec::new();
    reader.read_to_end(&mut observed).await.unwrap();
    assert!(observed.is_empty());
    task.await.unwrap();
}

#[tokio::test]
async fn a_writer_failure_cannot_be_acknowledged_as_a_clean_close() {
    let (writer, reader) = tokio::io::duplex(1024);
    let (sender, receiver) = mpsc::channel(1);
    let (close, control) = PipeInputClose::channel();
    drop(reader);
    let task = tokio::spawn(run_pipe_writer(writer, receiver, control));
    sender.send(b"unwritable".to_vec()).await.unwrap();
    close.request();
    assert_eq!(
        close.wait_closed().await.unwrap_err().kind(),
        io::ErrorKind::BrokenPipe
    );
    assert_eq!(
        close.wait_closed().await.unwrap_err().kind(),
        io::ErrorKind::BrokenPipe
    );
    assert!(sender.send(b"late".to_vec()).await.is_err());
    task.await.unwrap();
}

#[tokio::test]
async fn an_aborted_writer_keeps_an_uncertain_close_receipt() {
    let (writer, _reader) = tokio::io::duplex(1);
    let (sender, receiver) = mpsc::channel(1);
    let (close, control) = PipeInputClose::channel();
    let task = tokio::spawn(run_pipe_writer(writer, receiver, control));
    sender.send(b"blocked-write".to_vec()).await.unwrap();
    close.request();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(
        close.wait_closed().await.unwrap_err().kind(),
        io::ErrorKind::Other
    );
    assert!(sender.send(b"late".to_vec()).await.is_err());
}
