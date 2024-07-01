use hex::decode;
use k256::ecdsa::SigningKey;
use ripemd::Ripemd160;
use sha2::{Digest, Sha256};
use subtle_encoding::bech32;

pub fn signing_key_from_hex(hex_key: &str) -> Result<SigningKey, Box<dyn std::error::Error>> {
    let key_bytes = decode(hex_key).unwrap();
    Ok(SigningKey::from_slice(&key_bytes)?)
}

pub fn sign_data(
    data: &[u8],
    signing_key: &SigningKey,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut digest = Sha256::new();
    digest.update(data);
    let (signature, recid) = signing_key.sign_digest_recoverable(digest)?;
    let mut sig = vec![27 + recid.to_byte()];
    sig.extend(signature.to_bytes().to_vec());
    Ok(sig)
}

pub fn public_key_to_address(public_key: &Box<[u8]>, chain: &str) -> Result<String, Box<dyn std::error::Error>> {
    let sha256_hash = Sha256::digest(&public_key);
    let mut hasher = Ripemd160::new();
    hasher.update(sha256_hash);
    let ripemd160_hash = hasher.finalize();
    let address = bech32::encode(chain, &ripemd160_hash);
    Ok(address)
}
