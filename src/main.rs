// The example calculates BIP158 filters for each block
extern crate bitcoin;
extern crate bitcoin_utxo;
extern crate ergvein_filters;

pub mod filter;
pub mod utxo;

use crate::filter::*;
use crate::utxo::FilterCoin;

use futures::pin_mut;
use futures::stream;
use futures::SinkExt;

use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::{env, process};
use tokio::time::Duration;

use bitcoin::network::constants;

use bitcoin_utxo::cache::utxo::*;
use bitcoin_utxo::connection::connect;
use bitcoin_utxo::storage::init_storage;
use bitcoin_utxo::sync::headers::sync_headers;

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
