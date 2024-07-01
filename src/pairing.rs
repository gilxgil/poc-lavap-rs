use futures::future::join_all;
use reqwest;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex};
use tokio::time::timeout;
use tonic::transport::{Channel, Endpoint};

use crate::proto::relayer_client::RelayerClient;
use crate::proto::ProbeRequest;

const MAX_PROVIDERS_TO_TEST: usize = 10;
const BASE_URL: &str = "https://rest-public-rpc.lavanet.xyz/lavanet/lava/pairing/sdk_pairing";
const MAX_PROBE_DURATION: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SDKPairingParams {
    pub current_epoch: i64,
    pub time_left_to_next_pairing: u64,
    pub spec_last_updated_block: u64,
    pub block_of_next_pairing: u64,
    pub downtime_duration: String,
    pub epoch_duration: String,
}

#[derive(Debug, Clone)]
pub struct Provider {
    pub address: String,
    pub stake: u64,
    pub endpoints: Vec<String>,
    pub latest_block: u64,
}

#[derive(Debug, Clone)]
pub struct RankedProvider {
    pub provider: Provider,
    pub latency: Duration,
    client: Arc<Mutex<Option<RelayerClient<Channel>>>>,
}

pub struct SDKPairingState {
    pub params: SDKPairingParams,
    pub providers: Vec<Provider>,
    pub ranked_providers: Vec<RankedProvider>,
    pub last_updated: std::time::Instant,
}

impl SDKPairingState {
    pub fn new() -> Self {
        Self {
            params: SDKPairingParams::default(),
            providers: Vec::new(),
            ranked_providers: Vec::new(),
            last_updated: std::time::Instant::now(),
        }
    }
}

impl RankedProvider {
    pub async fn get_client(&self) -> Result<RelayerClient<Channel>, Box<dyn std::error::Error>> {
        let mut client_guard = self.client.lock().await;
        
        if client_guard.is_none() {
            if let Some(endpoint) = self.provider.endpoints.first() {
                let channel = tonic::transport::Channel::from_shared(endpoint.clone())
                    .unwrap()
                    .connect()
                    .await?;
                *client_guard = Some(RelayerClient::new(channel));
            } else {
                return Err("No endpoint available for the provider".into());
            }
        }
        
        Ok((*client_guard).as_ref().unwrap().clone())
    }
}

pub async fn sdk_pairing_task(
    address: String,
    chain_id: String,
    state: Arc<Mutex<SDKPairingState>>,
    mut shutdown: mpsc::Receiver<()>,
) {
    let client = reqwest::Client::new();

    loop {
        let next_pairing = get_sdk_pairing_params(Arc::clone(&state))
            .await
            .time_left_to_next_pairing;

        tokio::select! {
            _ = shutdown.recv() => {
                println!("Shutting down SDK pairing task");
                break;
            }
            _ = tokio::time::sleep(Duration::from_secs(next_pairing)) => {
                if let Err(e) = refresh_state(&client, &address, &chain_id, &state).await {
                    eprintln!("Error refreshing state: {}", e);
                }
            }
        }
    }
}

async fn refresh_state(
    client: &reqwest::Client,
    address: &str,
    chain_id: &str,
    state: &Arc<Mutex<SDKPairingState>>,
) -> Result<(), Box<dyn std::error::Error>> {
    //
    // In case of failure retry in 1 second
    let mut state_guard = state.lock().await;
    state_guard.params.time_left_to_next_pairing = 1;
    drop(state_guard);

    //
    //
    let url = format!("{}?chainID={}&client={}", BASE_URL, chain_id, address);
    let response = client.get(&url).send().await?;
    if response.status() != 200 {
        return Err(format!("Failed to fetch state: {}", response.status()).into());
    }

    //
    //
    let json = response.json::<serde_json::Value>().await?;
    if let Some(pairing) = json.get("pairing") {
        let new_params = parse_sdk_pairing_params(&json, pairing);
        let providers = parse_providers(pairing);
        let ranked_providers: Vec<RankedProvider> = probe_and_rank_providers(providers.clone()).await;

        let mut state_guard = state.lock().await;
        state_guard.params = new_params;
        state_guard.providers = providers;
        state_guard.last_updated = std::time::Instant::now();
        state_guard.ranked_providers = ranked_providers;
    } else {
        return Err("No pairing information found".into());
    }

    Ok(())
}

fn parse_sdk_pairing_params(
    json: &serde_json::Value,
    pairing: &serde_json::Value,
) -> SDKPairingParams {
    SDKPairingParams {
        current_epoch: pairing["current_epoch"]
            .as_str()
            .unwrap_or("")
            .parse::<i64>()
            .unwrap_or(0),
        time_left_to_next_pairing: pairing["time_left_to_next_pairing"]
            .as_str()
            .unwrap_or("")
            .parse::<u64>()
            .unwrap_or(0),
        spec_last_updated_block: pairing["spec_last_updated_block"]
            .as_str()
            .unwrap_or("")
            .parse::<u64>()
            .unwrap_or(0),
        block_of_next_pairing: pairing["block_of_next_pairing"]
            .as_str()
            .unwrap_or("")
            .parse::<u64>()
            .unwrap_or(0),
        downtime_duration: json["downtime_params"]["downtime_duration"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        epoch_duration: json["downtime_params"]["epoch_duration"]
            .as_str()
            .unwrap_or("")
            .to_string(),
    }
}

fn parse_providers(pairing: &serde_json::Value) -> Vec<Provider> {
    let mut providers = Vec::new();
    if let Some(providers_array) = pairing["providers"].as_array() {
        for provider in providers_array {
            if let Some(provider) = parse_provider(provider) {
                providers.push(provider);
            }
        }
    }
    providers.sort_by(|a, b| b.stake.cmp(&a.stake));
    providers.truncate(MAX_PROVIDERS_TO_TEST);
    providers
}

fn parse_provider(provider: &serde_json::Value) -> Option<Provider> {
    Some(Provider {
        address: provider["address"].as_str()?.to_string(),
        stake: provider["stake"]["amount"].as_str()?.parse().ok()?,
        endpoints: provider["endpoints"]
            .as_array()?
            .iter()
            .filter_map(|e| e["iPPORT"].as_str().map(|s| s.to_string()))
            .collect(),
        latest_block: provider["block_report"]["latest_block"]
            .as_str()?
            .parse()
            .ok()?,
    })
}

async fn probe_and_rank_providers(providers: Vec<Provider>) -> Vec<RankedProvider> {
    let mut probe_tasks = Vec::new();

    for provider in providers {
        if let Some(endpoint) = provider.endpoints.first().cloned() {
            let probe_task = tokio::spawn(async move {
                let (ranked_provider, is_successful) = probe_provider(provider, endpoint).await;
                if is_successful {
                    Some(ranked_provider)
                } else {
                    None
                }
            });
            probe_tasks.push(probe_task);
        }
    }

    let probe_results = join_all(probe_tasks).await;

    let mut ranked_providers = probe_results
        .into_iter()
        .filter_map(|result| {
            if let Ok(Some(ranked_provider)) = result {
                Some(ranked_provider)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    ranked_providers.sort_by(|a, b| a.latency.cmp(&b.latency));

    println!("Finished probing all providers");
    ranked_providers
}

async fn probe_provider(provider: Provider, mut endpoint: String) -> (RankedProvider, bool) {
    let start = Instant::now();
    endpoint = format!("https://{}", endpoint);

    let result = timeout(MAX_PROBE_DURATION, async {
        match Endpoint::from_shared(endpoint.clone()) {
            Ok(endpoint) => match endpoint.connect().await {
                Ok(channel) => {
                    let mut client = RelayerClient::new(channel);
                    let request = tonic::Request::new(ProbeRequest {
                        guid: 0,
                        spec_id: "ETH1".to_string(),
                        api_interface: "jsonrpc".to_string(),
                    });
                    let probe_result = client.probe(request).await;
                    (Some(client), probe_result)
                }
                Err(e) => {
                    println!("Connection failed: {}", e);
                    (
                        None,
                        Err(tonic::Status::unavailable(format!(
                            "Connection failed: {}",
                            e
                        ))),
                    )
                }
            },
            Err(e) => (
                None,
                Err(tonic::Status::invalid_argument(format!(
                    "Invalid endpoint: {}",
                    e
                ))),
            ),
        }
    })
    .await;

    let elapsed = start.elapsed();
    let (client, is_successful) = match result {
        Ok((Some(client), Ok(_))) => {
            println!(
                "Probe successful, latency: {:?}, endpoint: {}",
                elapsed, endpoint
            );
            (Some(client), true)
        }
        Ok((None, Ok(_))) => {
            // This case shouldn't occur in our current logic, but we'll handle it anyway
            println!(
                "Probe successful but client is None, latency: {:?}, endpoint: {}",
                elapsed, endpoint
            );
            (None, true)
        }
        Ok((client, Err(e))) => {
            println!(
                "Probe failed: {}, latency: {:?}, endpoint: {}",
                e, elapsed, endpoint
            );
            (client, false)
        }
        Err(_) => {
            println!(
                "Probe timed out after {:?}, endpoint: {}",
                elapsed, endpoint
            );
            (None, false)
        }
    };
    (
        RankedProvider {
            provider,
            latency: elapsed,
            client: Arc::new(Mutex::new(client)),
        },
        is_successful,
    )
}

pub async fn get_sdk_pairing_params(state: Arc<Mutex<SDKPairingState>>) -> SDKPairingParams {
    let state = state.lock().await;
    state.params.clone()
}

pub async fn get_ranked_providers(state: Arc<Mutex<SDKPairingState>>) -> Vec<RankedProvider> {
    let state = state.lock().await;
    state.ranked_providers.clone()
}
