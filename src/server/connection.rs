use ergvein_protocol::message::*;
use futures::{future, Sink, SinkExt, Stream, StreamExt};
use futures::future::{AbortHandle, Abortable, Aborted};
use futures::stream;
use futures::pin_mut;
use rand::{thread_rng, Rng};
use std::error::Error;
use std::fmt::Display;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio_util::codec::{FramedRead, FramedWrite};
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};
use rocksdb::DB;
use std::sync::Arc;

use bitcoin_utxo::connection::message::process_messages;

use super::codec::*;
use super::logic::*;

pub async fn indexer_server<A>(addr: A, db: Arc<DB>) -> Result<(), std::io::Error>
    where
        A:ToSocketAddrs+Display,
{
    let listener = TcpListener::bind(&addr).await?;
    println!("Listening on: {}", addr);

    loop {
        let (mut socket, _) = listener.accept().await?;

        tokio::spawn({
            let db = db.clone();
            async move {
                let (msg_future, msg_stream, msg_sink) = indexer_logic(db.clone()).await;
                pin_mut!(msg_sink);

                let (abort_handle, abort_registration) = AbortHandle::new_pair();
                tokio::spawn(async move {
                    let res = Abortable::new(msg_future, abort_registration).await;
                    match res {
                        Err(Aborted) => eprintln!("Client logic task was aborted!"),
                        _ => (),
                    }
                });

                connect(
                    &mut socket,
                    msg_stream,
                    msg_sink,
                ).await.unwrap_or_else(|err| {
                    println!("Connection to {} is closed with {}", socket.peer_addr().unwrap(), err);
                    abort_handle.abort();
                });
            }
        });
    }
}

pub async fn connect(
    socket: &mut TcpStream,
    inmsgs: impl Stream<Item = Message> + Unpin,
    outmsgs: impl Sink<Message, Error = ergvein_protocol::message::Error> + Unpin,
) -> Result<(), Box<dyn Error>> {
    let handshake_stream =
        stream::once(async { build_version_message() });
    pin_mut!(handshake_stream);

    let (verack_stream, verack_sink) = process_messages(|sender, msg| async move {
        match msg {
            Message::Version(_) => {
                println!("Received version message: {:?}", msg);
                println!("Sent verack message");
                sender.send(Message::VersionAck).await.unwrap();
            }
            Message::VersionAck => {
                println!("Received verack message: {:?}", msg);
            }
            _ => (),
        };
        sender
    });
    pin_mut!(verack_sink);

    let internal_inmsgs = stream::select(stream::select(inmsgs, handshake_stream), verack_stream);

    raw_connect(socket, internal_inmsgs, outmsgs.fanout(verack_sink)).await
}

async fn raw_connect(
    socket: &mut TcpStream,
    inmsgs: impl Stream<Item = Message> + Unpin,
    mut outmsgs: impl Sink<Message, Error = ergvein_protocol::message::Error> + Unpin,
) -> Result<(), Box<dyn Error>> {
    let (abort_handle, abort_registration) = AbortHandle::new_pair();
    let (r, w) = socket.split();
    let mut sink = FramedWrite::new(w, MessageCodec::default());
    let mut stream = FramedRead::new(r, MessageCodec::default()).filter_map(|i| match i {
        Ok(i) => future::ready(Some(Ok(i))),
        Err(e) => {
            println!("Failed to read from socket; error={}", e);
            abort_handle.abort();
            future::ready(Some(Err(e)))
        }
    });

    let mut inmsgs_err = inmsgs.map(Ok);
    match Abortable::new(
        future::join(
            sink.send_all(&mut inmsgs_err),
            outmsgs.send_all(&mut stream),
        ),
        abort_registration,
    )
    .await
    {
        Err(Aborted) => Err(ergvein_protocol::message::Error::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Connection closed!",
        ))
        .into()),
        Ok(res) => match res {
            (Err(e), _) | (_, Err(e)) => Err(e.into()),
            _ => Ok(()),
        },
    }
}

fn build_version_message() -> Message {
    // "standard UNIX timestamp in seconds"
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time error")
        .as_secs();

    // "Node random nonce, randomly generated every time a version packet is sent. This nonce is used to detect connections to self."
    let mut rng = thread_rng();
    let nonce: [u8; 8] = rng.gen();

    // Construct the message
    Message::Version(VersionMessage {
        version: Version::current(),
        time: timestamp,
        nonce: nonce,
        scan_blocks: vec![],
    })
}
