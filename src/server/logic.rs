use rocksdb::DB;
use ergvein_protocol::message::*;
use futures::{Future, Stream, Sink};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use futures::sink;

#[derive(Debug)]
pub enum IndexerError {
}

pub async fn indexer_logic(
    db: Arc<DB>,
) -> (
    impl Future<Output = Result<(), IndexerError>>,
    impl Stream<Item = Message> + Unpin,
    impl Sink<Message, Error = ergvein_protocol::message::Error>,
)
{
    const BUFFER_SIZE: usize = 100;
    let (broad_sender, _) = broadcast::channel(BUFFER_SIZE);
    let (msg_sender, msg_reciver) = mpsc::unbounded_channel::<Message>();
    let logic_future = {
        let broad_sender = broad_sender.clone();
        async move {
            Ok(())
        }
    };
    let msg_stream = UnboundedReceiverStream::new(msg_reciver);
    let msg_sink = sink::unfold(broad_sender, |broad_sender, msg| async move {
        broad_sender.send(msg).unwrap_or(0);
        Ok::<_, ergvein_protocol::message::Error>(broad_sender)
    });
    (logic_future, msg_stream, msg_sink)
}
