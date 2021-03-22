use dashmap::DashMap;
use ergvein_protocol::message::*;
// use reqwest::Error;
use std::error::Error;
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use tokio::time::Duration;

pub type RatesCache = DashMap<Currency, DashMap<Fiat, Rate>>;

pub fn new_rates_cache() -> RatesCache {
    DashMap::new()
}

#[derive(Deserialize, Debug)]
struct CoinbaseResp<T>{ data: T }

#[derive(Deserialize, Debug)]
struct CoinbaseRates {
    currency: String,
    rates: ExchangeRates,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ExchangeRates {
    usd: f64,
    eur: f64,
    rub: f64,
}

fn f64_to_centi(a: f64) -> Rate {
    Rate::new((a * 100.).round() as u64)
}

async fn request_btc_rates() -> Result<DashMap<Fiat, Rate>, Box<dyn Error>> {
    let request_url = "https://api.coinbase.com/v2/exchange-rates?currency=BTC";
    let response : CoinbaseResp<CoinbaseRates> = reqwest::get(request_url).await?.json().await?;
    let rates = DashMap::new();
    rates.insert(Fiat::Usd, f64_to_centi(response.data.rates.usd));
    rates.insert(Fiat::Eur, f64_to_centi(response.data.rates.eur));
    rates.insert(Fiat::Rub, f64_to_centi(response.data.rates.rub));
    Ok(rates)
}
