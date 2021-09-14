use super::utxo::FilterCoin;
use bitcoin::consensus::encode;
use bitcoin::network::message::NetworkMessage;
use bitcoin::util::bip158;
use bitcoin::{Block, BlockHash, OutPoint, Script};
use bitcoin_utxo::cache::utxo::*;
use bitcoin_utxo::storage::chain::*;
use bitcoin_utxo::sync::utxo::*;
use byteorder::{BigEndian, ByteOrder};
use ergvein_filters::btc::ErgveinFilter;
use futures::future::{AbortHandle, AbortRegistration, Abortable, Aborted};
use rocksdb::{WriteBatch, WriteOptions, DB};

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::Duration;

use crate::server::block_explorer::*;

pub async fn generate_filter(
    db: Arc<DB>,
    cache: Arc<UtxoCache<FilterCoin>>,
    h: u32,
    block: Block,
) -> Result<ErgveinFilter, UtxoSyncError> {
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
                .await?;
                hashmap.insert(i.previous_output, coin.script);
            }
        }
    }
    ErgveinFilter::new_script_filter(&block, |out| {
        hashmap
            .get(out)
            .map_or(Err(bip158::Error::UtxoMissing(*out)), |s| Ok(s.clone()))
    })
    .map_err(|e| UtxoSyncError::UserWith(Box::new(e)))
}

fn height_value(h: u32) -> [u8; 4] {
    let mut buf = [0; 4];
    BigEndian::write_u32(&mut buf, h);
    buf
}

pub fn get_filters_height(db: &DB) -> u32 {
    let cf = db.cf_handle("filters").unwrap();
    db.get_cf(cf, b"height")
        .unwrap()
        .map_or(0, |bs| BigEndian::read_u32(&bs))
}

/// Makes futures that polls chain height and finishes when it is changed
pub async fn filters_height_changes(db: &DB, dur: Duration) -> u32 {
    let starth = get_filters_height(db);
    let mut curh = starth;
    while curh == starth {
        tokio::time::sleep(dur).await;
        curh = get_filters_height(db);
    }
    curh
}

pub fn store_filter(
    db: &DB,
    hash: &BlockHash,
    height: u32,
    filter: ErgveinFilter,
) -> Result<(), rocksdb::Error> {
    let cf = db.cf_handle("filters").unwrap();
    let curh = db
        .get_cf(cf, b"height")?
        .map_or(0, |bs| BigEndian::read_u32(&bs));
    let mut batch = WriteBatch::default();
    batch.put_cf(cf, hash, filter.content);
    if height > curh {
        batch.put_cf(cf, b"height", height_value(height));
    }

    let mut write_options = WriteOptions::default();
    write_options.set_sync(true);
    write_options.disable_wal(false);
    db.write_opt(batch, &write_options)
}

pub fn read_filters(db: &DB, start: u32, amount: u32) -> Vec<(BlockHash, ErgveinFilter)> {
    let cf = db.cf_handle("filters").unwrap();
    let mut res = vec![];
    for h in start..start + amount {
        if let Some(hash) = get_block_hash(&db, h) {
            if let Some(filter) = db.get_cf(cf, hash).unwrap() {
                let efilter = ErgveinFilter { content: filter };
                res.push((hash, efilter));
            }
        }
    }
    res
}

pub async fn sync_filters(
    db: Arc<DB>,
    cache: Arc<UtxoCache<FilterCoin>>,
    fork_height: u32,
    max_coins: usize,
    flush_period: u32,
    block_batch: usize,
) -> (
    impl futures::Sink<NetworkMessage, Error = encode::Error>,
    impl Unpin + futures::Stream<Item = NetworkMessage>,
    Arc<Mutex<()>>,
    AbortHandle,
    AbortRegistration,
) {
    let cache = cache.clone();
    let sync_db = db.clone();
    let (sync_future, sync_mutex, utxo_stream, utxo_sink) = sync_utxo_with(
        db.clone(),
        cache.clone(),
        fork_height,
        max_coins,
        flush_period,
        block_batch,
        move |h, block| {
            let block = block.clone();
            let db = sync_db.clone();
            let cache = cache.clone();
            async move {
                let hash = block.block_hash();
                let filter = generate_filter(db.clone(), cache, h, block).await?;
                if h % 1000 == 0 {
                    println!(
                        "Filter for block {:?}: {:?}",
                        h,
                        hex::encode(&filter.content)
                    );
                }
                store_filter(&db, &hash, h, filter)
                    .map_err(|e| UtxoSyncError::UserWith(Box::new(e)))
            }
        },
    )
    .await;

    let (abort_handle, abort_registration) = AbortHandle::new_pair();
    let (restart_handle, restart_registration) = AbortHandle::new_pair();
    {
        let db = db.clone();
        let sync_mutex = sync_mutex.clone();
        tokio::spawn(async move {
            let discrepancy_watch_feature = tokio::spawn(discrepancy_watch(db, sync_mutex));
            let res_feature = Abortable::new(sync_future, abort_registration);
            tokio::select! {
                _ = discrepancy_watch_feature => {
                    restart_handle.abort();
                }
                res = res_feature => {
                    match res {
                        Err(Aborted) => eprintln!("Sync task was aborted!"),
                        Ok(Err(e)) => {
                            eprintln!("Sync was failed with error: {}", e);
                            restart_handle.abort();
                        }
                        Ok(Ok(())) => (),
                    }
                }
            };
        });
    }

    (
        utxo_sink,
        utxo_stream,
        sync_mutex,
        abort_handle,
        restart_registration,
    )
}

async fn discrepancy_watch (db: Arc<DB>, sync_mutex: Arc<Mutex<()>>) -> () {
    let delay = Duration::from_secs_f64(3600.0);
    let max_discrepancy = 4;
    sync_mutex.lock().await;
    loop {
        let btc_scanned_height = get_chain_height(&db) as i64;
        let btc_actual_height = ask_btc_actual_height().await.unwrap();
        tokio::time::sleep(delay).await;
        eprintln! ("{}", btc_actual_height - btc_scanned_height);
        if (btc_actual_height - btc_scanned_height) > max_discrepancy {break;}
    }

}
