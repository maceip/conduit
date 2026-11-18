//! The tests that hold the protocol to the schemas.
//!
//! The JSON Schemas in `spec/schemas/` are the normative form.  These
//! tests read them and check that the Rust agrees: the frame types of the
//! registry, the envelope's fields, and the shape of the identifiers.

use conduit_protocol::*;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

/// The root of the repository, found from the manifest of this crate.
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the root of the repository")
}

/// The registry of frame types.
fn registry() -> Value {
    let path = root().join("spec/frames.json");
    let text = fs::read_to_string(&path).expect("the registry is there");
    serde_json::from_str(&text).expect("the registry is JSON")
}

#[test]
fn every_registered_type_is_known_to_the_crate() {
    let registry = registry();
    let types = registry["frames"]
        .as_array()
        .expect("the frames are a list");
    assert!(!types.is_empty(), "the registry has no frames");
    for entry in types {
        let name = entry["type"].as_str().expect("a type has a name");
        assert!(
            FrameType::from_name(name).is_some(),
            "the registry names {name} and the crate does not know it"
        );
    }
    assert_eq!(
        FrameType::ALL.len(),
        types.len(),
        "the crate and the registry disagree about how many types there are"
    );
}

#[test]
fn the_registry_and_the_crate_agree_on_the_schema_of_every_type() {
    let registry = registry();
    for entry in registry["frames"]
        .as_array()
        .expect("the frames are a list")
    {
        let name = entry["type"].as_str().expect("a type has a name");
        let schema = entry["schema"].as_str().expect("a type has a schema");
        let kind = FrameType::from_name(name).expect("the crate knows the type");
        let id = SchemaId::new(
            schema
                .rsplit_once('/')
                .expect("a schema id has a version")
                .0,
            schema
                .rsplit_once('/')
                .expect("a schema id has a version")
                .1
                .trim_start_matches('v')
                .parse()
                .expect("a version is a number"),
        );
        assert_eq!(id.to_string(), schema, "the schema id does not round trip");
        assert_eq!(kind.name(), name, "the type does not round trip");
    }
}

#[test]
fn the_envelope_of_the_schema_is_the_envelope_of_the_crate() {
    let path = root().join("spec/schemas/envelope.schema.json");
    let text = fs::read_to_string(&path).expect("the envelope schema is there");
    let schema: Value = serde_json::from_str(&text).expect("the schema is JSON");
    let properties = schema["properties"]
        .as_object()
        .expect("the schema has fields");
    for field in ["t", "s", "c", "i", "seq", "p"] {
        assert!(
            properties.contains_key(field),
            "the envelope has no {field}"
        );
    }
    for field in properties.keys() {
        assert!(
            ["t", "s", "c", "i", "seq", "p"].contains(&field.as_str()),
            "the envelope has a field the crate does not know: {field}"
        );
    }
}

#[test]
fn a_frame_carries_its_fields_as_the_schema_says() {
    let frame = Frame {
        kind: FrameType::Publish,
        schema: SchemaId::new("dev.conduit.mail.message", 1),
        channel: "mail.messages".to_string(),
        call: Some(7),
        seq: 41,
        payload: Some("c2VhbGVk".to_string()),
    };
    let json = serde_json::to_value(&frame).expect("a frame is JSON");
    assert_eq!(json["t"], "publish");
    assert_eq!(json["s"], "dev.conduit.mail.message/v1");
    assert_eq!(json["c"], "mail.messages");
    assert_eq!(json["i"], 7);
    assert_eq!(json["seq"], 41);
    assert_eq!(json["p"], "c2VhbGVk");
}

#[test]
fn a_frame_without_a_call_or_a_payload_leaves_them_out() {
    let frame = Frame::new(FrameType::Ping, "tunnel", None);
    let json = serde_json::to_value(&frame).expect("a frame is JSON");
    assert!(json.get("i").is_none(), "a frame with no call says so");
    assert!(json.get("p").is_none(), "a frame with no payload says so");
}

#[test]
fn a_scope_carries_its_namespace_and_its_verb() {
    let scope = Scope::new("mail.compose", verb::WRITE);
    assert_eq!(scope.to_string(), "mail.compose.write");
    assert!(scope.is(verb::WRITE));
    assert!(!scope.is(verb::READ));
    assert_eq!(scope.namespace(), "mail.compose");
}

#[test]
fn a_grant_covers_only_what_it_was_given() {
    let grant = Grant {
        surface: SchemaId::new("dev.conduit.mail.compose", 1),
        instance: "4f9c".to_string(),
        scopes: vec![Scope::new("mail.compose", verb::READ)],
        schemes: vec!["x25519-chacha20poly1305/v1".to_string()],
    };
    assert!(grant.covers(&Scope::new("mail.compose", verb::READ)));
    assert!(!grant.covers(&Scope::new("mail.compose", verb::WRITE)));
    let missing = grant.missing(&[
        Scope::new("mail.compose", verb::READ),
        Scope::new("mail.compose", verb::WRITE),
    ]);
    assert_eq!(missing.len(), 1);
    assert_eq!(missing[0].verb(), verb::WRITE);
}

#[test]
fn the_replay_ring_keeps_the_last_frames_and_says_when_there_is_a_gap() {
    let mut replay = Replay::new();
    for seq in 0..10 {
        replay.push(seq, Frame::new(FrameType::Publish, "c", None));
    }
    let (frames, stopped, gap) = replay.replay_from(7);
    assert_eq!(frames.len(), 2);
    assert_eq!(stopped, 9);
    assert!(!gap, "there was no gap");
}

#[test]
fn the_replay_ring_says_when_the_gap_is_larger_than_the_ring() {
    let mut replay = Replay::new();
    for seq in 0..(REPLAY_RING as u64 + 10) {
        replay.push(seq, Frame::new(FrameType::Publish, "c", None));
    }
    let (_, _, gap) = replay.replay_from(0);
    assert!(
        gap,
        "the gap was larger than the ring and the end must know"
    );
}

#[test]
fn a_frame_over_the_limit_is_over_the_limit() {
    let mut frame = Frame::new(FrameType::Publish, "c", None);
    frame.payload = Some("x".repeat(MAX_FRAME_BYTES + 1));
    assert!(!frame.within_the_limit());
    frame.payload = Some("x".repeat(16));
    assert!(frame.within_the_limit());
}

#[test]
fn every_failure_carries_the_code_that_the_wire_uses() {
    use conduit_protocol::Failure;
    assert_eq!(Failure::TooLarge.code().name(), "too_large");
    assert_eq!(
        Failure::ScopeDenied(Scope::new("mail.compose", verb::WRITE))
            .code()
            .name(),
        "scope_denied"
    );
    assert_eq!(
        Failure::UnknownType("x".into()).code().name(),
        "unknown_type"
    );
    assert_eq!(Failure::Unauthorized.code().name(), "unauthorized");
}
