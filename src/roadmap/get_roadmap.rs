use super::{add_roadmap_item::RoadmapItem, note::base64_maybe_roadmap_note_to_roadmap_item};
use algonaut::{
    core::Address, crypto::HashDigest, indexer::v2::Indexer, model::indexer::v2::QueryTransaction,
};
use anyhow::{anyhow, Error, Result};
use chrono::{DateTime, Utc};
use mbase::{date_util::timestamp_seconds_to_date, models::{dao_id::DaoId, tx_id::TxId}};
use serde::Serialize;

pub async fn get_roadmap(
    indexer: &Indexer,
    dao_creator: &Address,
    dao_id: DaoId,
) -> Result<Roadmap> {
    // We get all the txs sent by dao's creator and filter manually by the dao prefix
    // Algorand's indexer has performance problems with note-prefix and it doesn't work at all with AlgoExplorer or PureStake currently:
    // https://github.com/algorand/indexer/issues/358
    // https://github.com/algorand/indexer/issues/669

    let response = indexer
        .transactions(&QueryTransaction {
            address: Some(dao_creator.to_string()),
            // indexer disabled this, for performance apparently https://github.com/algorand/indexer/commit/1216e7957d5fba7c6a858e244a2aaf7e99412e5d
            // so we filter locally
            // address_role: Some(Role::Sender),
            ..QueryTransaction::default()
        })
        .await?;

    let mut roadmap_items = vec![];

    for tx in response.transactions {
        let sender_address = tx.sender.parse::<Address>().map_err(Error::msg)?;
        if &sender_address == dao_creator {
            // Round time is documented as optional (https://developer.algorand.org/docs/rest-apis/indexer/#transaction)
            // Unclear when it's None. For now we just reject it.
            let round_time = tx
                .round_time
                .ok_or_else(|| anyhow!("Unexpected: tx has no round time: {:?}", tx))?;

            if tx.payment_transaction.is_some() {
                if let Some(note) = tx.note.clone() {
                    if let Some(roadmap_item) =
                        base64_maybe_roadmap_note_to_roadmap_item(&note, dao_id)?
                    {
                        let id = tx
                            .id
                            .clone()
                            .ok_or_else(|| anyhow!("Unexpected: tx has no id: {:?}", tx))?;

                        let saved_roadmap_item =
                            to_saved_roadmap_item(&roadmap_item, &id.parse()?, round_time)?;
                        roadmap_items.push(saved_roadmap_item);
                    }
                }
            }
        }
    }

    Ok(Roadmap {
        items: roadmap_items,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct Roadmap {
    pub items: Vec<SavedRoadmapItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SavedRoadmapItem {
    pub tx_id: TxId,
    pub dao_id: DaoId,
    pub title: String,
    pub date: DateTime<Utc>,
    pub saved_date: DateTime<Utc>,
    pub parent: Box<Option<HashDigest>>,
    pub hash: HashDigest,
}

fn to_saved_roadmap_item(
    item: &RoadmapItem,
    tx_id: &TxId,
    round_time: u64,
) -> Result<SavedRoadmapItem> {
    Ok(SavedRoadmapItem {
        tx_id: tx_id.clone(),
        dao_id: item.dao_id,
        title: item.title.clone(),
        date: item.date,
        saved_date: timestamp_seconds_to_date(round_time)?,
        parent: item.parent.clone(),
        hash: item.hash,
    })
}
