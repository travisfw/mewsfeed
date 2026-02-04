use activitypub_integrity::LinkTypes;
use activitypub_types::RemoteInteraction;
use hdk::prelude::*;

/// Create a RemoteInteraction entry (like/boost/reply from fediverse).
/// Uses the activity_uri as a dedup key to prevent duplicate interactions.
#[hdk_extern]
pub fn create_remote_interaction(interaction: RemoteInteraction) -> ExternResult<ActionHash> {
    let mew_hash = interaction.mew_hash.clone();

    // Create dedup anchor using globally unique activity_uri
    let dedup_anchor = Path::from(format!("interaction_activity:{}", interaction.activity_uri))
        .path_entry_hash()?;

    // Check if this interaction already exists
    let links = get_links(
        LinkQuery::new(
            dedup_anchor.clone(),
            LinkTypes::InteractionDedupToRemoteInteraction.try_into_filter()?,
        ),
        GetStrategy::Network,
    )?;

    if let Some(link) = links.first() {
        // Interaction already exists, return existing hash
        let existing_hash = ActionHash::try_from(link.target.clone()).map_err(|_| {
            wasm_error!("InteractionDedupToRemoteInteraction target is not an ActionHash")
        })?;
        return Ok(existing_hash);
    }

    // Create new interaction entry and links
    let action_hash = create_entry(activitypub_integrity::EntryTypes::RemoteInteraction(
        interaction,
    ))?;
    create_link(
        mew_hash,
        action_hash.clone(),
        LinkTypes::MewToRemoteInteractions,
        (),
    )?;
    create_link(
        dedup_anchor,
        action_hash.clone(),
        LinkTypes::InteractionDedupToRemoteInteraction,
        (),
    )?;
    Ok(action_hash)
}

/// List all remote interactions on a mew.
#[hdk_extern]
pub fn list_remote_interactions(mew_hash: ActionHash) -> ExternResult<Vec<RemoteInteraction>> {
    let links = get_links(
        LinkQuery::new(
            mew_hash,
            LinkTypes::MewToRemoteInteractions.try_into_filter()?,
        ),
        GetStrategy::Network,
    )?;
    let mut interactions = Vec::new();
    for link in links {
        let action_hash = ActionHash::try_from(link.target)
            .map_err(|_| wasm_error!("MewToRemoteInteractions target is not an ActionHash"))?;
        if let Some(record) = get(action_hash, GetOptions::default())? {
            if let Some(interaction) = record
                .entry()
                .to_app_option::<RemoteInteraction>()
                .map_err(|_e| wasm_error!("Deserialize RemoteInteraction"))?
            {
                interactions.push(interaction);
            }
        }
    }
    Ok(interactions)
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DeleteRemoteInteractionInput {
    pub mew_hash: ActionHash,
    pub activity_uri: String,
}

/// Delete a RemoteInteraction by mew hash and activity URI.
/// Also deletes the dedup anchor link.
#[hdk_extern]
pub fn delete_remote_interaction(input: DeleteRemoteInteractionInput) -> ExternResult<()> {
    let links = get_links(
        LinkQuery::new(
            input.mew_hash,
            LinkTypes::MewToRemoteInteractions.try_into_filter()?,
        ),
        GetStrategy::Local,
    )?;
    for link in links {
        let action_hash = match ActionHash::try_from(link.target.clone()) {
            Ok(h) => h,
            Err(_) => continue,
        };
        if let Some(record) = get(action_hash.clone(), GetOptions::default())? {
            if let Some(interaction) = record
                .entry()
                .to_app_option::<RemoteInteraction>()
                .map_err(|_e| wasm_error!("Deserialize RemoteInteraction"))?
            {
                if interaction.activity_uri == input.activity_uri {
                    // Delete MewToRemoteInteractions link
                    delete_link(link.create_link_hash, GetOptions::local())?;

                    // Delete dedup anchor link
                    let dedup_anchor =
                        Path::from(format!("interaction_activity:{}", interaction.activity_uri))
                            .path_entry_hash()?;
                    let dedup_links = get_links(
                        LinkQuery::new(
                            dedup_anchor,
                            LinkTypes::InteractionDedupToRemoteInteraction.try_into_filter()?,
                        ),
                        GetStrategy::Local,
                    )?;
                    for dedup_link in dedup_links {
                        if ActionHash::try_from(dedup_link.target.clone())
                            .ok()
                            .as_ref()
                            == Some(&action_hash)
                        {
                            delete_link(dedup_link.create_link_hash, GetOptions::local())?;
                        }
                    }

                    delete_entry(action_hash)?;
                    return Ok(());
                }
            }
        }
    }
    Err(wasm_error!("RemoteInteraction with activity_uri not found"))
}
