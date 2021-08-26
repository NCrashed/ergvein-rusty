use clap::{crate_version, value_t, App, Arg};
use bitcoin::network::constants;
use bitcoin_utxo::cache::utxo::*;
use bitcoin_utxo::connection::connect;
use bitcoin_utxo::storage::init_storage;
use bitcoin_utxo::sync::headers::sync_headers;
use bitcoin_utxo::sync::utxo::DEF_BLOCK_BATCH;

pub fn matches () -> App<'static, 'static> {
    let fork_depth_def = &UTXO_FORK_MAX_DEPTH.to_string();
    let cache_size_def = &UTXO_CACHE_MAX_COINS.to_string();
    let flush_period_def = &UTXO_FLUSH_PERIOD.to_string();
    let block_batch_def = &DEF_BLOCK_BATCH.to_string();
    App::new("ergvein-rusty")
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
}