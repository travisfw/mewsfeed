#!/usr/bin/env bash
# Integration tests for activitypub zomes via direct zome calls
# Run after starting conductor with: hc s generate workdir/mewsfeed.happ --run=8888

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

CONDUCTOR_PORT="${CONDUCTOR_PORT:-8888}"
APP_ID="test-app"

echo "Testing activitypub zomes via conductor on port $CONDUCTOR_PORT"
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

echo "=== Test 1: Set Instance Config ==="
CONFIG='{"subdomain":"alice","gateway_url":"http://localhost:8080","instance_uri":"http://localhost:8080/users/alice","public_key_id":"http://localhost:8080/users/alice#main-key"}'

if hc zome call activitypub activitypub set_instance_config "$CONFIG" --port $CONDUCTOR_PORT 2>&1; then
    test_passed "set_instance_config succeeded"
else
    test_failed "set_instance_config failed" "Could not set instance config"
fi

echo
echo "=== Test 2: Get Instance Config ==="
RESULT=$(hc zome call activitypub activitypub get_instance_config --port $CONDUCTOR_PORT 2>&1)

if echo "$RESULT" | grep -q "alice"; then
    test_passed "get_instance_config returned config with subdomain 'alice'"
else
    test_failed "get_instance_config failed" "Config not found or incorrect"
fi

echo
echo "=== Test 3: Set Instance Config Again (should fail - singleton) ==="
if hc zome call activitypub activitypub set_instance_config "$CONFIG" --port $CONDUCTOR_PORT 2>&1 | grep -q "error\|Error"; then
    test_passed "Second set_instance_config failed as expected (singleton enforcement)"
else
    test_failed "Second set_instance_config should have failed" "Singleton enforcement not working"
fi

echo
echo "=== Test 4: Create Remote Follower ==="
# Note: You'll need to get actual AgentPubKey from conductor
# For now, using placeholder - update with real agent key
AGENT_KEY="uhCAkXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
FOLLOWER_INPUT="{\"local_agent\":\"$AGENT_KEY\",\"remote_actor_uri\":\"https://mastodon.social/users/bob\",\"followed_at\":[0,0]}"

echo "Note: Update AGENT_KEY in script with actual agent public key"
echo "Skipping create_remote_follower test (needs real agent key)"
# Uncomment when you have real agent key:
# if hc zome call activitypub activitypub create_remote_follower "$FOLLOWER_INPUT" --port $CONDUCTOR_PORT 2>&1; then
#     test_passed "create_remote_follower succeeded"
# else
#     test_failed "create_remote_follower failed" "Could not create follower"
# fi

echo
echo "=== Test 5: Cache Remote Actor ==="
ACTOR_INPUT='{"actor_uri":"https://mastodon.social/users/bob","handle":"bob@mastodon.social","display_name":"Bob Smith"}'

if hc zome call activitypub activitypub cache_remote_actor "$ACTOR_INPUT" --port $CONDUCTOR_PORT 2>&1; then
    test_passed "cache_remote_actor succeeded"
else
    test_failed "cache_remote_actor failed" "Could not cache actor"
fi

echo
echo "=== Test 6: Get Remote Actor ==="
if hc zome call activitypub activitypub get_remote_actor '{"actor_uri":"https://mastodon.social/users/bob"}' --port $CONDUCTOR_PORT 2>&1 | grep -q "bob"; then
    test_passed "get_remote_actor returned cached actor"
else
    test_failed "get_remote_actor failed" "Actor not found"
fi

echo
echo "=== Test 7: Cache Remote Actor Again (idempotency) ==="
if hc zome call activitypub activitypub cache_remote_actor "$ACTOR_INPUT" --port $CONDUCTOR_PORT 2>&1; then
    test_passed "cache_remote_actor succeeded again (idempotent)"
else
    test_failed "Second cache_remote_actor failed" "Idempotency check failed"
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
