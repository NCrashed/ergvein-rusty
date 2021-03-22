use serde::Deserialize;
use reqwest::Error;
use std::sync::{Arc, Mutex};
use tokio::time::Duration;

#[derive(Debug)]
pub struct FeesCache {
    pub btc: BtcFee,
}

impl Default for FeesCache {
    fn default() -> Self {
        FeesCache {
            btc: BtcFee::default(),
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BtcFee {
    pub fastest_fee: u32,
    pub half_hour_fee: u32,
    pub hour_fee: u32,
}

impl Default for BtcFee {
    fn default() -> Self {
        BtcFee {
            fastest_fee: 0,
            half_hour_fee: 0,
            hour_fee: 0,
        }
    }
}

async fn request_btc_fees() -> Result<BtcFee, Error> {
    let request_url = "https://bitcoinfees.earn.com/api/v1/fees/recommended";
    let response = reqwest::get(request_url).await?;
    Ok(response.json().await)?
}

pub async fn fees_requester(cache: Arc<Mutex<FeesCache>>) {
    loop {
        match request_btc_fees().await {
            Err(err) => {
                eprintln!("Failed to request BTC fees: {:?}", err);
            }
            Ok(fee) => {
                let mut cache = cache.lock().unwrap();
                println!("Bitcoin fees are {:?}", fee);
                cache.btc = fee;
            }
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}
