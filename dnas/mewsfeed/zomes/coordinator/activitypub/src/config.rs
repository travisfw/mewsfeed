use activitypub_integrity::LinkTypes;
use activitypub_types::InstanceConfig;
use hdk::prelude::*;

/// Deterministic anchor for the singleton InstanceConfig.
fn instance_config_anchor() -> ExternResult<EntryHash> {
    Path::from("federation_instance_config").path_entry_hash()
}

/// Store the instance configuration. Only one config may exist per network
/// (enforced by the anchor link pattern).
#[hdk_extern]
pub fn set_instance_config(config: InstanceConfig) -> ExternResult<ActionHash> {
    let anchor = instance_config_anchor()?;
    let existing = get_links(
        LinkQuery::new(
            anchor.clone(),
            LinkTypes::AnchorToInstanceConfig.try_into_filter()?,
        ),
        GetStrategy::Network,
    )?;
    if !existing.is_empty() {
        return Err(wasm_error!("InstanceConfig already set for this network"));
    }
    let action_hash = create_entry(activitypub_integrity::EntryTypes::InstanceConfig(config))?;
    create_link(
        anchor,
        action_hash.clone(),
        LinkTypes::AnchorToInstanceConfig,
        (),
    )?;
    Ok(action_hash)
}

/// Retrieve the singleton InstanceConfig.
#[hdk_extern]
pub fn get_instance_config(_: ()) -> ExternResult<Option<InstanceConfig>> {
    let anchor = instance_config_anchor()?;
    let links = get_links(
        LinkQuery::new(anchor, LinkTypes::AnchorToInstanceConfig.try_into_filter()?),
        GetStrategy::Network,
    )?;
    let link = match links.first() {
        Some(l) => l,
        None => return Ok(None),
    };
    let action_hash = ActionHash::try_from(link.target.clone())
        .map_err(|_| wasm_error!("AnchorToInstanceConfig target is not an ActionHash"))?;
    let record = get(action_hash, GetOptions::default())?;
    match record {
        Some(r) => {
            let config: InstanceConfig = r
                .entry()
                .to_app_option()
                .map_err(|_e| wasm_error!("Deserialize InstanceConfig"))?
                .ok_or(wasm_error!("InstanceConfig entry missing"))?;
            Ok(Some(config))
        }
        None => Ok(None),
    }
}
