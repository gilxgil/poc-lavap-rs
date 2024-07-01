use crate::pairing::{get_ranked_providers, RankedProvider, SDKPairingState};
use k256::ecdsa::SigningKey;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone)]
pub struct ProviderSession {
    pub session_id: u64,
    pub cu_sum: u64,
    pub relay_num: u64,
}

pub struct ConsumerSessionContext {
    sessions: HashMap<String, ProviderSession>,
    ranked_providers: Vec<RankedProvider>,
    pub private_key: SigningKey,
    pub pairing_state: Arc<Mutex<SDKPairingState>>,
}

impl ConsumerSessionContext {
    pub fn new(private_key: SigningKey, pairing_state: Arc<Mutex<SDKPairingState>>) -> Self {
        ConsumerSessionContext {
            sessions: HashMap::new(),
            ranked_providers: Vec::new(),
            private_key,
            pairing_state,
        }
    }

    pub fn get_or_create_session(&mut self, provider_address: &str) -> &mut ProviderSession {
        self.sessions
            .entry(provider_address.to_string())
            .or_insert_with(|| {
                ProviderSession {
                // FIXME: u64 sometimes encodes incorrectly with this implementation, truncate to u32 for now
                session_id: (Uuid::new_v4().as_u128() as u32) as u64,
                cu_sum: 0,
                relay_num: 1,
            }})
    }

    pub fn update_session(&mut self, provider_address: &str) {
        if let Some(session) = self.sessions.get_mut(provider_address) {
            session.cu_sum += 10;
            session.relay_num += 1;
        }
    }

    pub async fn get_top_provider(&mut self) -> Option<&RankedProvider> {
        if self.ranked_providers.is_empty() {
            let ranked_providers = get_ranked_providers(self.pairing_state.clone()).await;
            if !ranked_providers.is_empty() {
                self.ranked_providers = ranked_providers;
            } else {
                return None;
            }
        }
        self.ranked_providers.first()
    }
}
