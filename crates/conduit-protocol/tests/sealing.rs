//! The tests of the sealing.
//!
//! The last one is the vector the extension is checked against: the same
//! keys, the same nonce prefix, the same sequence number, the same
//! ciphertext, in the browser.

use conduit_protocol::sealed::*;

fn secret() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = (index + 1) as u8;
    }
    bytes
}

fn other_secret() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = (index + 0x21) as u8;
    }
    bytes
}

fn prefix() -> [u8; 4] {
    [0xde, 0xad, 0xbe, 0xef]
}

#[test]
fn a_sealed_payload_opens_with_the_shared_secret() {
    let one = KeyPair::generate();
    let two = KeyPair::generate();

    let first = one.agree(&two.public_bytes());
    let second = two.agree(&one.public_bytes());
    assert_eq!(first.to_extension(), second.to_extension());
    assert_eq!(first.to_client(), second.to_client());

    let key = first.to_extension();
    let sealed = seal(&key, prefix(), 41, b"the message");
    let opened = open(&key, prefix(), 41, &sealed).expect("the payload opens");
    assert_eq!(opened, b"the message");
}

#[test]
fn the_two_directions_do_not_share_a_key() {
    let one = KeyPair::generate();
    let two = KeyPair::generate();
    let shared = one.agree(&two.public_bytes());
    assert_ne!(shared.to_extension(), shared.to_client());
}

#[test]
fn a_payload_does_not_open_with_the_wrong_key() {
    let one = KeyPair::generate();
    let two = KeyPair::generate();
    let three = KeyPair::generate();

    let sealed = seal(
        &one.agree(&two.public_bytes()).to_extension(),
        prefix(),
        1,
        b"secret",
    );
    let wrong = three.agree(&two.public_bytes()).to_extension();
    assert!(open(&wrong, prefix(), 1, &sealed).is_none());
}

#[test]
fn a_payload_does_not_open_under_another_sequence_number() {
    let one = KeyPair::generate();
    let two = KeyPair::generate();
    let key = one.agree(&two.public_bytes()).to_extension();
    let sealed = seal(&key, prefix(), 7, b"secret");
    assert!(
        open(&key, prefix(), 8, &sealed).is_none(),
        "the sequence number is part of the nonce, so the order is authenticated"
    );
}

#[test]
fn a_payload_does_not_open_under_another_nonce_prefix() {
    let one = KeyPair::generate();
    let two = KeyPair::generate();
    let key = one.agree(&two.public_bytes()).to_extension();
    let sealed = seal(&key, prefix(), 7, b"secret");
    assert!(open(&key, [0u8; 4], 7, &sealed).is_none());
}

#[test]
fn the_sealed_blob_survives_base64url() {
    let one = KeyPair::generate();
    let two = KeyPair::generate();
    let key = one.agree(&two.public_bytes()).to_extension();
    let sealed = seal(&key, prefix(), 3, b"round trip through the text");
    let text = sealed.to_base64();
    assert!(!text.contains('='), "the padding is not carried");
    assert!(
        !text.contains('+') && !text.contains('/'),
        "the alphabet is base64url"
    );
    let back = Sealed::from_base64(&text).expect("the text decodes");
    assert_eq!(back, sealed);
    let opened = open(&key, prefix(), 3, &back).expect("the payload opens");
    assert_eq!(opened, b"round trip through the text");
}

#[test]
fn the_key_pair_never_shows_its_secret() {
    let pair = KeyPair::generate();
    let text = format!("{pair:?}");
    assert!(!text.contains(&public_key_hex(&pair.secret_bytes())));
    assert!(text.contains(&pair.public_hex()));
}

/// The vector the extension is checked against.
///
/// The keys are fixed, so the ciphertext is fixed: the browser must seal
/// the same payload to the same bytes.
#[test]
fn the_vector_is_what_it_is() {
    let one = KeyPair::from_bytes(secret());
    let two = KeyPair::from_bytes(other_secret());

    let shared = one.agree(&two.public_bytes());
    let sealed = seal(&shared.to_extension(), prefix(), 1, b"conduit");

    println!("ONE_PUBLIC={}", one.public_hex());
    println!("TWO_PUBLIC={}", two.public_hex());
    println!("SEALED={}", sealed.to_base64());
}
