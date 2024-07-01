use crate::utils::{encode_uint64, byte_array_to_string};
use crate::proto::{RelaySession, RelayPrivateData, QualityOfServiceReport, ReportedProvider};
use sha2::{Digest, Sha256};

pub fn serialize_relay_session(request: &RelaySession) -> Vec<u8> {
    let mut serialized_request = String::new();
    let request_vec = request_to_vec(request);

    for (key, value) in request_vec {
        serialized_request.push_str(&serialize_key_value(&key, &value));
    }

    serialized_request.as_bytes().to_vec()
}

fn request_to_vec(request: &RelaySession) -> Vec<(&'static str, Value)> {
    let mut vec = Vec::new();
    vec.push(("spec_id", Value::String(request.spec_id.clone())));
    vec.push(("content_hash", Value::Bytes(request.content_hash.clone())));
    vec.push(("session_id", Value::Number(request.session_id as i64)));
    vec.push(("cu_sum", Value::Number(request.cu_sum as i64)));
    vec.push(("provider", Value::String(request.provider.clone())));
    vec.push(("relay_num", Value::Number(request.relay_num as i64)));
    if let Some(qos_report) = &request.qos_report {
        vec.push(("qos_report", Value::QoSReport(qos_report.clone())));
    }
    vec.push(("epoch", Value::Number(request.epoch)));
    if !request.unresponsive_providers.is_empty() {
        vec.push((
            "unresponsive_providers",
            Value::ReportedProviders(request.unresponsive_providers.clone()),
        ));
    }
    vec.push((
        "lava_chain_id",
        Value::String(request.lava_chain_id.clone()),
    ));
    if let Some(qos_excellence_report) = &request.qos_excellence_report {
        vec.push((
            "qos_excellence_report",
            Value::QoSReport(qos_excellence_report.clone()),
        ));
    }
    vec
}

#[derive(Clone)]
enum Value {
    String(String),
    Number(i64),
    Bytes(Vec<u8>),
    QoSReport(QualityOfServiceReport),
    ReportedProviders(Vec<ReportedProvider>),
}

fn serialize_key_value(key: &str, value: &Value) -> String {
    match value {
        Value::String(s) => {
            if s.is_empty() {
                String::new()
            } else {
                format!("{}:\"{}\" ", key, s)
            }
        }
        Value::Number(n) => {
            if *n == 0 {
                String::new()
            } else {
                format!("{}:{} ", key, n)
            }
        }
        Value::Bytes(b) => format!("{}:\"{}\" ", key, byte_array_to_string(b, false)),
        Value::QoSReport(qos) => format!("{}:<{}> ", key, serialize_qos_report(qos)),
        Value::ReportedProviders(providers) => serialize_reported_providers(key, providers),
    }
}

fn serialize_qos_report(qos: &QualityOfServiceReport) -> String {
    format!(
        "latency:\"{}\" availability:\"{}\" sync:\"{}\"",
        qos.latency, qos.availability, qos.sync
    )
}

fn serialize_reported_providers(key: &str, providers: &[ReportedProvider]) -> String {
    providers
        .iter()
        .map(|provider| {
            let inner = serialize_reported_provider(provider);
            if !inner.is_empty() {
                format!("{}:<{}> ", key, inner)
            } else {
                String::new()
            }
        })
        .collect()
}

fn serialize_reported_provider(provider: &ReportedProvider) -> String {
    format!(
        "address:\"{}\" disconnections:{} errors:{} timestamp_s:{}",
        provider.address, provider.disconnections, provider.errors, provider.timestamp_s
    )
}

pub fn generate_content_hash(data: &RelayPrivateData) -> Vec<u8> {
    let mut metadata_bytes = Vec::new();
    for metadata in &data.metadata {
        metadata_bytes.extend_from_slice(metadata.name.as_bytes());
        metadata_bytes.extend_from_slice(metadata.value.as_bytes());
    }

    let request_block_bytes = encode_uint64(data.request_block as u64);
    let seen_block_bytes = encode_uint64(data.seen_block as u64);

    let mut msg_parts = Vec::new();
    msg_parts.extend_from_slice(&metadata_bytes);
    msg_parts.extend_from_slice(data.extensions.concat().as_bytes());
    msg_parts.extend_from_slice(data.addon.as_bytes());
    msg_parts.extend_from_slice(data.api_interface.as_bytes());
    msg_parts.extend_from_slice(data.connection_type.as_bytes());
    msg_parts.extend_from_slice(data.api_url.as_bytes());
    msg_parts.extend_from_slice(&data.data);
    msg_parts.extend_from_slice(&request_block_bytes);
    msg_parts.extend_from_slice(&seen_block_bytes);
    msg_parts.extend_from_slice(&data.salt);

    let mut hasher = Sha256::new();
    hasher.update(&msg_parts);
    hasher.finalize().to_vec()
}