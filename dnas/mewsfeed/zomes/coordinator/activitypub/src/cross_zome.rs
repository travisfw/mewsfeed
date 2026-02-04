use activitypub_integrity::LinkTypes;
use activitypub_types::{FederationVisibility, MewFederationState};
use hc_call_utils::call_local_zome;
use hdk::prelude::*;

/// Profile data combined with instance config, for the S2S module to
/// construct an APActor.
#[derive(Serialize, Deserialize, Debug)]
pub struct AgentApProfile {
    pub agent: AgentPubKey,
    pub instance_uri: String,
    pub public_key_id: String,
    pub nickname: Option<String>,
    pub fields: std::collections::BTreeMap<String, String>,
}

/// Get an agent's profile data combined with instance config.
#[hdk_extern]
pub fn get_agent_ap_profile(agent: AgentPubKey) -> ExternResult<Option<AgentApProfile>> {
    // Get instance config
    let config: Option<activitypub_types::InstanceConfig> =
        call_local_zome("activitypub", "get_instance_config", ())?;
    let config = match config {
        Some(c) => c,
        None => return Ok(None),
    };

    // Get profile from profiles zome
    let profile_result: Option<Record> =
        call_local_zome("profiles", "get_agent_profile", agent.clone())?;

    let (nickname, fields) = match profile_result {
        Some(record) => match record.entry().to_app_option::<mews_types::Profile>() {
            Ok(Some(profile)) => (Some(profile.nickname), profile.fields),
            _ => (None, std::collections::BTreeMap::new()),
        },
        None => (None, std::collections::BTreeMap::new()),
    };

    Ok(Some(AgentApProfile {
        agent,
        instance_uri: config.instance_uri,
        public_key_id: config.public_key_id,
        nickname,
        fields,
    }))
}

/// Get all federated mews for an agent (those with a MewFederationState where
/// visibility is not HolochainOnly).
#[hdk_extern]
pub fn get_federated_mews_for_agent(agent: AgentPubKey) -> ExternResult<Vec<MewFederationState>> {
    // Get agent's mew hashes via their AgentMews links (from mews zome).
    // We use a cross-zome call to get the hashes.
    let mew_hashes: Vec<ActionHash> = call_local_zome("mews", "get_agent_mew_hashes", agent)?;

    let mut federated = Vec::new();
    for mew_hash in mew_hashes {
        let links = get_links(
            LinkQuery::new(mew_hash, LinkTypes::MewToFederationState.try_into_filter()?),
            GetStrategy::Network,
        )?;
        for link in links {
            let action_hash = match ActionHash::try_from(link.target) {
                Ok(h) => h,
                Err(_) => continue,
            };
            if let Some(record) = get(action_hash, GetOptions::default())? {
                if let Some(state) = record
                    .entry()
                    .to_app_option::<MewFederationState>()
                    .map_err(|_e| wasm_error!("Deserialize MewFederationState"))?
                {
                    if state.visibility != FederationVisibility::HolochainOnly {
                        federated.push(state);
                    }
                }
            }
        }
    }
    Ok(federated)
}
