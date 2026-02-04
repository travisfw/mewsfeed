use activitypub_integrity::LinkTypes;
use activitypub_types::RemoteFollower;
use hdk::prelude::*;

/// Create a RemoteFollower entry (inbound fediverse follower).
/// Uses a Path-based anchor to prevent duplicate follower entries from the same remote actor.
#[hdk_extern]
pub fn create_remote_follower(follower: RemoteFollower) -> ExternResult<ActionHash> {
    let agent = follower.local_agent.clone();

    // Create dedup anchor using local_agent + remote_actor_uri
    let dedup_anchor = Path::from(format!("follower:{}:{}", agent, follower.remote_actor_uri))
        .path_entry_hash()?;

    // Check if this follower relationship already exists
    let links = get_links(
        LinkQuery::new(
            dedup_anchor.clone(),
            LinkTypes::FollowerDedupToRemoteFollower.try_into_filter()?,
        ),
        GetStrategy::Network,
    )?;

    if let Some(link) = links.first() {
        // Follower already exists, return existing hash
        let existing_hash = ActionHash::try_from(link.target.clone()).map_err(|_| {
            wasm_error!("FollowerDedupToRemoteFollower target is not an ActionHash")
        })?;
        return Ok(existing_hash);
    }

    // Create new follower entry and links
    let action_hash = create_entry(activitypub_integrity::EntryTypes::RemoteFollower(follower))?;
    create_link(
        agent,
        action_hash.clone(),
        LinkTypes::AgentToRemoteFollowers,
        (),
    )?;
    create_link(
        dedup_anchor,
        action_hash.clone(),
        LinkTypes::FollowerDedupToRemoteFollower,
        (),
    )?;
    Ok(action_hash)
}

/// List all inbound remote followers for an agent.
#[hdk_extern]
pub fn list_remote_followers(agent: AgentPubKey) -> ExternResult<Vec<RemoteFollower>> {
    let links = get_links(
        LinkQuery::new(agent, LinkTypes::AgentToRemoteFollowers.try_into_filter()?),
        GetStrategy::Network,
    )?;
    let mut followers = Vec::new();
    for link in links {
        let action_hash = ActionHash::try_from(link.target)
            .map_err(|_| wasm_error!("AgentToRemoteFollowers target is not an ActionHash"))?;
        if let Some(record) = get(action_hash, GetOptions::default())? {
            if let Some(follower) = record
                .entry()
                .to_app_option::<RemoteFollower>()
                .map_err(|_e| wasm_error!("Deserialize RemoteFollower"))?
            {
                followers.push(follower);
            }
        }
    }
    Ok(followers)
}

/// Delete a RemoteFollower and its links (including dedup anchor link).
#[hdk_extern]
pub fn delete_remote_follower(follower_hash: ActionHash) -> ExternResult<()> {
    let record = get(follower_hash.clone(), GetOptions::default())?
        .ok_or(wasm_error!("RemoteFollower record not found"))?;
    let follower: RemoteFollower = record
        .entry()
        .to_app_option()
        .map_err(|_e| wasm_error!("Deserialize RemoteFollower"))?
        .ok_or(wasm_error!("RemoteFollower entry missing"))?;

    // Delete AgentToRemoteFollowers link
    let links = get_links(
        LinkQuery::new(
            follower.local_agent.clone(),
            LinkTypes::AgentToRemoteFollowers.try_into_filter()?,
        ),
        GetStrategy::Local,
    )?;
    for link in links {
        if ActionHash::try_from(link.target.clone()).ok().as_ref() == Some(&follower_hash) {
            delete_link(link.create_link_hash, GetOptions::local())?;
        }
    }

    // Delete dedup anchor link
    let dedup_anchor = Path::from(format!(
        "follower:{}:{}",
        follower.local_agent, follower.remote_actor_uri
    ))
    .path_entry_hash()?;
    let dedup_links = get_links(
        LinkQuery::new(
            dedup_anchor,
            LinkTypes::FollowerDedupToRemoteFollower.try_into_filter()?,
        ),
        GetStrategy::Local,
    )?;
    for link in dedup_links {
        if ActionHash::try_from(link.target.clone()).ok().as_ref() == Some(&follower_hash) {
            delete_link(link.create_link_hash, GetOptions::local())?;
        }
    }

    delete_entry(follower_hash)?;
    Ok(())
}
