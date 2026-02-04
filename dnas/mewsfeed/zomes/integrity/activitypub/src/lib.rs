use activitypub_types::*;
use hdi::prelude::*;

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
#[hdk_entry_types]
#[unit_enum(UnitEntryTypes)]
pub enum EntryTypes {
    InstanceConfig(InstanceConfig),
    MewFederationState(MewFederationState),
    RemoteActorRef(RemoteActorRef),
    RemoteFollow(RemoteFollow),
    RemoteFollower(RemoteFollower),
    RemoteInteraction(RemoteInteraction),
}

#[derive(Serialize, Deserialize)]
#[hdk_link_types]
pub enum LinkTypes {
    MewToFederationState,
    AgentToRemoteFollows,
    AgentToRemoteFollowers,
    MewToRemoteInteractions,
    AnchorToInstanceConfig,
    ApUriToFederationState,
    ActorUriToRemoteActorRef,
    FollowDedupToRemoteFollow,
    FollowerDedupToRemoteFollower,
    InteractionDedupToRemoteInteraction,
}

#[hdk_extern]
pub fn genesis_self_check(_data: GenesisSelfCheckData) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Valid)
}

pub fn validate_agent_joining(
    _agent_pub_key: AgentPubKey,
    _membrane_proof: &Option<MembraneProof>,
) -> ExternResult<ValidateCallbackResult> {
    Ok(ValidateCallbackResult::Valid)
}

/// Validate that a MewFederationState is authored by the mew's author.
fn validate_mew_federation_state(
    action: EntryCreationAction,
    state: MewFederationState,
) -> ExternResult<ValidateCallbackResult> {
    let mew_action = must_get_action(state.mew_hash.clone())?;
    let mew_author = mew_action.hashed.content.author().clone();
    let state_author = action.author().clone();
    if state_author != mew_author {
        return Ok(ValidateCallbackResult::Invalid(
            "Only the mew author may create a MewFederationState".to_string(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

/// Validate that a RemoteFollow is created by the local_agent it references.
fn validate_remote_follow(
    action: EntryCreationAction,
    follow: RemoteFollow,
) -> ExternResult<ValidateCallbackResult> {
    let author = action.author().clone();
    if author != follow.local_agent {
        return Ok(ValidateCallbackResult::Invalid(
            "RemoteFollow local_agent must match the action author".to_string(),
        ));
    }
    Ok(ValidateCallbackResult::Valid)
}

#[allow(unused_variables)]
#[hdk_extern]
pub fn validate(op: Op) -> ExternResult<ValidateCallbackResult> {
    match op.flattened::<EntryTypes, LinkTypes>()? {
        FlatOp::StoreEntry(store_entry) => match store_entry {
            OpEntry::CreateEntry { app_entry, action } => match app_entry {
                EntryTypes::MewFederationState(state) => {
                    validate_mew_federation_state(EntryCreationAction::Create(action), state)
                }
                EntryTypes::RemoteFollow(follow) => {
                    validate_remote_follow(EntryCreationAction::Create(action), follow)
                }
                _ => Ok(ValidateCallbackResult::Valid),
            },
            OpEntry::UpdateEntry {
                app_entry, action, ..
            } => match app_entry {
                EntryTypes::MewFederationState(state) => {
                    validate_mew_federation_state(EntryCreationAction::Update(action), state)
                }
                EntryTypes::RemoteFollow(follow) => {
                    validate_remote_follow(EntryCreationAction::Update(action), follow)
                }
                _ => Ok(ValidateCallbackResult::Valid),
            },
            _ => Ok(ValidateCallbackResult::Valid),
        },
        FlatOp::RegisterUpdate(update_entry) => match update_entry {
            OpUpdate::Entry { app_entry, action } => match app_entry {
                EntryTypes::MewFederationState(state) => {
                    validate_mew_federation_state(EntryCreationAction::Update(action), state)
                }
                EntryTypes::RemoteFollow(follow) => {
                    validate_remote_follow(EntryCreationAction::Update(action), follow)
                }
                _ => Ok(ValidateCallbackResult::Valid),
            },
            _ => Ok(ValidateCallbackResult::Valid),
        },
        FlatOp::RegisterDelete(delete_entry) => Ok(ValidateCallbackResult::Valid),
        FlatOp::RegisterCreateLink {
            link_type,
            base_address,
            target_address,
            tag,
            action,
        } => Ok(ValidateCallbackResult::Valid),
        FlatOp::RegisterDeleteLink {
            link_type,
            base_address,
            target_address,
            tag,
            original_action,
            action,
        } => Ok(ValidateCallbackResult::Valid),
        FlatOp::StoreRecord(store_record) => match store_record {
            OpRecord::CreateEntry { app_entry, action } => match app_entry {
                EntryTypes::MewFederationState(state) => {
                    validate_mew_federation_state(EntryCreationAction::Create(action), state)
                }
                EntryTypes::RemoteFollow(follow) => {
                    validate_remote_follow(EntryCreationAction::Create(action), follow)
                }
                _ => Ok(ValidateCallbackResult::Valid),
            },
            OpRecord::UpdateEntry {
                app_entry, action, ..
            } => match app_entry {
                EntryTypes::MewFederationState(state) => {
                    validate_mew_federation_state(EntryCreationAction::Update(action), state)
                }
                EntryTypes::RemoteFollow(follow) => {
                    validate_remote_follow(EntryCreationAction::Update(action), follow)
                }
                _ => Ok(ValidateCallbackResult::Valid),
            },
            OpRecord::DeleteEntry {
                original_action_hash,
                action,
                ..
            } => Ok(ValidateCallbackResult::Valid),
            OpRecord::CreateLink {
                base_address,
                target_address,
                tag,
                link_type,
                action,
            } => Ok(ValidateCallbackResult::Valid),
            OpRecord::DeleteLink {
                original_action_hash,
                base_address,
                action,
            } => Ok(ValidateCallbackResult::Valid),
            OpRecord::CreatePrivateEntry { .. } => Ok(ValidateCallbackResult::Valid),
            OpRecord::UpdatePrivateEntry { .. } => Ok(ValidateCallbackResult::Valid),
            OpRecord::CreateCapClaim { .. } => Ok(ValidateCallbackResult::Valid),
            OpRecord::CreateCapGrant { .. } => Ok(ValidateCallbackResult::Valid),
            OpRecord::UpdateCapClaim { .. } => Ok(ValidateCallbackResult::Valid),
            OpRecord::UpdateCapGrant { .. } => Ok(ValidateCallbackResult::Valid),
            OpRecord::Dna { .. } => Ok(ValidateCallbackResult::Valid),
            OpRecord::OpenChain { .. } => Ok(ValidateCallbackResult::Valid),
            OpRecord::CloseChain { .. } => Ok(ValidateCallbackResult::Valid),
            OpRecord::InitZomesComplete { .. } => Ok(ValidateCallbackResult::Valid),
            _ => Ok(ValidateCallbackResult::Valid),
        },
        FlatOp::RegisterAgentActivity(agent_activity) => match agent_activity {
            OpActivity::CreateAgent { agent, action } => {
                let previous_action = must_get_action(action.prev_action)?;
                match previous_action.action() {
                    Action::AgentValidationPkg(AgentValidationPkg {
                        membrane_proof, ..
                    }) => validate_agent_joining(agent, membrane_proof),
                    _ => Ok(ValidateCallbackResult::Invalid(
                        "The previous action for a `CreateAgent` action must be \
                         an `AgentValidationPkg`"
                            .to_string(),
                    )),
                }
            }
            _ => Ok(ValidateCallbackResult::Valid),
        },
    }
}
