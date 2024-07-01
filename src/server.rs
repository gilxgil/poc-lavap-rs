use axum::{
    body::Bytes, extract::State, http::StatusCode, response::IntoResponse, routing::post, Router,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::Request;
use crate::utils::{LAVA_CHAIN_ID, SPEC_ID, JSONRPC_INTERFACE};
use crate::session_context::ConsumerSessionContext;
use crate::crypto::sign_data;
use crate::proto::{RelayPrivateData, RelayRequest, RelaySession};
use crate::relay_session::{generate_content_hash, serialize_relay_session};

pub async fn start_server(
    context: Arc<Mutex<ConsumerSessionContext>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = Router::new()
        .route("/", post(handle_query))
        .with_state((context,));

    let addr = "127.0.0.1:3000";
    println!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();

    Ok(())
}

async fn handle_query(
    State((context,)): State<(Arc<Mutex<ConsumerSessionContext>>,)>,
    payload: Bytes,
) -> Result<impl IntoResponse, StatusCode> {
    //
    let (top_provider, provider_address, private_key, epoch) = {
        let mut context = context.lock().await;

        let mut epoch: i64 = 0;
        {
            let state = context.pairing_state.lock().await;
            epoch = state.params.current_epoch;
        }
        let top_provider = context.get_top_provider().await.ok_or_else(|| {
            println!("No top provider found");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        (
            top_provider.clone(),
            top_provider.provider.address.clone(),
            context.private_key.clone(),
            epoch,
        )      
    };
    println!("epoch: {:?}", epoch);

    //
    let session = {
        let mut context = context.lock().await;
        let session = context.get_or_create_session(&provider_address).clone();
        context.update_session(&provider_address);
        session
    };
    
    // Get the client from the top provider
    let mut client = top_provider.get_client().await.map_err(|e| {
        println!("Failed to get client: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    //
    let relay_data = RelayPrivateData {
        connection_type: "POST".to_string(),
        api_url: "".to_string(),
        data: payload.to_vec(),
        request_block: -1,
        api_interface: JSONRPC_INTERFACE.to_string(),
        salt: vec![],
        metadata: vec![],
        addon: "".to_string(),
        extensions: vec![],
        seen_block: 0i64,
    };
    let content_hash = generate_content_hash(&relay_data);
    let relay_session = RelaySession {
        spec_id: SPEC_ID.to_string(),
        content_hash,
        session_id: session.session_id,
        cu_sum: session.cu_sum,
        provider: provider_address, // This is already a String
        relay_num: session.relay_num,
        qos_report: None,
        epoch,
        unresponsive_providers: vec![],
        lava_chain_id: LAVA_CHAIN_ID.to_string(),
        sig: vec![],
        badge: None,
        qos_excellence_report: None,
    };
    let serialized_relay_session = serialize_relay_session(&relay_session);
    let signature = sign_data(&serialized_relay_session, &private_key).map_err(|e| {
        println!("Failed to sign data: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let relay_request = Request::new(RelayRequest {
        relay_session: Some(RelaySession {
            sig: signature,
            ..relay_session
        }),
        relay_data: Some(relay_data),
    });

    let response: tonic::Response<crate::proto::RelayReply> = client.relay(relay_request).await.map_err(|e| {
        println!("Failed to relay request: {:?}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let response_data = response.into_inner().data;
    Ok(response_data)
}