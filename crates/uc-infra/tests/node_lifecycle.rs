#![cfg(not(feature = "test-util"))]

// This test exercises production endpoint lifecycle and multi-node behavior.

use std::collections::HashMap;
use std::net::{Ipv4Addr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use uc_core::ports::{LocalIdentityPort, SecureStorageError, SecureStoragePort};
use uc_infra::network::iroh::{IrohIdentityStore, IrohNodeBuilder, IrohNodeConfig};
use uc_infra::security::Sha256IdentityFingerprintFactory;

#[derive(Default)]
struct InMemorySecureStorage {
    values: Mutex<HashMap<String, Vec<u8>>>,
}

impl SecureStoragePort for InMemorySecureStorage {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, SecureStorageError> {
        Ok(self.values.lock().unwrap().get(key).cloned())
    }

    fn set(&self, key: &str, value: &[u8]) -> Result<(), SecureStorageError> {
        self.values
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_vec());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), SecureStorageError> {
        self.values.lock().unwrap().remove(key);
        Ok(())
    }
}

fn identity_store() -> IrohIdentityStore {
    IrohIdentityStore::new(
        Arc::new(InMemorySecureStorage::default()),
        Arc::new(Sha256IdentityFingerprintFactory),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn production_node_restarts_ten_times_with_stable_identity_and_released_port() {
    let port_probe = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("reserve test port");
    let port = port_probe.local_addr().expect("read test port").port();
    drop(port_probe);

    let primary_identity_store = identity_store();
    let config = IrohNodeConfig {
        bind_port: Some(port),
        disable_relays: true,
        ..Default::default()
    };
    let mut expected_identity = None;

    for cycle in 0..10 {
        let builder = tokio::time::timeout(
            Duration::from_secs(30),
            IrohNodeBuilder::bind(&primary_identity_store, config.clone()),
        )
        .await
        .expect("production bind exceeded 30 seconds")
        .expect("bind production node");

        if cycle == 0 {
            let second_identity_store = identity_store();
            let second_config = IrohNodeConfig {
                bind_port: None,
                disable_relays: true,
                ..Default::default()
            };
            let duplicate = tokio::time::timeout(
                Duration::from_secs(5),
                IrohNodeBuilder::bind(&second_identity_store, second_config),
            )
            .await
            .expect("concurrent production bind exceeded 5 seconds")
            .expect("independent production node should bind while the first is live");
            tokio::time::timeout(Duration::from_secs(30), duplicate.spawn().shutdown())
                .await
                .expect("second production node shutdown exceeded 30 seconds");
        }

        let identity = primary_identity_store
            .get_current_fingerprint()
            .await
            .expect("read persistent identity")
            .expect("bind creates identity");
        if let Some(expected) = &expected_identity {
            assert_eq!(&identity, expected, "identity changed on restart {cycle}");
        } else {
            expected_identity = Some(identity);
        }

        tokio::time::timeout(Duration::from_secs(30), builder.spawn().shutdown())
            .await
            .expect("production shutdown exceeded 30 seconds");

        let released = UdpSocket::bind((Ipv4Addr::LOCALHOST, port))
            .expect("shutdown must release the fixed UDP port");
        drop(released);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn production_nodes_reject_conflicting_lan_only_policies() {
    let primary_identity_store = identity_store();
    let primary_config = IrohNodeConfig {
        disable_relays: true,
        ..Default::default()
    };
    let primary = IrohNodeBuilder::bind(&primary_identity_store, primary_config)
        .await
        .expect("bind primary production node");

    let second_identity_store = identity_store();
    let conflicting_config = IrohNodeConfig {
        disable_relays: false,
        ..Default::default()
    };
    let error = match IrohNodeBuilder::bind(&second_identity_store, conflicting_config).await {
        Ok(_) => panic!("conflicting process-wide network policy must be rejected"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        uc_infra::network::iroh::IrohNodeError::RuntimePolicyConflict {
            active_lan_only: true,
            requested_lan_only: false,
        }
    ));

    primary.spawn().shutdown().await;
}
