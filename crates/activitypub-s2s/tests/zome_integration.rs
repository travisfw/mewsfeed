//! Integration tests for ZomeDataSource with a live Holochain conductor.
//!
//! These tests use sweettest to spin up a test conductor, install the mewsfeed
//! DNA with activitypub zomes, and verify ZomeDataSource can communicate with them.
//!
//! Run with:
//!   cargo test --features zome --test zome_integration

#![cfg(feature = "zome")]

use std::path::PathBuf;

use holochain::sweettest::{SweetConductor, SweetDnaFile};

#[tokio::test(flavor = "multi_thread")]
async fn test_conductor_setup() {
    // Basic smoke test: can we start a conductor and install the DNA?

    let mut conductor = SweetConductor::from_standard_config().await;

    // Path to the DNA bundle (built by npm run build:happ)
    let dna_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../dnas/mewsfeed/workdir/mewsfeed.dna");

    assert!(
        dna_path.exists(),
        "DNA not found at {:?}. Run `npm run build:happ` first.",
        dna_path
    );

    let dna = SweetDnaFile::from_bundle(&dna_path)
        .await
        .expect("Failed to load DNA bundle");

    // Verify activitypub zomes are present in the DNA definition
    let mut zomes: Vec<_> = dna
        .dna_def()
        .integrity_zomes
        .iter()
        .map(|(name, _)| name.to_string())
        .collect();
    zomes.extend(
        dna.dna_def()
            .coordinator_zomes
            .iter()
            .map(|(name, _)| name.to_string()),
    );

    assert!(zomes.contains(&"activitypub_integrity".to_string()), "activitypub_integrity zome not found");
    assert!(zomes.contains(&"activitypub".to_string()), "activitypub zome not found");

    let _app = conductor
        .setup_app("test-app", &[dna])
        .await
        .expect("Failed to install app");

    println!("✅ Conductor started and mewsfeed DNA installed successfully");
    println!("   Available zomes: {:?}", zomes);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_set_and_get_instance_config() {
    // Test the activitypub zome's instance config functions

    let mut conductor = SweetConductor::from_standard_config().await;

    let dna_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../dnas/mewsfeed/workdir/mewsfeed.dna");

    let dna = SweetDnaFile::from_bundle(&dna_path).await.unwrap();
    let app = conductor.setup_app("test-app", &[dna]).await.unwrap();
    let cell = &app.cells()[0];

    // Call set_instance_config
    let config = serde_json::json!({
        "subdomain": "alice",
        "gateway_url": "http://localhost:8080",
        "instance_uri": "http://localhost:8080/users/alice",
        "public_key_id": "http://localhost:8080/users/alice#main-key"
    });

    let _: () = conductor
        .call(
            &cell.zome("activitypub"),
            "set_instance_config",
            config.clone(),
        )
        .await;

    // Call get_instance_config
    let retrieved: Option<serde_json::Value> = conductor
        .call(&cell.zome("activitypub"), "get_instance_config", ())
        .await;

    assert!(retrieved.is_some(), "get_instance_config returned None");
    let retrieved = retrieved.unwrap();

    assert_eq!(
        retrieved.get("subdomain").and_then(|v| v.as_str()),
        Some("alice")
    );
    assert_eq!(
        retrieved.get("gateway_url").and_then(|v| v.as_str()),
        Some("http://localhost:8080")
    );

    println!("✅ Instance config set and retrieved successfully");

    // Test singleton behavior: second set should fail
    let result: Result<(), _> = conductor
        .call_fallible(
            &cell.zome("activitypub"),
            "set_instance_config",
            config,
        )
        .await;

    assert!(result.is_err(), "Second set_instance_config should fail (singleton)");
    println!("✅ Singleton enforcement working (second set failed as expected)");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_remote_follower_crud() {
    // Test create/get/delete remote follower

    let mut conductor = SweetConductor::from_standard_config().await;

    let dna_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../dnas/mewsfeed/workdir/mewsfeed.dna");

    let dna = SweetDnaFile::from_bundle(&dna_path).await.unwrap();
    let app = conductor.setup_app("test-app", &[dna]).await.unwrap();
    let cell = &app.cells()[0];
    let agent = cell.agent_pubkey().clone();

    // Create remote follower
    let follower_input = serde_json::json!({
        "local_agent": agent,
        "remote_actor_uri": "https://mastodon.social/users/bob",
        "followed_at": [0, 0] // Timestamp placeholder
    });

    let action_hash_1: serde_json::Value = conductor
        .call(
            &cell.zome("activitypub"),
            "create_remote_follower",
            follower_input.clone(),
        )
        .await;

    println!("Created remote follower: {:?}", action_hash_1);

    // Test idempotency: create again with same params
    let action_hash_2: serde_json::Value = conductor
        .call(
            &cell.zome("activitypub"),
            "create_remote_follower",
            follower_input,
        )
        .await;

    assert_eq!(
        action_hash_1, action_hash_2,
        "Idempotent create should return same action hash"
    );
    println!("✅ Idempotency working (same action hash returned)");

    // Get remote followers
    let followers: Vec<serde_json::Value> = conductor
        .call(
            &cell.zome("activitypub"),
            "get_remote_followers",
            serde_json::json!({"agent": agent}),
        )
        .await;

    assert_eq!(followers.len(), 1, "Should have 1 follower");
    assert_eq!(
        followers[0].get("remote_actor_uri").and_then(|v| v.as_str()),
        Some("https://mastodon.social/users/bob")
    );
    println!("✅ get_remote_followers returned correct follower");

    // Delete remote follower
    let delete_input = serde_json::json!({
        "agent": agent,
        "remote_actor_uri": "https://mastodon.social/users/bob"
    });

    let _: () = conductor
        .call(
            &cell.zome("activitypub"),
            "delete_remote_follower",
            delete_input,
        )
        .await;

    // Verify deleted
    let followers_after: Vec<serde_json::Value> = conductor
        .call(
            &cell.zome("activitypub"),
            "get_remote_followers",
            serde_json::json!({"agent": agent}),
        )
        .await;

    assert_eq!(followers_after.len(), 0, "Follower should be deleted");
    println!("✅ Remote follower deleted successfully");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "Requires ZomeDataSource implementation - run manually"]
async fn test_zome_data_source_integration() {
    // Full integration test: ZomeDataSource → activitypub zome
    // This test is ignored by default because it requires additional setup

    let mut conductor = SweetConductor::from_standard_config().await;

    let dna_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../dnas/mewsfeed/workdir/mewsfeed.dna");

    let dna = SweetDnaFile::from_bundle(&dna_path).await.unwrap();
    let _app = conductor.setup_app("test-app", &[dna]).await.unwrap();

    // Get app port from conductor
    let _app_port = conductor.get_arbitrary_admin_websocket_port().unwrap();

    // Connect via AppWebsocket
    // NOTE: This requires proper authentication setup
    // For now, this test is marked as #[ignore]

    todo!("Complete ZomeDataSource integration once conductor app auth is set up");
}
