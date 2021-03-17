// The example calculates BIP158 filters for each block
extern crate bitcoin;
extern crate bitcoin_utxo;
extern crate ergvein_filters;

pub mod filter;
pub mod utxo;

use crate::filter::*;
use crate::utxo::FilterCoin;

use futures::future::{AbortHandle, Abortable, Aborted};
use futures::pin_mut;
use futures::stream;
use futures::SinkExt;

use hex;

use rocksdb::DB;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::{env, process};
use tokio::time::Duration;

use bitcoin::consensus::encode;
use bitcoin::network::constants;
use bitcoin::network::message::NetworkMessage;

use bitcoin_utxo::cache::utxo::*;
use bitcoin_utxo::connection::connect;
use bitcoin_utxo::storage::init_storage;
use bitcoin_utxo::sync::headers::sync_headers;
use bitcoin_utxo::sync::utxo::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("not enough arguments");
        process::exit(1);
    }

    let str_address = &args[1];

    let address: SocketAddr = str_address.parse().unwrap_or_else(|error| {
        eprintln!("Error parsing address: {:?}", error);
        process::exit(1);
    });

    let dbname = "./filters_utxo_db";
    println!("Opening database {:?}", dbname);
    let db = Arc::new(init_storage(dbname, vec!["filters"])?);
    println!("Creating cache");
    let cache = Arc::new(new_cache::<FilterCoin>());

    loop {
        let (headers_stream, headers_sink) = sync_headers(db.clone()).await;
        pin_mut!(headers_sink);

        let (utxo_sink, utxo_stream, abort_handle) = sync_filters(db.clone(), cache.clone()).await;
        pin_mut!(utxo_sink);

        let msg_stream = stream::select(headers_stream, utxo_stream);
        let msg_sink = headers_sink.fanout(utxo_sink);

        println!("Connecting to node");
        let res = connect(
            &address,
            constants::Network::Bitcoin,
            "rust-client".to_string(),
            0,
            msg_stream,
            msg_sink,
        )
        .await;

        match res {
            Err(err) => {
                abort_handle.abort();
                eprintln!("Connection closed: {:?}. Reconnecting...", err);
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
            Ok(_) => {
                println!("Gracefull termination");
                break;
            }
        }
    }

    Ok(())
}

pub async fn sync_filters(
    db: Arc<DB>,
    cache: Arc<UtxoCache<FilterCoin>>,
) -> (
    impl futures::Sink<NetworkMessage, Error = encode::Error>,
    impl Unpin + futures::Stream<Item = NetworkMessage>,
    AbortHandle,
) {
    let db = db.clone();
    let cache = cache.clone();
    let (sync_future, utxo_stream, utxo_sink) =
        sync_utxo_with(db.clone(), cache.clone(), move |h, block| {
            let block = block.clone();
            let db = db.clone();
            let cache = cache.clone();
            async move {
                let hash = block.block_hash();
                let filter = generate_filter(db.clone(), cache, h, block).await;
                if h % 1000 == 0 {
                    println!(
                        "Filter for block {:?}: {:?}",
                        h,
                        hex::encode(&filter.content)
                    );
                }
                store_filter(db, &hash, filter);
            }
        })
        .await;

    let (abort_handle, abort_registration) = AbortHandle::new_pair();
    tokio::spawn(async move {
        let res = Abortable::new(sync_future, abort_registration).await;
        match res {
            Err(Aborted) => eprintln!("Sync task was aborted!"),
            _ => (),
        }
    });

    (utxo_sink, utxo_stream, abort_handle)
}
