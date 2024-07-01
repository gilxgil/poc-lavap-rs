use byteorder::{ByteOrder, LittleEndian};

pub const LAVA_CHAIN_ID: &str = "lava-testnet-2";
pub const SPEC_ID: &str = "ETH1";
pub const LAVA_CHAIN_PREFIX: &str = "lava@";
pub const JSONRPC_INTERFACE: &str = "jsonrpc";

pub fn encode_uint64(value: u64) -> [u8; 8] {
    let mut buf = [0u8; 8];
    LittleEndian::write_u64(&mut buf, value);
    buf
}

pub fn byte_array_to_string(array: &[u8], replace_double_quotes: bool) -> String {
    array
        .iter()
        .map(|&byte| match byte {
            0x09 => "\\t".to_string(),
            0x0a => "\\n".to_string(),
            0x0d => "\\r".to_string(),
            0x5c => "\\\\".to_string(),
            0x22 if replace_double_quotes => "\\\"".to_string(),
            0x20..=0x7e => (byte as char).to_string(),
            _ => format!("\\{:03o}", byte),
        })
        .collect()
}