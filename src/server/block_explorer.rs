use serde::de::DeserializeOwned;

const BASE_URL: &str = "https://blockstream.info/";
const ACTUAL_HEIGHT: &str = "api/blocks/tip/height";

async fn ask_explorer<T: DeserializeOwned>(relative_url: &str) -> Result<T, reqwest::Error> {
    let url = BASE_URL.to_owned() + relative_url;
    let result = reqwest::get(url).await?.json().await?;
    Ok(result)
  }

pub async fn ask_btc_actual_height() -> Result<i64, reqwest::Error>{
    let actual_height = ask_explorer (ACTUAL_HEIGHT).await?;
    Ok(actual_height)
  }
