//! Low-level interfaces to ed25519 functions
//!
//! # ⚠️ Warning: Hazmat
//!
//! These primitives are easy-to-misuse low-level interfaces.
//!
//! If you are an end user / non-expert in cryptography, **do not use any of these functions**.
//! Failure to use them correctly can lead to catastrophic failures including **full private key
//! recovery.**

// Permit dead code because 1) this module is only public when the `hazmat` feature is set, and 2)
// even without `hazmat` we still need this module because this is where `ExpandedSecretKey` is
// defined.
#![allow(dead_code)]

use crate::{InternalError, SignatureError};

use curve25519_dalek::Scalar;

#[cfg(feature = "zeroize")]
use zeroize::{Zeroize, ZeroizeOnDrop};

// These are used in the functions that are made public when the hazmat feature is set
use crate::{Signature, VerifyingKey};
use curve25519_dalek::digest::{generic_array::typenum::U64, Digest};

/// Contains the secret scalar and domain separator used for generating signatures.
///
/// This is used internally for signing.
///
/// In the usual Ed25519 signing algorithm, `scalar` and `hash_prefix` are defined such that
/// `scalar || hash_prefix = H(sk)` where `sk` is the signing key and `H` is SHA-512.
/// **WARNING:** Deriving the values for these fields in any other way can lead to full key
/// recovery, as documented in [`raw_sign`] and [`raw_sign_prehashed`].
///
/// Instances of this secret are automatically overwritten with zeroes when they fall out of scope.
pub struct ExpandedSecretKey {
    /// The secret scalar used for signing
    pub scalar: Scalar,
    /// The domain separator used when hashing the message to generate the pseudorandom `r` value
    pub hash_prefix: [u8; 32],
}

#[cfg(feature = "zeroize")]
impl Drop for ExpandedSecretKey {
    fn drop(&mut self) {
        self.scalar.zeroize();
        self.hash_prefix.zeroize()
    }
}

#[cfg(feature = "zeroize")]
impl ZeroizeOnDrop for ExpandedSecretKey {}

// Some conversion methods for `ExpandedSecretKey`. The signing methods are defined in
// `signing.rs`, since we need them even when `not(feature = "hazmat")`
impl ExpandedSecretKey {
    /// Convert this `ExpandedSecretKey` into an array of 64 bytes.
    pub fn to_bytes(&self) -> [u8; 64] {
        let mut bytes: [u8; 64] = [0u8; 64];

        bytes[..32].copy_from_slice(self.scalar.as_bytes());
        bytes[32..].copy_from_slice(&self.hash_prefix[..]);
        bytes
    }

    /// Construct an `ExpandedSecretKey` from an array of 64 bytes.
    pub fn from_bytes(bytes: &[u8; 64]) -> Self {
        // TODO: Use bytes.split_array_ref once it’s in MSRV.
        let mut lower: [u8; 32] = [0u8; 32];
        let mut upper: [u8; 32] = [0u8; 32];

        lower.copy_from_slice(&bytes[00..32]);
        upper.copy_from_slice(&bytes[32..64]);

        ExpandedSecretKey {
            scalar: Scalar::from_bytes_mod_order(lower),
            hash_prefix: upper,
        }
    }

    /// Construct an `ExpandedSecretKey` from a slice of 64 bytes.
    ///
    /// # Returns
    ///
    /// A `Result` whose okay value is an EdDSA `ExpandedSecretKey` or whose error value is an
    /// `SignatureError` describing the error that occurred, namely that the given slice's length
    /// is not 64.
    #[allow(clippy::unwrap_used)]
    pub fn from_slice(bytes: &[u8]) -> Result<Self, SignatureError> {
        if bytes.len() != 64 {
            Err(InternalError::BytesLength {
                name: "ExpandedSecretKey",
                length: 64,
            }
            .into())
        } else {
            // If the input is 64 bytes long, coerce it to a 64-byte array
            Ok(Self::from_bytes(bytes.try_into().unwrap()))
        }
    }
}

impl TryFrom<&[u8]> for ExpandedSecretKey {
    type Error = SignatureError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        Self::from_slice(bytes)
    }
}

/// Compute an ordinary Ed25519 signature over the given message. `CtxDigest` is the digest used to
/// calculate the pseudorandomness needed for signing. According to the Ed25519 spec, `CtxDigest =
/// Sha512`.
///
/// # ⚠️  Unsafe
///
/// Do NOT use this function unless you absolutely must. Using the wrong values in
/// `ExpandedSecretKey` can leak your signing key. See
/// [here](https://github.com/MystenLabs/ed25519-unsafe-libs) for more details on this attack.
pub fn raw_sign<CtxDigest>(
    esk: &ExpandedSecretKey,
    message: &[u8],
    verifying_key: &VerifyingKey,
) -> Signature
where
    CtxDigest: Digest<OutputSize = U64>,
{
    esk.raw_sign::<CtxDigest>(message, verifying_key)
}

/// Compute a signature over the given prehashed message, the Ed25519ph algorithm defined in
/// [RFC8032 §5.1][rfc8032]. `MsgDigest` is the digest function used to hash the signed message.
/// `CtxDigest` is the digest function used to calculate the pseudorandomness needed for signing.
/// According to the Ed25519 spec, `MsgDigest = CtxDigest = Sha512`.
///
/// # ⚠️  Unsafe
//
/// Do NOT use this function unless you absolutely must. Using the wrong values in
/// `ExpandedSecretKey` can leak your signing key. See
/// [here](https://github.com/MystenLabs/ed25519-unsafe-libs) for more details on this attack.
///
/// # Inputs
///
/// * `esk` is the [`ExpandedSecretKey`] being used for signing
/// * `prehashed_message` is an instantiated hash digest with 512-bits of
///   output which has had the message to be signed previously fed into its
///   state.
/// * `verifying_key` is a [`VerifyingKey`] which corresponds to this secret key.
/// * `context` is an optional context string, up to 255 bytes inclusive,
///   which may be used to provide additional domain separation.  If not
///   set, this will default to an empty string.
///
/// `scalar` and `hash_prefix` are usually selected such that `scalar || hash_prefix = H(sk)` where
/// `sk` is the signing key
///
/// # Returns
///
/// A `Result` whose `Ok` value is an Ed25519ph [`Signature`] on the
/// `prehashed_message` if the context was 255 bytes or less, otherwise
/// a `SignatureError`.
///
/// [rfc8032]: https://tools.ietf.org/html/rfc8032#section-5.1
#[cfg(feature = "digest")]
#[allow(non_snake_case)]
pub fn raw_sign_prehashed<'a, CtxDigest, MsgDigest>(
    esk: &ExpandedSecretKey,
    prehashed_message: MsgDigest,
    verifying_key: &VerifyingKey,
    context: Option<&'a [u8]>,
) -> Result<Signature, SignatureError>
where
    MsgDigest: Digest<OutputSize = U64>,
    CtxDigest: Digest<OutputSize = U64>,
{
    esk.raw_sign_prehashed::<CtxDigest, MsgDigest>(prehashed_message, verifying_key, context)
}

/// The ordinary non-batched Ed25519 verification check, rejecting non-canonical R
/// values.`CtxDigest` is the digest used to calculate the pseudorandomness needed for signing.
/// According to the Ed25519 spec, `CtxDigest = Sha512`.
pub fn raw_verify<CtxDigest>(
    vk: &VerifyingKey,
    message: &[u8],
    signature: &ed25519::Signature,
) -> Result<(), SignatureError>
where
    CtxDigest: Digest<OutputSize = U64>,
{
    vk.raw_verify::<CtxDigest>(message, signature)
}

/// The batched Ed25519 verification check, rejecting non-canonical R values. `MsgDigest` is the
/// digest used to hash the signed message. `CtxDigest` is the digest used to calculate the
/// pseudorandomness needed for signing. According to the Ed25519 spec, `MsgDigest = CtxDigest =
/// Sha512`.
#[cfg(feature = "digest")]
#[allow(non_snake_case)]
pub fn raw_verify_prehashed<CtxDigest, MsgDigest>(
    vk: &VerifyingKey,
    prehashed_message: MsgDigest,
    context: Option<&[u8]>,
    signature: &ed25519::Signature,
) -> Result<(), SignatureError>
where
    MsgDigest: Digest<OutputSize = U64>,
    CtxDigest: Digest<OutputSize = U64>,
{
    vk.raw_verify_prehashed::<CtxDigest, MsgDigest>(prehashed_message, context, signature)
}

#[cfg(test)]
mod test {
    use super::*;

    use curve25519_dalek::Scalar;
    use rand::{rngs::OsRng, CryptoRng, RngCore};

    // Pick distinct, non-spec 512-bit hash functions for message and sig-context hashing
    type CtxDigest = blake2::Blake2b512;
    type MsgDigest = sha3::Sha3_512;

    impl ExpandedSecretKey {
        // Make a random expanded secret key for testing purposes. This is NOT how you generate
        // expanded secret keys IRL. They're the hash of a seed.
        fn random<R: RngCore + CryptoRng>(mut rng: R) -> Self {
            // The usual signing algorithm clamps its scalars
            let scalar_bytes = [0u8; 32];
            let scalar = Scalar::from_bits_clamped(scalar_bytes);

            let mut hash_prefix = [0u8; 32];
            rng.fill_bytes(&mut hash_prefix);

            ExpandedSecretKey {
                scalar,
                hash_prefix,
            }
        }
    }

    // Check that raw_sign and raw_verify work when a non-spec CtxDigest is used
    #[test]
    fn sign_verify_nonspec() {
        // Generate the keypair
        let mut rng = OsRng;
        let esk = ExpandedSecretKey::random(&mut rng);
        let vk = VerifyingKey::from(&esk);

        let msg = b"Then one day, a piano fell on my head";

        // Sign and verify
        let sig = raw_sign::<CtxDigest>(&esk, msg, &vk);
        raw_verify::<CtxDigest>(&vk, msg, &sig).unwrap();
    }

    // Check that raw_sign_prehashed and raw_verify_prehashed work when distinct, non-spec
    // MsgDigest and CtxDigest are used
    #[cfg(feature = "digest")]
    #[test]
    fn sign_verify_prehashed_nonspec() {
        use curve25519_dalek::digest::Digest;

        // Generate the keypair
        let mut rng = OsRng;
        let esk = ExpandedSecretKey::random(&mut rng);
        let vk = VerifyingKey::from(&esk);

        // Hash the message
        let msg = b"And then I got trampled by a herd of buffalo";
        let mut h = MsgDigest::new();
        h.update(msg);

        let ctx_str = &b"consequences"[..];

        // Sign and verify prehashed
        let sig = raw_sign_prehashed::<CtxDigest, MsgDigest>(&esk, h.clone(), &vk, Some(ctx_str))
            .unwrap();
        raw_verify_prehashed::<CtxDigest, MsgDigest>(&vk, h, Some(ctx_str), &sig).unwrap();
    }
}
