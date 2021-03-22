use bitcoin_utxo::storage::chain::get_chain_height;
use crate::filter::*;
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

/// Amount of seconds connection is open after handshake
pub const CONNECTION_DROP_TIMEOUT: u64 = 60*20;
/// Limit to amount of filters that can be requested via the server in one request
pub const MAX_FILTERS_REQ: u32 = 2000;

#[derive(Debug)]
pub enum IndexerError {
    HandshakeSendError,
    HandshakeTimeout,
    HandshakeRecv,
    HandshakeViolation,
    HandshakeNonceIdentical,
    NotCompatible(Version),
    NotSupportedCurrency(Currency),
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
            handshake(addr.clone(), db.clone(), &mut in_reciver, &out_sender).await?;

            let timeout = tokio::time::sleep(Duration::from_secs(CONNECTION_DROP_TIMEOUT));
            tokio::pin!(timeout);

            let filters_fut = serve_filters(addr.clone(), db.clone(), &mut in_reciver, &out_sender);
            tokio::pin!(filters_fut);

            let mut close = false;
            while !close {
                tokio::select! {
                    _ = &mut timeout => {
                        eprintln!("Connection closed by mandatory timeout {}", addr);
                        close = true;
                    }
                    res = &mut filters_fut => match res {
                        Err(e) => {
                            eprintln!("Failed to serve filters to client {}, reason: {:?}", addr, e);
                            close = true;
                        }
                        Ok(_) => {
                            eprintln!("Impossible, fitlers serve ended to client {}", addr);
                            close = true;
                        }
                    }
                }
            }

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

fn is_supported_currency(currency: &Currency) -> bool {
    *currency == Currency::Btc
}

async fn serve_filters(
    addr: String,
    db: Arc<DB>,
    msg_reciever: &mut mpsc::UnboundedReceiver<Message>,
    msg_sender: &mpsc::UnboundedSender<Message>,
) -> Result<(), IndexerError> {
    loop {
        if let Some(msg) = msg_reciever.recv().await {
            match &msg {
                Message::GetFilters(req) => {
                    if !is_supported_currency(&req.currency) {
                        msg_sender.send(Message::Reject(RejectMessage{
                            id: msg.id(),
                            data: RejectData::InternalError,
                            message: format!("Not supported currency {:?}", req.currency),
                        })).unwrap();
                        Err(IndexerError::NotSupportedCurrency(req.currency))?
                    }
                    let h = get_filters_height(db.clone());
                    if req.start > h as u64 {
                        let resp = Message::Filters(FiltersResp {
                            currency: req.currency,
                            filters: vec![],
                        });
                        msg_sender.send(resp).unwrap();
                    } else {
                        let amount = req.amount.min(MAX_FILTERS_REQ);
                        let filters = read_filters(db.clone(), req.start as u32, amount)
                        .iter()
                        .map(|(h, f)| Filter {
                            block_id: h.to_vec(),
                            filter: f.content.clone()
                        })
                        .collect();
                        let resp = Message::Filters(FiltersResp {
                            currency: req.currency,
                            filters: filters,
                        });
                        msg_sender.send(resp).unwrap();
                    }
                }
                _ => (),
            }
        }
    }
}
