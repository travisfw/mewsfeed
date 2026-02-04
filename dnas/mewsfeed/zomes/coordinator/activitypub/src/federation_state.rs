use activitypub_integrity::LinkTypes;
use activitypub_types::MewFederationState;
use hdk::prelude::*;

#[derive(Serialize, Deserialize, Debug)]
pub struct SetApUriInput {
    pub mew_hash: ActionHash,
    pub ap_uri: String,
}

/// Create a MewFederationState entry and link it from the mew.
#[hdk_extern]
pub fn create_mew_federation_state(state: MewFederationState) -> ExternResult<ActionHash> {
    let action_hash = create_entry(activitypub_integrity::EntryTypes::MewFederationState(
        state.clone(),
    ))?;
    create_link(
        state.mew_hash.clone(),
        action_hash.clone(),
        LinkTypes::MewToFederationState,
        (),
    )?;
    if let Some(ref ap_uri) = state.ap_uri {
        let uri_anchor = Path::from(format!("ap_uri:{ap_uri}")).path_entry_hash()?;
        create_link(
            uri_anchor,
            action_hash.clone(),
            LinkTypes::ApUriToFederationState,
            (),
        )?;
    }
    Ok(action_hash)
}

/// Set the AP URI on an existing MewFederationState.
#[hdk_extern]
pub fn set_mew_ap_uri(input: SetApUriInput) -> ExternResult<()> {
    let links = get_links(
        LinkQuery::new(
            input.mew_hash.clone(),
            LinkTypes::MewToFederationState.try_into_filter()?,
        ),
        GetStrategy::Network,
    )?;
    let link = links
        .first()
        .ok_or(wasm_error!("No MewFederationState found for this mew"))?;
    let state_hash = ActionHash::try_from(link.target.clone())
        .map_err(|_| wasm_error!("MewToFederationState target is not an ActionHash"))?;
    let record = get(state_hash, GetOptions::default())?
        .ok_or(wasm_error!("MewFederationState record not found"))?;
    let mut state: MewFederationState = record
        .entry()
        .to_app_option()
        .map_err(|_e| wasm_error!("Deserialize MewFederationState"))?
        .ok_or(wasm_error!("MewFederationState entry missing"))?;
    state.ap_uri = Some(input.ap_uri.clone());
    let new_hash = update_entry(
        record.action_address().clone(),
        activitypub_integrity::EntryTypes::MewFederationState(state),
    )?;
    // Create reverse lookup link
    let uri_anchor = Path::from(format!("ap_uri:{}", input.ap_uri)).path_entry_hash()?;
    create_link(uri_anchor, new_hash, LinkTypes::ApUriToFederationState, ())?;
    Ok(())
}

/// Get the federation state for a mew.
#[hdk_extern]
pub fn get_mew_federation_state(mew_hash: ActionHash) -> ExternResult<Option<MewFederationState>> {
    let links = get_links(
        LinkQuery::new(mew_hash, LinkTypes::MewToFederationState.try_into_filter()?),
        GetStrategy::Network,
    )?;
    let link = match links.first() {
        Some(l) => l,
        None => return Ok(None),
    };
    let action_hash = ActionHash::try_from(link.target.clone())
        .map_err(|_| wasm_error!("MewToFederationState target is not an ActionHash"))?;
    let record = get(action_hash, GetOptions::default())?;
    match record {
        Some(r) => {
            let state: MewFederationState = r
                .entry()
                .to_app_option()
                .map_err(|_e| wasm_error!("Deserialize MewFederationState"))?
                .ok_or(wasm_error!("MewFederationState entry missing"))?;
            Ok(Some(state))
        }
        None => Ok(None),
    }
}

/// Look up a MewFederationState by its AP URI.
#[hdk_extern]
pub fn get_mew_by_ap_uri(ap_uri: String) -> ExternResult<Option<MewFederationState>> {
    let uri_anchor = Path::from(format!("ap_uri:{ap_uri}")).path_entry_hash()?;
    let links = get_links(
        LinkQuery::new(
            uri_anchor,
            LinkTypes::ApUriToFederationState.try_into_filter()?,
        ),
        GetStrategy::Network,
    )?;
    let link = match links.first() {
        Some(l) => l,
        None => return Ok(None),
    };
    let action_hash = ActionHash::try_from(link.target.clone())
        .map_err(|_| wasm_error!("ApUriToFederationState target is not an ActionHash"))?;
    let record = get(action_hash, GetOptions::default())?;
    match record {
        Some(r) => {
            let state: MewFederationState = r
                .entry()
                .to_app_option()
                .map_err(|_e| wasm_error!("Deserialize MewFederationState"))?
                .ok_or(wasm_error!("MewFederationState entry missing"))?;
            Ok(Some(state))
        }
        None => Ok(None),
    }
}
