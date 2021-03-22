use dashmap::DashMap;
use ergvein_protocol::message::*;
use std::error::Error;
use serde::Deserialize;
use std::sync::Arc;
use std::num::ParseFloatError;
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
#[serde(rename_all = "UPPERCASE")]
struct ExchangeRates {
    usd: String,
    eur: String,
    rub: String,
}

fn parse_rate(a: String) -> Result<Rate, ParseFloatError> {
    let f: f64 = a.parse()?;
    Ok(f64_to_centi(f))
}

fn f64_to_centi(a: f64) -> Rate {
    Rate::new((a * 100.).round() as u64)
}

async fn request_btc_rates() -> Result<DashMap<Fiat, Rate>, Box<dyn Error>> {
    let request_url = "https://api.coinbase.com/v2/exchange-rates?currency=BTC";
    let response : CoinbaseResp<CoinbaseRates> = reqwest::get(request_url).await?.json().await?;
    let rates = DashMap::new();
    rates.insert(Fiat::Usd, parse_rate(response.data.rates.usd)?);
    rates.insert(Fiat::Eur, parse_rate(response.data.rates.eur)?);
    rates.insert(Fiat::Rub, parse_rate(response.data.rates.rub)?);
    Ok(rates)
}

pub async fn rates_requester(cache: Arc<RatesCache>) {
    loop {
        match request_btc_rates().await {
            Err(err) => {
                eprintln!("Failed to request BTC rates: {:?}", err);
            }
            Ok(rates) => {
                println!("Bitcoin rates are {:?}", rates);
                cache.insert(Currency::Btc, rates);
            }
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}
