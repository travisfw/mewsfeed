use activitypub_integrity::LinkTypes;
use activitypub_types::RemoteActorRef;
use hdk::prelude::*;

/// Insert or update a cached remote actor reference. Uses the actor URI as a
/// dedup key via a Path-based anchor link.
#[hdk_extern]
pub fn upsert_remote_actor_ref(actor: RemoteActorRef) -> ExternResult<ActionHash> {
    let uri_anchor = Path::from(format!("actor_uri:{}", actor.actor_uri)).path_entry_hash()?;
    let links = get_links(
        LinkQuery::new(
            uri_anchor.clone(),
            LinkTypes::ActorUriToRemoteActorRef.try_into_filter()?,
        ),
        GetStrategy::Network,
    )?;
    if let Some(link) = links.first() {
        let existing_hash = ActionHash::try_from(link.target.clone())
            .map_err(|_| wasm_error!("ActorUriToRemoteActorRef target is not an ActionHash"))?;

        // Get existing entry to compare for changes
        let record = get(existing_hash.clone(), GetOptions::default())?
            .ok_or_else(|| wasm_error!("RemoteActorRef record not found"))?;
        let existing_actor: RemoteActorRef = record
            .entry()
            .to_app_option()
            .map_err(|e| {
                wasm_error!(WasmErrorInner::Guest(format!(
                    "Deserialize RemoteActorRef: {}",
                    e
                )))
            })?
            .ok_or_else(|| wasm_error!("RemoteActorRef entry missing"))?;

        // Only update if there are actual changes
        let has_changes = existing_actor.actor_uri != actor.actor_uri
            || existing_actor.handle != actor.handle
            || existing_actor.display_name != actor.display_name;

        if has_changes {
            let new_hash = update_entry(
                existing_hash,
                activitypub_integrity::EntryTypes::RemoteActorRef(actor),
            )?;
            Ok(new_hash)
        } else {
            // No changes, return existing hash without updating DHT
            Ok(existing_hash)
        }
    } else {
        let action_hash = create_entry(activitypub_integrity::EntryTypes::RemoteActorRef(actor))?;
        create_link(
            uri_anchor,
            action_hash.clone(),
            LinkTypes::ActorUriToRemoteActorRef,
            (),
        )?;
        Ok(action_hash)
    }
}

/// Retrieve a cached remote actor reference by URI.
#[hdk_extern]
pub fn get_remote_actor_ref(actor_uri: String) -> ExternResult<Option<RemoteActorRef>> {
    let uri_anchor = Path::from(format!("actor_uri:{actor_uri}")).path_entry_hash()?;
    let links = get_links(
        LinkQuery::new(
            uri_anchor,
            LinkTypes::ActorUriToRemoteActorRef.try_into_filter()?,
        ),
        GetStrategy::Network,
    )?;
    let link = match links.first() {
        Some(l) => l,
        None => return Ok(None),
    };
    let action_hash = ActionHash::try_from(link.target.clone())
        .map_err(|_| wasm_error!("ActorUriToRemoteActorRef target is not an ActionHash"))?;
    let record = get(action_hash, GetOptions::default())?;
    match record {
        Some(r) => {
            let actor: RemoteActorRef = r
                .entry()
                .to_app_option()
                .map_err(|_e| wasm_error!("Deserialize RemoteActorRef"))?
                .ok_or(wasm_error!("RemoteActorRef entry missing"))?;
            Ok(Some(actor))
        }
        None => Ok(None),
    }
}
