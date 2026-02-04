# ActivityPub Zome Integration Tests

These tests verify the activitypub zomes work correctly with a live Holochain conductor.

## Running the Tests

### Option 1: Bash Script (Works Now)

1. Start a conductor with the mewsfeed hApp:
   ```bash
   hc s generate workdir/mewsfeed.happ --run=8888
   ```

2. In another terminal, run the test script:
   ```bash
   ./tests/activitypub/test-activitypub-zomes.bash
   ```

The script tests:
- ✅ Instance config set/get with singleton enforcement
- ✅ Remote actor caching with idempotency
- ⏭️  Remote follower CRUD (needs agent key - see script comments)

### Option 2: Rust Integration Tests (Blocked)

The Rust sweettest integration tests in `crates/activitypub-s2s/tests/zome_integration.rs` are currently blocked by datachannel-sys build dependencies.

When sweettest builds work:
```bash
cargo test --features zome --test zome_integration
```

## Manual Testing

You can also test zome functions directly:

```bash
# Set instance config
hc zome call activitypub activitypub set_instance_config \
  '{"subdomain":"alice","gateway_url":"http://localhost:8080","instance_uri":"http://localhost:8080/users/alice","public_key_id":"http://localhost:8080/users/alice#main-key"}' \
  --port 8888

# Get instance config
hc zome call activitypub activitypub get_instance_config --port 8888

# Cache remote actor
hc zome call activitypub activitypub cache_remote_actor \
  '{"actor_uri":"https://mastodon.social/users/bob","handle":"bob@mastodon.social","display_name":"Bob Smith"}' \
  --port 8888

# Get cached actor
hc zome call activitypub activitypub get_remote_actor \
  '{"actor_uri":"https://mastodon.social/users/bob"}' \
  --port 8888
```

## Test Coverage

The activitypub zomes provide these functions:

### Config Module
- `set_instance_config` - Singleton config for the instance
- `get_instance_config` - Retrieve instance config

### Federation State Module
- `create_mew_federation_state` - Track federation state for local mews
- `get_mew_federation_state` - Get state by mew hash
- `update_mew_federation_state` - Update visibility/AP URI
- `get_mew_by_ap_uri` - Reverse lookup by ActivityPub URI

### Remote Actors Module
- `cache_remote_actor` - Idempotent actor caching
- `get_remote_actor` - Retrieve cached actor

### Remote Follows Module
- `create_remote_follow` - Idempotent outbound follow
- `delete_remote_follow` - Remove follow
- `get_remote_follows` - List agent's outbound follows
- `update_remote_follow_status` - Change follow state

### Remote Followers Module
- `create_remote_follower` - Idempotent inbound follower
- `delete_remote_follower` - Remove follower
- `get_remote_followers` - List agent's inbound followers

### Remote Interactions Module
- `create_remote_interaction` - Idempotent interaction (Like/Announce/Reply)
- `delete_remote_interaction` - Remove interaction
- `get_remote_interactions` - List mew's remote interactions

### Cross-Zome Module
- `get_agent_ap_profile` - Combines profile + instance config for ActivityPub
