// The example calculates BIP158 filters for each block
extern crate bitcoin_utxo;
extern crate bitcoin;
extern crate clap;
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

use clap::{crate_version, App, Arg};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let matches = App::new("ergvein-rusty")
        .about("P2P node to support ergvein mobile wallet")
        .version(crate_version!())
        .arg(Arg::with_name("host")
            .short("n")
            .long("host")
            .value_name("HOST")
            .help("Listen network interface")
            .default_value("127.0.0.1")
            .takes_value(true))
        .arg(Arg::with_name("port")
            .short("p")
            .long("port")
            .value_name("PORT")
            .help("Listen port")
            .default_value("8667")
            .takes_value(true))
        .arg(Arg::with_name("bitcoin")
            .short("b")
            .long("bitcoin")
            .value_name("BITCOIN_NODE")
            .help("Host and port for bitcoin node to use")
            .default_value("127.0.0.1:8333")
            .multiple(true)
            .takes_value(true))
        .arg(Arg::with_name("data")
            .short("d")
            .long("data")
            .value_name("DATABASE_PATH")
            .help("Directory where to store application state")
            .default_value("./ergvein_rusty_db")
            .takes_value(true))
        .get_matches();

    let str_address = matches.value_of("bitcoin").unwrap();

    let address: SocketAddr = str_address.parse().unwrap_or_else(|error| {
        eprintln!("Error parsing address: {:?}", error);
        process::exit(1);
    });

    let dbname = matches.value_of("data").unwrap();
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
