use crate::filter::get_filters_height;
use bitcoin_utxo::storage::chain::get_chain_height;
use prometheus::{self, Encoder, IntCounter, IntGauge, TextEncoder};
use rocksdb::DB;
use std::net::SocketAddr;
use std::sync::Arc;
use warp::{Filter, Rejection};
use crate::server::block_explorer::*;

lazy_static! {
    pub static ref ACTIVE_CONNS_GAUGE: IntGauge =
        register_int_gauge!("active_connections", "Amount of opened TCP connections").unwrap();
    pub static ref FILTERS_SERVED_COUNTER: IntCounter =
        register_int_counter!("filters_served", "Amount of served filters").unwrap();
    pub static ref BTC_HEIGHT_GAUGE: IntGauge =
        register_int_gauge!("btc_current_height", "Reported height of BTC currency").unwrap();
    pub static ref BTC_SCAN_GAUGE: IntGauge = register_int_gauge!(
        "btc_scanned_height",
        "Amount of scanned blocks for BTC currency"
    )
    .unwrap();
    pub static ref BTC_BLOCK_EXPLORER_HEIGHT: IntGauge = register_int_gauge!(
        "btc_block_explorer_height",
        "BTC height according to block explorer"
    )
    .unwrap();
    pub static ref SPACE_GAUGE: IntGauge = register_int_gauge!(
        "available_space",
        "Amount of space left for indecies until the server stops"
    )
    .unwrap();
}

pub async fn serve_metrics(addr: SocketAddr, db: Arc<DB>, db_path: String) {
    let btc_actual_height = ask_btc_actual_height().await;

    match btc_actual_height {
      Ok(height) => BTC_BLOCK_EXPLORER_HEIGHT.set (height),
      _ => BTC_BLOCK_EXPLORER_HEIGHT.set(0)
    } 
    
    ACTIVE_CONNS_GAUGE.set(0);
    FILTERS_SERVED_COUNTER.inc_by(0);
    BTC_HEIGHT_GAUGE.set(get_chain_height(&db) as i64);
    BTC_SCAN_GAUGE.set(get_filters_height(&db) as i64);
    SPACE_GAUGE.set(fs2::available_space(db_path.clone()).unwrap_or(0) as i64);
    
    
    let metrics = warp::path::end().and_then(move || {
        let db = db.clone();
        let db_path = db_path.clone();
        async move {
            let btc_actual_height = ask_btc_actual_height().await;
            match btc_actual_height {
              Ok(height) => BTC_BLOCK_EXPLORER_HEIGHT.set (height),
              _ => BTC_BLOCK_EXPLORER_HEIGHT.set(0)
            }
            BTC_HEIGHT_GAUGE.set(get_chain_height(&db) as i64);
            BTC_SCAN_GAUGE.set(get_filters_height(&db) as i64);
            SPACE_GAUGE.set(fs2::available_space(&db_path).unwrap_or(0) as i64);
    
            let mut buffer = Vec::new();
            let metric_families = prometheus::gather();
            TextEncoder::new()
                .encode(&metric_families, &mut buffer)
                .unwrap();
                Result::<_, Rejection>::Ok(String::from_utf8(buffer).unwrap())
        }
 
    });
    let routes = warp::get().and(metrics);

    warp::serve(routes).run(addr).await;
}
