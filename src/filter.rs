use super::utxo::FilterCoin;
use bitcoin::util::bip158;
use bitcoin::{Block, BlockHash, OutPoint, Script};
use bitcoin_utxo::cache::utxo::*;
use ergvein_filters::btc::ErgveinFilter;
use rocksdb::{WriteBatch, WriteOptions, DB};
use std::collections::HashMap;
use std::sync::Arc;
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
