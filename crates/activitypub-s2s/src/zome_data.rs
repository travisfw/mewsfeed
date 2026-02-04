//! `ZomeDataSource` -- implements `FederationDataSource` via Holochain zome calls.
//!
//! This module bridges the ActivityPub HTTP server to the activitypub zomes
//! running in a Holochain conductor. It connects via `AppWebsocket` and
//! maps each trait method to one or more zome extern calls.
//!
//! Mirror types are defined here so that the `activitypub-s2s` crate does not
//! depend on `hdk` (which targets WASM). The mirror types deserialize the same
//! msgpack bytes returned by zome calls.

use std::collections::{BTreeMap, HashMap};
use std::net::ToSocketAddrs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use async_trait::async_trait;
use holo_hash::{ActionHash, AgentPubKey};
use holochain_client::{
    AppAuthenticationToken, AppWebsocket, ClientAgentSigner, ZomeCallTarget,
};
use holochain_zome_types::prelude::{ExternIO, FunctionName, RoleName, Timestamp, ZomeName};
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::actor::{APActor, APPublicKey};
use crate::collections::OrderedCollection;
use crate::crypto;
use crate::error::S2SError;
use crate::mock_data::FederationDataSource;
use crate::webfinger::{WebFingerLink, WebFingerResponse};
use crate::{activity_type, APActivity, AS_CONTEXT, SECURITY_CONTEXT};

// ---------------------------------------------------------------------------
// Mirror types -- match the msgpack layout of activitypub_types / coordinator
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceConfig {
    pub subdomain: String,
    pub gateway_url: String,
    pub instance_uri: String,
    pub public_key_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MewFederationState {
    pub mew_hash: ActionHash,
    pub ap_uri: Option<String>,
    pub visibility: FederationVisibility,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FederationVisibility {
    Public,
    Unlisted,
    FollowersOnly,
    Direct(Vec<String>),
    HolochainOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteActorRef {
    pub actor_uri: String,
    pub handle: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteFollower {
    pub local_agent: AgentPubKey,
    pub remote_actor_uri: String,
    pub followed_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteInteraction {
    pub mew_hash: ActionHash,
    pub actor_uri: String,
    pub interaction_type: RemoteInteractionType,
    pub activity_uri: String,
    pub timestamp: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RemoteInteractionType {
    Like,
    Announce,
    Reply { note_uri: String },
}

/// Input for `delete_remote_interaction` zome call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteRemoteInteractionInput {
    pub mew_hash: ActionHash,
    pub activity_uri: String,
}

/// Combined profile + instance config, returned by `get_agent_ap_profile`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentApProfile {
    pub agent: AgentPubKey,
    pub instance_uri: String,
    pub public_key_id: String,
    pub nickname: Option<String>,
    pub fields: BTreeMap<String, String>,
}

// ---------------------------------------------------------------------------
// Keystore persistence
// ---------------------------------------------------------------------------

fn load_keystore(path: &Path) -> Result<HashMap<String, RsaPrivateKey>, S2SError> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let data = std::fs::read_to_string(path)
        .map_err(|e| S2SError::Internal(format!("read keystore: {e}")))?;
    let map: HashMap<String, String> = serde_json::from_str(&data)
        .map_err(|e| S2SError::Internal(format!("parse keystore: {e}")))?;
    let mut keys = HashMap::new();
    for (username, pem) in map {
        keys.insert(username, crypto::private_key_from_pem(&pem)?);
    }
    Ok(keys)
}

fn save_keystore(path: &Path, keys: &HashMap<String, RsaPrivateKey>) -> Result<(), S2SError> {
    let map: HashMap<String, String> = keys
        .iter()
        .map(|(k, v)| Ok((k.clone(), crypto::private_key_to_pem(v)?)))
        .collect::<Result<_, S2SError>>()?;
    let json = serde_json::to_string_pretty(&map)
        .map_err(|e| S2SError::Internal(format!("serialize keystore: {e}")))?;
    std::fs::write(path, json)
        .map_err(|e| S2SError::Internal(format!("write keystore: {e}")))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// ZomeDataSource
// ---------------------------------------------------------------------------

/// `FederationDataSource` backed by a live Holochain conductor.
///
/// Single-agent sidecar model: the S2S server runs alongside one Holochain
/// agent. Username resolution maps the agent's profile nickname to their
/// `AgentPubKey`. Multi-agent gateway mode is future work.
pub struct ZomeDataSource {
    client: AppWebsocket,
    base_url: String,
    role_name: RoleName,
    /// Cached username -> AgentPubKey mapping.
    agent_map: RwLock<HashMap<String, AgentPubKey>>,
    /// RSA signing keys, keyed by username. NOT stored on DHT.
    signing_keys: RwLock<HashMap<String, RsaPrivateKey>>,
    keystore_path: PathBuf,
    /// In-memory debug buffer of received activities.
    received_activities: Mutex<HashMap<String, Vec<APActivity>>>,
}

impl ZomeDataSource {
    /// Connect to a Holochain conductor and build a `ZomeDataSource`.
    ///
    /// The caller must obtain an `AppAuthenticationToken` from the admin
    /// interface first (via `AdminWebsocket::issue_app_auth_token`).
    ///
    /// Reads `InstanceConfig` from the activitypub zome to determine the
    /// domain. Loads (or creates) the RSA keystore at `keystore_path`.
    pub async fn connect(
        socket_addr: impl ToSocketAddrs,
        token: AppAuthenticationToken,
        keystore_path: PathBuf,
    ) -> Result<Self, S2SError> {
        let signer = ClientAgentSigner::default();
        let client = AppWebsocket::connect(socket_addr, token, signer.into(), None)
            .await
            .map_err(|e| S2SError::Internal(format!("conductor connect: {e}")))?;

        let role_name = RoleName::from("mewsfeed");

        // Fetch InstanceConfig to determine the domain.
        let config: Option<InstanceConfig> =
            call_zome(&client, &role_name, "activitypub", "get_instance_config", ()).await?;
        let config = config.ok_or_else(|| {
            S2SError::Internal(
                "InstanceConfig not set -- call set_instance_config first".to_string(),
            )
        })?;

        let base_url = config.instance_uri.clone();

        let signing_keys = load_keystore(&keystore_path)?;

        Ok(ZomeDataSource {
            client,
            base_url,
            role_name,
            agent_map: RwLock::new(HashMap::new()),
            signing_keys: RwLock::new(signing_keys),
            keystore_path,
            received_activities: Mutex::new(HashMap::new()),
        })
    }

    /// Resolve a username to an `AgentPubKey`.
    ///
    /// In single-agent mode, checks whether the local agent's profile nickname
    /// matches the requested username.
    async fn resolve_username(&self, username: &str) -> Result<Option<AgentPubKey>, S2SError> {
        // Check cache.
        {
            let map = self.agent_map.read().await;
            if let Some(agent) = map.get(username) {
                return Ok(Some(agent.clone()));
            }
        }

        // Check local agent's profile.
        let my_agent = self.client.my_pub_key.clone();
        let profile: Option<AgentApProfile> =
            call_zome(&self.client, &self.role_name, "activitypub", "get_agent_ap_profile", my_agent.clone())
                .await?;

        if let Some(p) = profile {
            if p.nickname.as_deref() == Some(username) {
                let mut map = self.agent_map.write().await;
                map.insert(username.to_string(), my_agent.clone());
                return Ok(Some(my_agent));
            }
        }

        Ok(None)
    }

    /// Look up a mew's `ActionHash` from its AP URI.
    async fn resolve_ap_uri_to_mew_hash(&self, ap_uri: &str) -> Result<ActionHash, S2SError> {
        let state: Option<MewFederationState> = call_zome(
            &self.client,
            &self.role_name,
            "activitypub",
            "get_mew_by_ap_uri",
            ap_uri.to_string(),
        )
        .await?;
        state
            .map(|s| s.mew_hash)
            .ok_or_else(|| S2SError::NotFound(format!("no mew for AP URI: {ap_uri}")))
    }

    /// Get or lazily generate the RSA signing key for a user, persisting to disk.
    async fn ensure_signing_key(&self, username: &str) -> Result<RsaPrivateKey, S2SError> {
        {
            let keys = self.signing_keys.read().await;
            if let Some(key) = keys.get(username) {
                return Ok(key.clone());
            }
        }

        let (private_key, _public_key) = crypto::generate_rsa_keypair()?;
        {
            let mut keys = self.signing_keys.write().await;
            keys.insert(username.to_string(), private_key.clone());
            save_keystore(&self.keystore_path, &keys)?;
        }
        Ok(private_key)
    }

    fn build_actor_url(&self, username: &str) -> String {
        format!("{}/users/{}", self.base_url, username)
    }
}

// ---------------------------------------------------------------------------
// Trait implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl FederationDataSource for ZomeDataSource {
    async fn get_actor(&self, username: &str) -> Result<Option<APActor>, S2SError> {
        let agent = match self.resolve_username(username).await? {
            Some(a) => a,
            None => return Ok(None),
        };

        let profile: Option<AgentApProfile> = call_zome(
            &self.client,
            &self.role_name,
            "activitypub",
            "get_agent_ap_profile",
            agent,
        )
        .await?;

        let profile = match profile {
            Some(p) => p,
            None => return Ok(None),
        };

        let public_key_pem = self.get_public_key_pem(username).await?.unwrap_or_default();
        let actor_url = self.build_actor_url(username);

        Ok(Some(APActor {
            context: serde_json::json!([AS_CONTEXT, SECURITY_CONTEXT]),
            id: actor_url.clone(),
            actor_type: "Person".to_string(),
            preferred_username: username.to_string(),
            name: profile.nickname.clone(),
            summary: profile
                .fields
                .get("bio")
                .map(|b| format!("<p>{}</p>", b)),
            icon: None,
            url: Some(actor_url.clone()),
            inbox: format!("{actor_url}/inbox"),
            outbox: format!("{actor_url}/outbox"),
            followers: Some(format!("{actor_url}/followers")),
            following: Some(format!("{actor_url}/following")),
            public_key: APPublicKey {
                id: format!("{actor_url}#main-key"),
                owner: actor_url,
                public_key_pem,
            },
            manually_approves_followers: false,
            discoverable: true,
        }))
    }

    async fn get_outbox(&self, username: &str) -> Result<Option<OrderedCollection>, S2SError> {
        let agent = match self.resolve_username(username).await? {
            Some(a) => a,
            None => return Ok(None),
        };

        let states: Vec<MewFederationState> = call_zome(
            &self.client,
            &self.role_name,
            "activitypub",
            "get_federated_mews_for_agent",
            agent,
        )
        .await?;

        let outbox_url = format!("{}/users/{}/outbox", self.base_url, username);
        Ok(Some(OrderedCollection {
            context: serde_json::json!(AS_CONTEXT),
            id: outbox_url.clone(),
            collection_type: "OrderedCollection".to_string(),
            total_items: states.len() as u64,
            first: if states.is_empty() {
                None
            } else {
                Some(format!("{outbox_url}?page=true"))
            },
            last: None,
        }))
    }

    async fn get_webfinger(
        &self,
        username: &str,
        domain: &str,
    ) -> Result<Option<WebFingerResponse>, S2SError> {
        if self.resolve_username(username).await?.is_none() {
            return Ok(None);
        }
        let actor_url = self.build_actor_url(username);
        Ok(Some(WebFingerResponse {
            subject: format!("acct:{username}@{domain}"),
            aliases: vec![actor_url.clone()],
            links: vec![
                WebFingerLink {
                    rel: "self".to_string(),
                    link_type: Some("application/activity+json".to_string()),
                    href: Some(actor_url.clone()),
                    template: None,
                    properties: Default::default(),
                },
                WebFingerLink {
                    rel: "http://webfinger.net/rel/profile-page".to_string(),
                    link_type: Some("text/html".to_string()),
                    href: Some(actor_url),
                    template: None,
                    properties: Default::default(),
                },
            ],
        }))
    }

    async fn process_inbox(&self, username: &str, activity: APActivity) -> Result<(), S2SError> {
        let agent = self
            .resolve_username(username)
            .await?
            .ok_or_else(|| S2SError::NotFound(format!("user: {username}")))?;

        // Log for debug endpoint.
        if let Ok(mut map) = self.received_activities.lock() {
            map.entry(username.to_string())
                .or_default()
                .push(activity.clone());
        }

        tracing::info!(
            username,
            activity_type = activity.activity_type,
            "inbox processing"
        );

        match activity.activity_type.as_str() {
            activity_type::FOLLOW => {
                let now = current_timestamp();
                let follower = RemoteFollower {
                    local_agent: agent,
                    remote_actor_uri: activity.actor.clone(),
                    followed_at: now,
                };
                let _: ActionHash = call_zome(
                    &self.client,
                    &self.role_name,
                    "activitypub",
                    "create_remote_follower",
                    follower,
                )
                .await?;

                // Cache the remote actor reference.
                let handle = extract_handle_from_uri(&activity.actor);
                let actor_ref = RemoteActorRef {
                    actor_uri: activity.actor.clone(),
                    handle,
                    display_name: None,
                };
                let _: ActionHash = call_zome(
                    &self.client,
                    &self.role_name,
                    "activitypub",
                    "upsert_remote_actor_ref",
                    actor_ref,
                )
                .await?;
            }

            activity_type::LIKE => {
                let object_uri = activity
                    .object
                    .as_str()
                    .ok_or_else(|| S2SError::BadRequest("Like object must be a URI".into()))?;
                let mew_hash = self.resolve_ap_uri_to_mew_hash(object_uri).await?;
                let interaction = RemoteInteraction {
                    mew_hash,
                    actor_uri: activity.actor.clone(),
                    interaction_type: RemoteInteractionType::Like,
                    activity_uri: activity.id.clone(),
                    timestamp: current_timestamp(),
                };
                let _: ActionHash = call_zome(
                    &self.client,
                    &self.role_name,
                    "activitypub",
                    "create_remote_interaction",
                    interaction,
                )
                .await?;
            }

            activity_type::ANNOUNCE => {
                let object_uri = activity
                    .object
                    .as_str()
                    .ok_or_else(|| {
                        S2SError::BadRequest("Announce object must be a URI".into())
                    })?;
                let mew_hash = self.resolve_ap_uri_to_mew_hash(object_uri).await?;
                let interaction = RemoteInteraction {
                    mew_hash,
                    actor_uri: activity.actor.clone(),
                    interaction_type: RemoteInteractionType::Announce,
                    activity_uri: activity.id.clone(),
                    timestamp: current_timestamp(),
                };
                let _: ActionHash = call_zome(
                    &self.client,
                    &self.role_name,
                    "activitypub",
                    "create_remote_interaction",
                    interaction,
                )
                .await?;
            }

            activity_type::UNDO => {
                let inner_type = activity
                    .object
                    .get("type")
                    .and_then(|t| t.as_str())
                    .unwrap_or("");

                match inner_type {
                    "Follow" => {
                        // Undo Follow: find and delete the RemoteFollower.
                        // Future: add a find_remote_follower_by_actor_uri extern.
                        tracing::warn!("Undo Follow not yet fully implemented");
                    }
                    "Like" | "Announce" => {
                        let inner_id = activity
                            .object
                            .get("id")
                            .and_then(|i| i.as_str())
                            .ok_or_else(|| {
                                S2SError::BadRequest("Undo object must have id".into())
                            })?;
                        let inner_object_uri = activity
                            .object
                            .get("object")
                            .and_then(|o| o.as_str())
                            .ok_or_else(|| {
                                S2SError::BadRequest("Undo inner object must be a URI".into())
                            })?;
                        let mew_hash =
                            self.resolve_ap_uri_to_mew_hash(inner_object_uri).await?;
                        let input = DeleteRemoteInteractionInput {
                            mew_hash,
                            activity_uri: inner_id.to_string(),
                        };
                        let _: () = call_zome(
                            &self.client,
                            &self.role_name,
                            "activitypub",
                            "delete_remote_interaction",
                            input,
                        )
                        .await?;
                    }
                    _ => {
                        tracing::warn!(inner_type, "unhandled Undo object type");
                    }
                }
            }

            activity_type::ACCEPT => {
                // Remote server accepted our Follow request.
                // Future: update the matching RemoteFollow status to Accepted.
                tracing::info!("Accept activity received -- follow status update deferred");
            }

            activity_type::CREATE => {
                // Possibly a reply to one of our mews.
                if let Some(in_reply_to) =
                    activity.object.get("inReplyTo").and_then(|v| v.as_str())
                {
                    if let Ok(mew_hash) = self.resolve_ap_uri_to_mew_hash(in_reply_to).await {
                        let note_uri = activity
                            .object
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or(&activity.id)
                            .to_string();
                        let interaction = RemoteInteraction {
                            mew_hash,
                            actor_uri: activity.actor.clone(),
                            interaction_type: RemoteInteractionType::Reply { note_uri },
                            activity_uri: activity.id.clone(),
                            timestamp: current_timestamp(),
                        };
                        let _: ActionHash = call_zome(
                            &self.client,
                            &self.role_name,
                            "activitypub",
                            "create_remote_interaction",
                            interaction,
                        )
                        .await?;
                    }
                }
            }

            _ => {
                tracing::info!(activity.activity_type, "unhandled activity type");
            }
        }

        Ok(())
    }

    async fn get_signing_key(&self, username: &str) -> Result<Option<RsaPrivateKey>, S2SError> {
        if self.resolve_username(username).await?.is_none() {
            return Ok(None);
        }
        Ok(Some(self.ensure_signing_key(username).await?))
    }

    async fn get_public_key_pem(&self, username: &str) -> Result<Option<String>, S2SError> {
        let key = self.get_signing_key(username).await?;
        match key {
            Some(private_key) => {
                let public_key = RsaPublicKey::from(&private_key);
                Ok(Some(crypto::public_key_to_pem(&public_key)?))
            }
            None => Ok(None),
        }
    }

    async fn get_received_activities(&self, username: &str) -> Result<Vec<APActivity>, S2SError> {
        let map = self
            .received_activities
            .lock()
            .map_err(|e| S2SError::Internal(format!("lock: {e}")))?;
        Ok(map.get(username).cloned().unwrap_or_default())
    }

    async fn get_private_key_pem(&self, username: &str) -> Result<Option<String>, S2SError> {
        let key = self.get_signing_key(username).await?;
        match key {
            Some(k) => Ok(Some(crypto::private_key_to_pem(&k)?)),
            None => Ok(None),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Generic zome call: serialize input, call via AppWebsocket, deserialize output.
async fn call_zome<I, O>(
    client: &AppWebsocket,
    role_name: &RoleName,
    zome_name: &str,
    fn_name: &str,
    input: I,
) -> Result<O, S2SError>
where
    I: Serialize + std::fmt::Debug,
    O: for<'de> Deserialize<'de> + std::fmt::Debug,
{
    let payload = ExternIO::encode(input)
        .map_err(|e| S2SError::Internal(format!("encode zome input: {e}")))?;

    let response = client
        .call_zome(
            ZomeCallTarget::RoleName(role_name.clone()),
            ZomeName::from(zome_name),
            FunctionName::from(fn_name),
            payload,
        )
        .await
        .map_err(|e| {
            let msg = format!("{e}");
            if msg.contains("not found") || msg.contains("missing") {
                S2SError::NotFound(msg)
            } else {
                S2SError::Internal(format!("zome call {zome_name}::{fn_name}: {msg}"))
            }
        })?;

    response
        .decode()
        .map_err(|e| S2SError::Internal(format!("decode {zome_name}::{fn_name} response: {e}")))
}

/// Extract domain from a URI like "https://cats.mewsfeed.example".
#[cfg(test)]
fn extract_domain(uri: &str) -> Result<String, S2SError> {
    let url = url::Url::parse(uri)
        .map_err(|e| S2SError::Internal(format!("parse instance_uri: {e}")))?;
    url.host_str()
        .map(|h| h.to_string())
        .ok_or_else(|| S2SError::Internal(format!("no host in instance_uri: {uri}")))
}

/// Derive a fediverse-style handle from an actor URI.
///
/// `https://mastodon.social/users/alice` -> `@alice@mastodon.social`
fn extract_handle_from_uri(uri: &str) -> String {
    if let Ok(parsed) = url::Url::parse(uri) {
        let host = parsed.host_str().unwrap_or("unknown");
        let username = parsed
            .path_segments()
            .and_then(|mut s| {
                // Skip path segments until we find "users", then take the next.
                while let Some(seg) = s.next() {
                    if seg == "users" {
                        return s.next();
                    }
                }
                None
            })
            .unwrap_or("unknown");
        format!("@{username}@{host}")
    } else {
        format!("@unknown@unknown")
    }
}

/// Current time as a Holochain `Timestamp` (microseconds since epoch).
fn current_timestamp() -> Timestamp {
    let micros = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as i64;
    Timestamp::from_micros(micros)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_domain() {
        assert_eq!(
            extract_domain("https://cats.mewsfeed.example").unwrap(),
            "cats.mewsfeed.example"
        );
        assert_eq!(
            extract_domain("http://localhost:3000").unwrap(),
            "localhost"
        );
    }

    #[test]
    fn test_extract_handle_from_uri() {
        assert_eq!(
            extract_handle_from_uri("https://mastodon.social/users/alice"),
            "@alice@mastodon.social"
        );
        assert_eq!(
            extract_handle_from_uri("https://pleroma.example/users/bob"),
            "@bob@pleroma.example"
        );
    }

    #[test]
    fn test_extract_handle_from_uri_malformed() {
        assert_eq!(
            extract_handle_from_uri("not-a-url"),
            "@unknown@unknown"
        );
    }

    #[test]
    fn test_current_timestamp() {
        let ts = current_timestamp();
        // Should be after 2024-01-01 in microseconds.
        assert!(ts.as_micros() > 1_704_067_200_000_000);
    }
}
