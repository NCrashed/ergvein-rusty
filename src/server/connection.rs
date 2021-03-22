use ergvein_protocol::message::*;
use futures::{future, Sink, SinkExt, Stream, StreamExt};
use futures::future::{AbortHandle, Abortable, Aborted};
use futures::pin_mut;
use rocksdb::DB;
use std::error::Error;
use std::fmt::Display;
use std::sync::{Arc, Mutex};
use super::fee::{FeesCache};
use tokio_util::codec::{FramedRead, FramedWrite};
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};

use super::codec::*;
use super::logic::*;

pub async fn indexer_server<A>(addr: A, db: Arc<DB>, fees: Arc<Mutex<FeesCache>>) -> Result<(), std::io::Error>
where
    A: ToSocketAddrs + Display,
{
    let listener = TcpListener::bind(&addr).await?;
    println!("Listening on: {}", addr);

    loop {
        let (mut socket, _) = listener.accept().await?;

        tokio::spawn({
            let db = db.clone();
            let fees = fees.clone();
            let addr = addr.to_string();
            async move {
                let (msg_future, msg_stream, msg_sink) = indexer_logic(addr, db.clone(), fees.clone()).await;
                pin_mut!(msg_sink);

                let (abort_logic, reg_logic_abort) = AbortHandle::new_pair();
                let (abort_conn, reg_conn_abort) = AbortHandle::new_pair();
                tokio::spawn(async move {
                    let res = Abortable::new(msg_future, reg_logic_abort).await;
                    match res {
                        Err(Aborted) => eprintln!("Client logic task was aborted!"),
                        Ok(Err(_)) => abort_conn.abort(),
                        _ => (),
                    }
                });

                Abortable::new(connect(&mut socket, msg_stream, msg_sink), reg_conn_abort)
                    .await
                    .unwrap_or_else(|_| Ok(()))
                    .unwrap_or_else(|err| {
                        println!(
                            "Connection to {} is closed with {}",
                            socket.peer_addr().unwrap(),
                            err
                        );
                        abort_logic.abort();
                    });
                println!("Connection to {} is closed", socket.peer_addr().unwrap());
            }
        });
    }
}

async fn connect(
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
