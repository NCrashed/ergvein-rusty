// The example calculates BIP158 filters for each block
extern crate bitcoin;
extern crate bitcoin_utxo;
extern crate byteorder;
extern crate bytes;
extern crate clap;
extern crate ergvein_filters;
extern crate fs2;
extern crate reqwest;
extern crate serde;
extern crate tokio;
extern crate tokio_stream;
extern crate tokio_util;
#[macro_use]
extern crate prometheus;
#[macro_use]
extern crate lazy_static;
extern crate warp;

pub mod filter;
pub mod server;
pub mod utxo;

use crate::filter::*;
use crate::server::connection::indexer_server;
use crate::server::fee::{fees_requester, FeesCache};
use crate::server::metrics::serve_metrics;
use crate::server::rates::{new_rates_cache, rates_requester};
use crate::utxo::FilterCoin;
use crate::utxo::coin_script;

use futures::future::{AbortHandle, Abortable, Aborted};
use futures::pin_mut;
use futures::stream;
use futures::SinkExt;
use mempool_filters::filtertree::FilterTree;
use mempool_filters::txtree::TxTree;
use mempool_filters::worker::mempool_worker;
use std::error::Error;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::Mutex;
use std::{env, process};
use tokio::time::Duration;

use bitcoin::network::constants;

use bitcoin_utxo::cache::utxo::*;
use bitcoin_utxo::connection::connect;
use bitcoin_utxo::storage::init_storage;
use bitcoin_utxo::sync::headers::sync_headers;
use bitcoin_utxo::sync::utxo::DEF_BLOCK_BATCH;

use clap::{crate_version, value_t, App, Arg};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let fork_depth_def = &UTXO_FORK_MAX_DEPTH.to_string();
    let cache_size_def = &UTXO_CACHE_MAX_COINS.to_string();
    let flush_period_def = &UTXO_FLUSH_PERIOD.to_string();
    let block_batch_def = &DEF_BLOCK_BATCH.to_string();
    let matches = App::new("ergvein-rusty")
        .about("P2P node to support ergvein mobile wallet")
        .version(crate_version!())
        .arg(
            Arg::with_name("host")
                .short("n")
                .long("host")
                .value_name("HOST")
                .help("Listen network interface")
                .default_value("0.0.0.0")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("port")
                .short("p")
                .long("port")
                .value_name("PORT")
                .help("Listen port")
                .default_value("8667")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("bitcoin")
                .short("b")
                .long("bitcoin")
                .value_name("BITCOIN_NODE")
                .help("Host and port for bitcoin node to use")
                .default_value("127.0.0.1:8333")
                .multiple(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("data")
                .short("d")
                .long("data")
                .value_name("DATABASE_PATH")
                .help("Directory where to store application state")
                .default_value("./ergvein_rusty_db")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("fork-depth")
                .long("fork-depth")
                .value_name("BLOCKS_AMOUNT")
                .help("Maximum reorganizatrion depth in blockchain")
                .default_value(&fork_depth_def)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("max-cache")
                .long("max-cache")
                .value_name("COINS_AMOUNT")
                .help("Maximum size of cache in coins amount, limits memory for cache")
                .default_value(&cache_size_def)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("flush-period")
                .long("flush-period")
                .value_name("BLOCKS_AMOUNT")
                .help("Flush cache to disk every given amount of blocks")
                .default_value(&flush_period_def)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("block-batch")
                .long("block-batch")
                .value_name("BLOCKS_AMOUNT")
                .help("Amount of blocks to process in parallel while syncing")
                .default_value(&block_batch_def)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("metrics-host")
                .long("metrics-host")
                .value_name("METRICS_HOST")
                .help("Listen network interface for Prometheus metrics")
                .default_value("0.0.0.0")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("metrics-port")
                .long("metrics-port")
                .value_name("METRICS_PORT")
                .help("Listen port")
                .default_value("9667")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("mempool-period")
                .long("mempool-period")
                .value_name("MEMPOOL_PERIOD")
                .help("How often to rebuild mempool filters. Integer seconds")
                .default_value("60")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("mempool-timeout")
                .long("mempool-timeout")
                .value_name("MEMPOOL_TIMEOUT")
                .help("Filter timeout. After it, consider the attempt failed and yield all mutexes")
                .default_value("10")
                .takes_value(true),
        )
        .get_matches();

    let str_address = matches.value_of("bitcoin").unwrap();

    let address: SocketAddr = str_address.parse().unwrap_or_else(|error| {
        eprintln!("Error parsing address: {:?}", error);
        process::exit(1);
    });

    let dbname = matches.value_of("data").unwrap();
    println!("Opening database {:?}", dbname);
    let db = Arc::new(init_storage(dbname.clone(), vec!["filters"])?);
    println!("Creating cache");
    let cache = Arc::new(new_cache::<FilterCoin>());
    let fee_cache = Arc::new(Mutex::new(FeesCache::default()));
    let rates_cache = Arc::new(new_rates_cache());
    tokio::spawn({
        let fee_cache = fee_cache.clone();
        async move {
            fees_requester(fee_cache).await;
        }
    });
    tokio::spawn({
        let rates_cache = rates_cache.clone();
        async move {
            rates_requester(rates_cache).await;
        }
    });

    let listen_addr = format!(
        "{}:{}",
        matches.value_of("host").unwrap(),
        value_t!(matches, "port", u32).unwrap()
    );
    tokio::spawn({
        let db = db.clone();
        let fee_cache = fee_cache.clone();
        let rates_cache = rates_cache.clone();
        async move {
            match indexer_server(listen_addr, db, fee_cache, rates_cache).await {
                Err(err) => {
                    eprintln!("Failed to listen TCP server: {}", err);
                }
                Ok(_) => (),
            }
        }
    });

    let metrics_addr = SocketAddr::new(
        value_t!(matches, "metrics-host", IpAddr).unwrap(),
        value_t!(matches, "metrics-port", u16).unwrap(),
    );
    tokio::spawn({
        let db = db.clone();
        let dbname = dbname.to_string();
        async move {
            println!("Start serving metrics at {}", metrics_addr);
            serve_metrics(metrics_addr, db.clone(), dbname).await;
        }
    });

    loop {
        let (headers_future, headers_stream, headers_sink) = sync_headers(db.clone()).await;
        pin_mut!(headers_sink);
        let (abort_headers_handle, abort_headers_registration) = AbortHandle::new_pair();
        tokio::spawn(async move {
            let res = Abortable::new(headers_future, abort_headers_registration).await;
            match res {
                Err(Aborted) => eprintln!("Headers task was aborted!"),
                _ => (),
            }
        });

        let (utxo_sink, utxo_stream, sync_mutex, abort_utxo_handle, restart_registration) = sync_filters(
            db.clone(),
            cache.clone(),
            value_t!(matches, "fork-depth", u32).unwrap(),
            value_t!(matches, "max-cache", usize).unwrap(),
            value_t!(matches, "flush-period", u32).unwrap(),
            value_t!(matches, "block-batch", usize).unwrap(),
        )
        .await;

        let txtree = Arc::new(TxTree::new());
        let ftree = Arc::new(FilterTree::new());
        let full_filter = Arc::new(tokio::sync::Mutex::new(None));
        let mempool_period = Duration::from_secs(
            value_t!(matches, "mempool-period", u64).unwrap_or(60)
        );
        let mempool_timeout = Duration::from_secs(
            value_t!(matches, "mempool-timeout", u64).unwrap_or(60)
        );
        let (tx_future, filt_future, filters_sink, filters_stream) =
            mempool_worker(
                txtree,
                ftree,
                full_filter,
                db.clone(),
                cache.clone(),
                sync_mutex.clone(),
                coin_script,
                mempool_period,
                mempool_timeout
            ).await;

        tokio::spawn(async move {
            tx_future.await.map_or_else(|e| eprintln!("TxTree task was aborted: {:?}", e), |_| ())
        });

        tokio::spawn(async move {
            filt_future.await.map_or_else(|e| eprintln!("FilterTree task was aborted: {:?}", e), |_| ())
        });

        pin_mut!(utxo_sink);
        pin_mut!(filters_sink);

        let msg_stream = stream::select(headers_stream, stream::select(utxo_stream, filters_stream));
        let msg_sink = headers_sink.fanout(utxo_sink.fanout(filters_sink));

        println!("Connecting to node");
        let res = Abortable::new(connect(
            &address,
            constants::Network::Bitcoin,
            "rust-client".to_string(),
            0,
            msg_stream,
            msg_sink,
        ), restart_registration)
        .await;

        match res {
            Err(_) => {
                abort_headers_handle.abort();
                eprintln!("Connection closed due local error. Reconnecting...");
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
            Ok(res) => match res {
                Err(err) => {
                    abort_utxo_handle.abort();
                    abort_headers_handle.abort();
                    eprintln!("Connection closed: {:?}. Reconnecting...", err);
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
                Ok(_) => {
                    println!("Gracefull termination");
                    break;
                }
            }

        }
    }

    Ok(())
}
