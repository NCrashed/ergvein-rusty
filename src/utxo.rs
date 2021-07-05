use bitcoin::consensus::encode::{self, Decodable, Encodable};
use bitcoin::{BlockHeader, Script, Transaction};
use bitcoin_utxo::utxo::UtxoState;
use std::io;

#[derive(Debug, Clone)]
pub struct FilterCoin {
    pub script: Script,
}

pub fn coin_script(fc: &FilterCoin) -> Script {
    fc.script.clone()
}

impl UtxoState for FilterCoin {
    fn new_utxo(_height: u32, _header: &BlockHeader, tx: &Transaction, vout: u32) -> Self {
        FilterCoin {
            script: tx.output[vout as usize].script_pubkey.clone(),
        }
    }
}

impl Encodable for FilterCoin {
    fn consensus_encode<W: io::Write>(&self, writer: W) -> Result<usize, io::Error> {
        let len = self.script.consensus_encode(writer)?;
        Ok(len)
    }
}
impl Decodable for FilterCoin {
    fn consensus_decode<D: io::Read>(mut d: D) -> Result<Self, encode::Error> {
        Ok(FilterCoin {
            script: Decodable::consensus_decode(&mut d)?,
        })
    }
}
