use serde::de::DeserializeOwned;

const BASE_URL: &str = "https://blockstream.info/";
const BASE_TESTNET_URL: &str = "https://blockstream.info/testnet/";
const ACTUAL_HEIGHT: &str = "api/blocks/tip/height";

async fn ask_explorer<T: DeserializeOwned>(is_testnet : bool, relative_url: &str) -> Result<T, reqwest::Error> {
    let url = (if is_testnet { BASE_TESTNET_URL } else { BASE_URL }).to_owned() + relative_url;
    let result = reqwest::get(url).await?.json().await?;
    Ok(result)
  }

pub async fn ask_btc_actual_height(is_testnet : bool) -> Result<i64, reqwest::Error>{
    let actual_height = ask_explorer (is_testnet, ACTUAL_HEIGHT).await?;
    Ok(actual_height)
  }
