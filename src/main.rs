mod cli;
mod crypto;
mod pairing;
mod relay_session;
mod server;
mod session_context;
mod utils;

use crate::utils::LAVA_CHAIN_PREFIX;
use cli::{Cli, Creds};
use crypto::{public_key_to_address, signing_key_from_hex};
use server::start_server;
use session_context::ConsumerSessionContext;

use crate::pairing::{
    get_ranked_providers, get_sdk_pairing_params, sdk_pairing_task, SDKPairingState,
};

use std::sync::Arc;
use structopt::StructOpt;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

pub mod proto {
    tonic::include_proto!("lavanet.lava.pairing");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::from_args();
    let creds = Creds::from_file(&args.creds)?;
    let private_key = signing_key_from_hex(&creds.secret_key)?;
    let verifying_key = private_key.verifying_key();
    let public_key_bytes = verifying_key.to_sec1_bytes();
    let address = public_key_to_address(&public_key_bytes, LAVA_CHAIN_PREFIX)?;

    //
    // Start the SDK pairing task
    let state = Arc::new(Mutex::new(SDKPairingState::new()));
    let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
    let pairing_state = Arc::clone(&state);
    tokio::spawn(async move {
        sdk_pairing_task(address, "ETH1".to_string(), pairing_state, shutdown_rx).await;
    });

    //
    //
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        let ranked_providers = get_ranked_providers(Arc::clone(&state)).await;

        if ranked_providers.len() > 0 {
            println!("Top Ranked Providers:");
            for (i, provider) in ranked_providers.iter().enumerate() {
                println!(
                    "{}. Address: {}, Latency: {:?}",
                    i + 1,
                    provider.provider.address,
                    provider.latency,
                );
            }

            let params = get_sdk_pairing_params(Arc::clone(&state)).await;
            println!("Current SDK Pairing Params: {:?}", params);
            break;
        }
    }

    //
    // Spawn the server
    let context = Arc::new(Mutex::new(ConsumerSessionContext::new(private_key.clone(), state)));
    let server_context = context.clone();
    let task = tokio::spawn(async move {
        if let Err(e) = start_server(server_context).await {
            eprintln!("Server error: {}", e);
        }
    });
    task.await?;

    // Shutdown the SDK pairing task
    shutdown_tx.send(()).await?;

    Ok(())
}
