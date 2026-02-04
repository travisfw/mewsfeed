use activitypub_integrity::LinkTypes;
use activitypub_types::{RemoteFollow, RemoteFollowStatus};
use hdk::prelude::*;

#[derive(Serialize, Deserialize, Debug)]
pub struct UpdateFollowStatusInput {
    pub follow_hash: ActionHash,
    pub status: RemoteFollowStatus,
}

/// Create a RemoteFollow entry (outbound cross-network follow).
/// Uses a Path-based anchor to prevent duplicate follows to the same remote actor.
#[hdk_extern]
pub fn create_remote_follow(follow: RemoteFollow) -> ExternResult<ActionHash> {
    let agent = follow.local_agent.clone();

    // Create dedup anchor using local_agent + remote_actor_uri
    let dedup_anchor =
        Path::from(format!("follow:{}:{}", agent, follow.remote_actor_uri)).path_entry_hash()?;

    // Check if this follow relationship already exists
    let links = get_links(
        LinkQuery::new(
            dedup_anchor.clone(),
            LinkTypes::FollowDedupToRemoteFollow.try_into_filter()?,
        ),
        GetStrategy::Network,
    )?;

    if let Some(link) = links.first() {
        // Follow already exists, return existing hash
        let existing_hash = ActionHash::try_from(link.target.clone())
            .map_err(|_| wasm_error!("FollowDedupToRemoteFollow target is not an ActionHash"))?;
        return Ok(existing_hash);
    }

    // Create new follow entry and links
    let action_hash = create_entry(activitypub_integrity::EntryTypes::RemoteFollow(follow))?;
    create_link(
        agent,
        action_hash.clone(),
        LinkTypes::AgentToRemoteFollows,
        (),
    )?;
    create_link(
        dedup_anchor,
        action_hash.clone(),
        LinkTypes::FollowDedupToRemoteFollow,
        (),
    )?;
    Ok(action_hash)
}

/// Update the status of a RemoteFollow (e.g., Pending -> Accepted).
#[hdk_extern]
pub fn update_remote_follow_status(input: UpdateFollowStatusInput) -> ExternResult<()> {
    let record = get(input.follow_hash.clone(), GetOptions::default())?
        .ok_or(wasm_error!("RemoteFollow record not found"))?;
    let mut follow: RemoteFollow = record
        .entry()
        .to_app_option()
        .map_err(|_e| wasm_error!("Deserialize RemoteFollow"))?
        .ok_or(wasm_error!("RemoteFollow entry missing"))?;
    follow.status = input.status;
    update_entry(
        record.action_address().clone(),
        activitypub_integrity::EntryTypes::RemoteFollow(follow),
    )?;
    Ok(())
}

/// List all outbound remote follows for an agent.
#[hdk_extern]
pub fn list_remote_follows(agent: AgentPubKey) -> ExternResult<Vec<RemoteFollow>> {
    let links = get_links(
        LinkQuery::new(agent, LinkTypes::AgentToRemoteFollows.try_into_filter()?),
        GetStrategy::Network,
    )?;
    let mut follows = Vec::new();
    for link in links {
        let action_hash = ActionHash::try_from(link.target)
            .map_err(|_| wasm_error!("AgentToRemoteFollows target is not an ActionHash"))?;
        if let Some(record) = get(action_hash, GetOptions::default())? {
            if let Some(follow) = record
                .entry()
                .to_app_option::<RemoteFollow>()
                .map_err(|_e| wasm_error!("Deserialize RemoteFollow"))?
            {
                follows.push(follow);
            }
        }
    }
    Ok(follows)
}

/// Delete a RemoteFollow and its links (including dedup anchor link).
#[hdk_extern]
pub fn delete_remote_follow(follow_hash: ActionHash) -> ExternResult<()> {
    let record = get(follow_hash.clone(), GetOptions::default())?
        .ok_or(wasm_error!("RemoteFollow record not found"))?;
    let follow: RemoteFollow = record
        .entry()
        .to_app_option()
        .map_err(|_e| wasm_error!("Deserialize RemoteFollow"))?
        .ok_or(wasm_error!("RemoteFollow entry missing"))?;

    // Delete AgentToRemoteFollows link
    let links = get_links(
        LinkQuery::new(
            follow.local_agent.clone(),
            LinkTypes::AgentToRemoteFollows.try_into_filter()?,
        ),
        GetStrategy::Local,
    )?;
    for link in links {
        if ActionHash::try_from(link.target.clone()).ok().as_ref() == Some(&follow_hash) {
            delete_link(link.create_link_hash, GetOptions::local())?;
        }
    }

    // Delete dedup anchor link
    let dedup_anchor = Path::from(format!(
        "follow:{}:{}",
        follow.local_agent, follow.remote_actor_uri
    ))
    .path_entry_hash()?;
    let dedup_links = get_links(
        LinkQuery::new(
            dedup_anchor,
            LinkTypes::FollowDedupToRemoteFollow.try_into_filter()?,
        ),
        GetStrategy::Local,
    )?;
    for link in dedup_links {
        if ActionHash::try_from(link.target.clone()).ok().as_ref() == Some(&follow_hash) {
            delete_link(link.create_link_hash, GetOptions::local())?;
        }
    }

    delete_entry(follow_hash)?;
    Ok(())
}
