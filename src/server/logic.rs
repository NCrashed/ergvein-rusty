use bitcoin_utxo::storage::chain::get_chain_height;
use crate::filter::get_filters_height;
use ergvein_protocol::message::*;
use futures::{Future, Stream, Sink};
use futures::sink;
use rand::{thread_rng, Rng};
use rocksdb::DB;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio::sync::mpsc;
use tokio::time::Duration;


#[derive(Debug)]
pub enum IndexerError {
    HandshakeSendError,
    HandshakeTimeout,
    HandshakeLagged,
    HandshakeRecv,
    HandshakeViolation,
    HandshakeNonceIdentical,
    NotCompatible(Version),
}

pub async fn indexer_logic(
    addr: String,
    db: Arc<DB>,
) -> (
    impl Future<Output = Result<(), IndexerError>>,
    impl Stream<Item = Message> + Unpin,
    impl Sink<Message, Error = ergvein_protocol::message::Error>,
)
{
    let (in_sender, mut in_reciver) = mpsc::unbounded_channel::<Message>();
    let (out_sender, out_reciver) = mpsc::unbounded_channel::<Message>();
    let logic_future = {
        let db = db.clone();
        async move {
            handshake(addr, db, &mut in_reciver, &out_sender).await?;
            Ok(())
        }
    };
    let msg_stream = UnboundedReceiverStream::new(out_reciver);
    let msg_sink = sink::unfold(in_sender, |in_sender, msg| async move {
        in_sender.send(msg).unwrap();
        Ok::<_, ergvein_protocol::message::Error>(in_sender)
    });
    (logic_future, msg_stream, msg_sink)
}

async fn handshake(
    addr: String,
    db: Arc<DB>,
    msg_reciever: &mut mpsc::UnboundedReceiver<Message>,
    msg_sender: &mpsc::UnboundedSender<Message>,
) -> Result<(), IndexerError> {
    let ver_msg = build_version_message(db);
    msg_sender.send(Message::Version(ver_msg.clone())).map_err(|e| { println!("Error when sending handshake: {:?}", e); IndexerError::HandshakeSendError})?;
    let timeout = tokio::time::sleep(Duration::from_secs(20));
    tokio::pin!(timeout);
    let mut got_version = false;
    let mut got_ack = false;
    while !(got_version && got_ack) {
        tokio::select! {
            _ = &mut timeout => {
                eprintln!("Handshake timeout {}", addr);
                Err(IndexerError::HandshakeTimeout)?
            }
            emsg = msg_reciever.recv() => match emsg {
                None => {
                    eprintln!("Failed to recv handshake for {}", addr);
                    Err(IndexerError::HandshakeRecv)?
                }
                Some(msg) => match msg {
                    Message::Version(vmsg)=> {
                        if !Version::current().compatible(&vmsg.version) {
                            eprint!("Not compatible version for client {}, version {:?}", addr, vmsg.version);
                            Err(IndexerError::NotCompatible(vmsg.version.clone()))?;
                        }
                        if vmsg.nonce == ver_msg.nonce {
                            eprint!("Connected to self, nonce identical for {}", addr);
                            Err(IndexerError::HandshakeNonceIdentical)?;
                        }
                        println!("Handshaked with client {} and version {:?}", addr, vmsg.version);
                        got_version = true;
                    }
                    Message::VersionAck => {
                        println!("Received verack for client {}", addr);
                        got_ack = true;
                    }
                    _ => {
                        eprintln!("Received from {} something that not handshake: {:?}", addr, msg);
                        Err(IndexerError::HandshakeViolation)?;
                    },
                },
            }
        }
    }
    Ok(())
}

fn build_version_message(db: Arc<DB>) -> VersionMessage {
    // "standard UNIX timestamp in seconds"
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time error")
        .as_secs();

    // "Node random nonce, randomly generated every time a version packet is sent. This nonce is used to detect connections to self."
    let mut rng = thread_rng();
    let nonce: [u8; 8] = rng.gen();

    // Construct the message
    VersionMessage {
        version: Version::current(),
        time: timestamp,
        nonce: nonce,
        scan_blocks: vec![
            ScanBlock {
                currency: Currency::Btc,
                version: Version {
                    major: 1,
                    minor: 0,
                    patch: 0,
                },
                scan_height: get_filters_height(db.clone()) as u64,
                height: get_chain_height(&db.clone()) as u64,
            }
        ],
    }
}
