use clap::{crate_version, value_t, App, Arg};
use bitcoin_utxo::cache::utxo::*;
use bitcoin_utxo::sync::utxo::DEF_BLOCK_BATCH;
use tokio::time::Duration;
use std::option::*;

use std::net::{IpAddr, SocketAddr};

pub struct AppConfig {
    pub is_testnet : bool,
    pub node_address : SocketAddr,
    pub db_name : String,
    pub host : String,
    pub port : u32,
    pub metrics_addr : SocketAddr,
    pub fork_depth : u32,
    pub max_cache : usize,
    pub flush_period : u32,
    pub block_batch : usize,
    pub mempool_period : Duration,
    pub mempool_timeout : Duration
}

pub fn app_config () -> Option<AppConfig>  {
    let fork_depth_def = &UTXO_FORK_MAX_DEPTH.to_string();
    let cache_size_def = &UTXO_CACHE_MAX_COINS.to_string();
    let flush_period_def = &UTXO_FLUSH_PERIOD.to_string();
    let block_batch_def = &DEF_BLOCK_BATCH.to_string();
    let app = App::new("ergvein-rusty")
        .about("P2P node to support ergvein mobile wallet")
        .version(crate_version!())
        .arg(
            Arg::with_name("testnet")
                .short("t")
                .long("testnet")
                .value_name("TESTNET")
                .help("network type")
                .takes_value(false)
        )
        .arg(
            Arg::with_name("host")
                .short("n")
                .long("host")
                .value_name("HOST")
                .help("Listen network interface")
                .default_value("0.0.0.0")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("port")
                .short("p")
                .long("port")
                .value_name("PORT")
                .help("Listen port")
                .default_value("8667")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("fork-depth")
                .long("fork-depth")
                .value_name("BLOCKS_AMOUNT")
                .help("Maximum reorganization depth in blockchain")
                .default_value(&fork_depth_def)
                .takes_value(true)
        )
        .arg(
            Arg::with_name("max-cache")
                .long("max-cache")
                .value_name("COINS_AMOUNT")
                .help("Maximum size of cache in coins amount, limits memory for cache")
                .default_value(&cache_size_def)
                .takes_value(true)
        )
        .arg(
            Arg::with_name("flush-period")
                .long("flush-period")
                .value_name("BLOCKS_AMOUNT")
                .help("Flush cache to disk every given amount of blocks")
                .default_value(&flush_period_def)
                .takes_value(true)
        )
        .arg(
            Arg::with_name("block-batch")
                .long("block-batch")
                .value_name("BLOCKS_AMOUNT")
                .help("Amount of blocks to process in parallel while syncing")
                .default_value(&block_batch_def)
                .takes_value(true)
        )
        .arg(
            Arg::with_name("bitcoin")
                .short("b")
                .long("bitcoin")
                .value_name("BITCOIN_NODE")
                .help("Host and port for bitcoin node to use")
                .default_value("127.0.0.1:8333")
                .multiple(true)
                .takes_value(true)
        )
        .arg(
            Arg::with_name("data")
                .short("d")
                .long("data")
                .value_name("DATABASE_PATH")
                .help("Directory where to store application state")
                .default_value("./ergvein_rusty_db")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("metrics-host")
                .long("metrics-host")
                .value_name("METRICS_HOST")
                .help("Listen network interface for Prometheus metrics")
                .default_value("0.0.0.0")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("metrics-port")
                .long("metrics-port")
                .value_name("METRICS_PORT")
                .help("Listen port")
                .default_value("9667")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("mempool-period")
                .long("mempool-period")
                .value_name("MEMPOOL_PERIOD")
                .help("How often to rebuild mempool filters. Integer seconds")
                .default_value("60")
                .takes_value(true)
        )
        .arg(
            Arg::with_name("mempool-timeout")
                .long("mempool-timeout")
                .value_name("MEMPOOL_TIMEOUT")
                .help("Filter timeout. After it, consider the attempt failed and yield all mutexes")
                .default_value("10")
                .takes_value(true)
        );

    let matches = app.get_matches();
    let is_testnet = matches.is_present("testnet");
    let str_address = matches.value_of("bitcoin")?;
    let address: SocketAddr = str_address.parse().ok()?;
    let db_name = matches.value_of("data")?.to_string();
    let host = matches.value_of("host")?.to_string();
    let port = value_t!(matches, "port", u32).ok()?;
    let metrics_host = value_t!(matches, "metrics-host", IpAddr).ok()?;
    let metrics_port = value_t!(matches, "metrics-port", u16).ok()?;
    let metrics_addr = SocketAddr::new(metrics_host, metrics_port);
    let fork_depth = value_t!(matches, "fork-depth", u32).ok()?;
    let max_cache = value_t!(matches, "max-cache", usize).ok()?;
    let flush_period = value_t!(matches, "flush-period", u32).ok()?;
    let block_batch = value_t!(matches, "block-batch", usize).ok()?;
    let mempool_period = Duration::from_secs(value_t!(matches, "mempool-period", u64).unwrap_or(60));
    let mempool_timeout = Duration::from_secs(value_t!(matches, "mempool-timeout", u64).unwrap_or(60));

    Some(AppConfig {
        is_testnet: is_testnet,
        node_address : address,
        db_name : db_name, 
        host : host,
        port : port,
        metrics_addr : metrics_addr,
        fork_depth : fork_depth,
        max_cache : max_cache,
        flush_period : flush_period,
        block_batch : block_batch,
        mempool_period : mempool_period,
        mempool_timeout : mempool_timeout
    })
}