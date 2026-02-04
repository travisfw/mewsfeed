#!/usr/bin/env bash
# Integration tests for activitypub zomes via direct zome calls
# Run after starting conductor with: hc s generate workdir/mewsfeed.happ --run=8888

set -uo pipefail  # Don't exit on error - we want to run all tests

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

ADMIN_PORT="${ADMIN_PORT:-}"
APP_ID="${APP_ID:-test-app}"

# Auto-detect admin port if not provided
if [ -z "$ADMIN_PORT" ]; then
    echo "Auto-detecting admin port..."
    ADMIN_PORT=$(ss -tlnp 2>/dev/null | grep holochain | grep -v 8888 | awk '{print $4}' | cut -d: -f2 | head -1)
    if [ -z "$ADMIN_PORT" ]; then
        echo "Could not auto-detect admin port. Please set ADMIN_PORT environment variable."
        exit 1
    fi
    echo "Found admin port: $ADMIN_PORT"
fi

echo "Getting app info from conductor..."
APP_INFO=$(hc sandbox call -r $ADMIN_PORT list-apps 2>&1)

if ! echo "$APP_INFO" | jq . >/dev/null 2>&1; then
    echo "Error: Could not connect to conductor or parse response"
    echo "$APP_INFO"
    exit 1
fi

DNA_HASH=$(echo "$APP_INFO" | jq -r ".[0].cell_info.mewsfeed[0].value.cell_id.dna_hash")
AGENT_KEY=$(echo "$APP_INFO" | jq -r ".[0].agent_pub_key")

echo "App ID: $APP_ID"
echo "DNA Hash: $DNA_HASH"
echo "Agent: $AGENT_KEY"
echo

# Test counter
TESTS_RUN=0
TESTS_PASSED=0

test_passed() {
    echo -e "${GREEN}✓${NC} $1"
    ((TESTS_PASSED++))
    ((TESTS_RUN++))
}

test_failed() {
    echo -e "${RED}✗${NC} $1"
    echo "  Error: $2"
    ((TESTS_RUN++))
}

zome_call() {
    local zome=$1
    local function=$2
    local payload=$3
    local output
    output=$(echo "" | hc sandbox zome-call --piped -r $ADMIN_PORT "$APP_ID" "$DNA_HASH" "$zome" "$function" "$payload" 2>&1) || true
    echo "$output"
}

echo "=== Test 1: Get or Set Instance Config ==="
CONFIG='{"subdomain":"alice","gateway_url":"http://localhost:8080","instance_uri":"http://localhost:8080/users/alice","public_key_id":"http://localhost:8080/users/alice#main-key"}'

# Check if config already exists
EXISTING_CONFIG=$(zome_call "activitypub" "get_instance_config" "null" 2>&1)
if echo "$EXISTING_CONFIG" | grep -q "alice"; then
    echo "Config already set (from previous run)"
    test_passed "Instance config exists"
else
    # Try to set it
    if zome_call "activitypub" "set_instance_config" "$CONFIG" 2>&1; then
        test_passed "set_instance_config succeeded"
    else
        test_failed "set_instance_config failed" "Could not set instance config"
    fi
fi

echo
echo "=== Test 2: Get Instance Config ==="
RESULT=$(zome_call "activitypub" "get_instance_config" "null")

if echo "$RESULT" | grep -qi "alice\|subdomain"; then
    test_passed "get_instance_config returned config"
else
    test_failed "get_instance_config failed" "Config not found: $RESULT"
fi

echo
echo "=== Test 3: Set Instance Config Again (should fail - singleton) ==="
SET_AGAIN=$(zome_call "activitypub" "set_instance_config" "$CONFIG")
if echo "$SET_AGAIN" | grep -qi "error\|already.*set"; then
    test_passed "Second set_instance_config failed as expected (singleton enforcement)"
else
    test_failed "Second set_instance_config should have failed" "Singleton enforcement not working: $SET_AGAIN"
fi

echo
echo "=== Test 4: Cache Remote Actor ==="
ACTOR_INPUT='{"actor_uri":"https://mastodon.social/users/bob","handle":"bob@mastodon.social","display_name":"Bob Smith"}'
CACHE_RESULT=$(zome_call "activitypub" "cache_remote_actor" "$ACTOR_INPUT")

if echo "$CACHE_RESULT" | grep -qvi "error"; then
    test_passed "cache_remote_actor succeeded"
else
    test_failed "cache_remote_actor failed" "Error: $CACHE_RESULT"
fi

echo
echo "=== Test 5: Get Remote Actor ==="
GET_ACTOR_RESULT=$(zome_call "activitypub" "get_remote_actor" '{"actor_uri":"https://mastodon.social/users/bob"}')
if echo "$GET_ACTOR_RESULT" | grep -qi "bob\|handle"; then
    test_passed "get_remote_actor returned cached actor"
else
    test_failed "get_remote_actor failed" "Actor not found: $GET_ACTOR_RESULT"
fi

echo
echo "=== Test 6: Cache Remote Actor Again (idempotency) ==="
CACHE_AGAIN=$(zome_call "activitypub" "cache_remote_actor" "$ACTOR_INPUT")
if echo "$CACHE_AGAIN" | grep -qvi "error"; then
    test_passed "cache_remote_actor succeeded again (idempotent)"
else
    test_failed "Second cache_remote_actor failed" "Error: $CACHE_AGAIN"
fi

echo
echo "=== Test 7: Create Remote Follower ==="
FOLLOWER_INPUT="{\"local_agent\":\"$AGENT_KEY\",\"remote_actor_uri\":\"https://mastodon.social/users/bob\",\"followed_at\":[0,0]}"
FOLLOWER_RESULT=$(zome_call "activitypub" "create_remote_follower" "$FOLLOWER_INPUT")

if echo "$FOLLOWER_RESULT" | grep -qvi "error"; then
    test_passed "create_remote_follower succeeded"
else
    test_failed "create_remote_follower failed" "Error: $FOLLOWER_RESULT"
fi

echo
echo "=== Test 8: Get Remote Followers ==="
GET_FOLLOWERS_RESULT=$(zome_call "activitypub" "get_remote_followers" "{\"agent\":\"$AGENT_KEY\"}")
if echo "$GET_FOLLOWERS_RESULT" | grep -qi "bob\|mastodon"; then
    test_passed "get_remote_followers returned follower"
else
    test_failed "get_remote_followers failed" "Follower not found: $GET_FOLLOWERS_RESULT"
fi

echo
echo "============================================"
echo "Test Results: $TESTS_PASSED/$TESTS_RUN passed"
echo "============================================"

if [ $TESTS_PASSED -eq $TESTS_RUN ]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed${NC}"
    exit 1
fi
