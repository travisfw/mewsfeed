pub mod config;
pub mod cross_zome;
pub mod federation_state;
pub mod remote_actors;
pub mod remote_followers;
pub mod remote_follows;
pub mod remote_interactions;

use hdk::prelude::*;

#[hdk_extern]
pub fn init(_: ()) -> ExternResult<InitCallbackResult> {
    Ok(InitCallbackResult::Pass)
}
