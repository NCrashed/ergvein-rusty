use prometheus::{self, Encoder, IntCounter, IntGauge, TextEncoder};
use std::net::SocketAddr;
use warp::Filter;

lazy_static! {
    static ref ACTIVE_CONNS_GAUGE: IntGauge =
        register_int_gauge!("active_connections", "Amount of opened TCP connections").unwrap();
    static ref FILTERS_SERVED_COUNTER: IntCounter =
        register_int_counter!("filters_served", "Amount of served filters").unwrap();
    static ref BTC_HEIGHT_GAUGE: IntGauge =
        register_int_gauge!("btc_current_height", "Reported height of BTC currency").unwrap();
    static ref SCAN_GAUGE: IntGauge = register_int_gauge!(
        "btc_scanned_height",
        "Amount of scanned blocks for BTC currency"
    )
    .unwrap();
    static ref SPACE_GAUGE: IntGauge = register_int_gauge!(
        "available_space",
        "Amount of space left for indecies until the server stops"
    )
    .unwrap();
}

pub async fn serve_metrics(addr: SocketAddr) {
    let metrics = warp::path::end().map(|| {
        let mut buffer = Vec::new();
        let metric_families = prometheus::gather();
        TextEncoder::new().encode(&metric_families, &mut buffer).unwrap();
        String::from_utf8(buffer.clone()).unwrap()
    });
    let routes = warp::get().and(metrics);

    warp::serve(routes).run(addr).await;
}
