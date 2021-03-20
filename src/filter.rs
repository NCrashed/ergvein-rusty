use bitcoin_utxo::cache::utxo::*;
use bitcoin_utxo::sync::utxo::*;
use bitcoin::{Block, BlockHash, OutPoint, Script};
use bitcoin::consensus::encode;
use bitcoin::network::message::NetworkMessage;
use bitcoin::util::bip158;
use ergvein_filters::btc::ErgveinFilter;
use futures::future::{AbortHandle, Abortable, Aborted};
use rocksdb::{WriteBatch, WriteOptions, DB};
use std::collections::HashMap;
use std::sync::Arc;
use super::utxo::FilterCoin;
use tokio::time::Duration;

pub async fn generate_filter(
    db: Arc<DB>,
    cache: Arc<UtxoCache<FilterCoin>>,
    h: u32,
    block: Block,
) -> ErgveinFilter {
    let mut hashmap = HashMap::<OutPoint, Script>::new();
    for tx in &block.txdata {
        if !tx.is_coin_base() {
            for i in &tx.input {
                let coin = wait_utxo(
                    db.clone(),
                    cache.clone(),
                    &i.previous_output,
                    h,
                    Duration::from_millis(100),
                )
                .await;
                hashmap.insert(i.previous_output, coin.script);
            }
        }
    }
    ErgveinFilter::new_script_filter(&block, |out| {
        hashmap
            .get(out)
            .map_or(Err(bip158::Error::UtxoMissing(*out)), |s| Ok(s.clone()))
    })
    .unwrap()
}

pub fn store_filter(db: Arc<DB>, hash: &BlockHash, filter: ErgveinFilter) {
    let cf = db.cf_handle("filters").unwrap();
    let mut batch = WriteBatch::default();
    batch.put_cf(cf, hash, filter.content);

    let mut write_options = WriteOptions::default();
    write_options.set_sync(false);
    write_options.disable_wal(true);
    db.write_opt(batch, &write_options).unwrap();
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
        sync_utxo_with(db.clone(), cache.clone(),
            UTXO_FORK_MAX_DEPTH,
            UTXO_CACHE_MAX_COINS,
            UTXO_FLUSH_PERIOD,
            DEF_BLOCK_BATCH,
            move |h, block| {
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
