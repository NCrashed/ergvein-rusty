// The example calculates BIP158 filters for each block
extern crate bitcoin;
extern crate bitcoin_utxo;
extern crate byteorder;
extern crate bytes;
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

pub mod options;
pub mod filter;
pub mod server;
pub mod utxo;

use crate::filter::*;
use crate::options::*;
use crate::server::connection::indexer_server;
use crate::server::fee::{fees_requester, FeesCache};
use crate::server::metrics::serve_metrics;
use crate::server::rates::{new_rates_cache, rates_requester};
use crate::utxo::coin_script;
use crate::utxo::FilterCoin;

use futures::future::{AbortHandle, Abortable, Aborted};
use futures::pin_mut;
use futures::stream;
use futures::SinkExt;
use mempool_filters::filtertree::FilterTree;
use mempool_filters::txtree::TxTree;
use mempool_filters::worker::mempool_worker;
use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::time::Duration;

use bitcoin::network::constants;

use bitcoin_utxo::cache::utxo::*;
use bitcoin_utxo::connection::connect;
use bitcoin_utxo::storage::init_storage;
use bitcoin_utxo::sync::headers::sync_headers;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    match app_config() {
      Some(cfg) => {
        println!("Opening database {:?}", &cfg.db_name);

    let db = Arc::new(init_storage(&cfg.db_name, vec!["filters"])?);

    println!("Creating cache");

    let cache = Arc::new(new_cache::<FilterCoin>());
    let fee_cache = Arc::new(Mutex::new(FeesCache::default()));
    let rates_cache = Arc::new(new_rates_cache());
    let txtree = Arc::new(TxTree::new());
    let ftree = Arc::new(FilterTree::new());
    let full_filter = Arc::new(tokio::sync::RwLock::new(None));

    tokio::spawn({
        let fee_cache = fee_cache.clone();
        let is_testnet = cfg.is_testnet.clone();
        async move {
            fees_requester(is_testnet, fee_cache).await;
        }
    });

    tokio::spawn({
        let rates_cache = rates_cache.clone();
        let is_testnet = cfg.is_testnet.clone();
        async move {
            rates_requester(is_testnet, rates_cache).await;
        }
    });

    let listen_addr = format!("{}:{}", cfg.host, cfg.port);

    tokio::spawn({
        let db = db.clone();
        let fee_cache = fee_cache.clone();
        let rates_cache = rates_cache.clone();
        let txtree = txtree.clone();
        let ftree = ftree.clone();
        let full_filter = full_filter.clone();
        let is_testnet = cfg.is_testnet.clone();
        async move {
            if let Err(err) =
                indexer_server(
                    is_testnet,
                    listen_addr,
                    db,
                    fee_cache,
                    rates_cache,
                    txtree,
                    ftree,
                    full_filter
                    ).await {
                        eprintln!("Failed to listen TCP server: {}", err);
                    }
        }
    });

    tokio::spawn({
        let db = db.clone();
        let db_name = cfg.db_name.clone();
        let m_addr = cfg.metrics_addr.clone();
        let is_testnet = cfg.is_testnet.clone();
        async move {
            println!("Start serving metrics at {}", m_addr);
            serve_metrics(is_testnet, m_addr, db, db_name).await;
        }
    });

    loop {
        let (headers_future, headers_stream, headers_sink) = sync_headers(db.clone()).await;

        pin_mut!(headers_sink);

        let (abort_headers_handle, abort_headers_registration) = AbortHandle::new_pair();

        tokio::spawn(async move {
            if let Err(Aborted) = Abortable::new(headers_future, abort_headers_registration).await {
                eprintln!("Headers task was aborted!")
            }
        });

        let (utxo_sink, utxo_stream, sync_mutex, abort_utxo_handle, restart_registration) =
            sync_filters(
                cfg.is_testnet,
                db.clone(),
                cache.clone(),
                cfg.fork_depth,
                cfg.max_cache,
                cfg.flush_period,
                cfg.block_batch,
            )
            .await;

        let (mempool_future, filters_sink, filters_stream) = mempool_worker(
            txtree.clone(),
            ftree.clone(),
            full_filter.clone(),
            db.clone(),
            cache.clone(),
            sync_mutex.clone(),
            coin_script,
            cfg.mempool_period,
            cfg.mempool_timeout,
        )
        .await;

        tokio::spawn(async move {
            mempool_future
                .await
                .map_or_else(|e| eprintln!("Mempool task was aborted: {:?}", e), |_| ())
        });

        pin_mut!(utxo_sink);
        pin_mut!(filters_sink);

        let msg_stream =
            stream::select(headers_stream, stream::select(utxo_stream, filters_stream));
        let msg_sink = headers_sink.fanout(utxo_sink.fanout(filters_sink));

        println!("Connecting to node");
        let res = Abortable::new(
            connect(
                &cfg.node_address,
                if cfg.is_testnet {constants::Network::Testnet} else {constants::Network::Bitcoin},
                "rust-client".to_string(),
                0,
                msg_stream,
                msg_sink,
            ),
            restart_registration,
        )
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
            },
        }
    }
      }
      None => {
        println!("Error in app parameters");
      }
    }
    Ok(())
}
