//! The tests that hold the registry of surfaces to the truth.
//!
//! There is one declaration of every surface, in `spec/surfaces.json`:
//! its identity, its channel, the schemas it may carry and the scopes
//! it needs.  The near end is told about a surface from there, the
//! checks read it, and the pages of the site are written from it.
//!
//! These tests check that the registry is well formed, that every
//! surface in the tree is in it, and that the scopes it names are the
//! scopes the protocol knows.

use conduit_protocol::{verb, SchemaId, Scope};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

/// The root of the repository, found from the manifest of this crate.
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the root of the repository")
}

/// The registry of surfaces.
fn registry() -> Value {
    let path = root().join("spec/surfaces.json");
    let text = fs::read_to_string(&path).expect("the registry is there");
    serde_json::from_str(&text).expect("the registry is JSON")
}

/// The surfaces of the registry.
fn surfaces() -> Vec<Value> {
    registry()["surfaces"]
        .as_array()
        .expect("the surfaces are a list")
        .clone()
}

#[test]
fn every_surface_is_declared_once() {
    let surfaces = surfaces();
    assert!(!surfaces.is_empty(), "the registry has no surfaces");
    let mut names = HashSet::new();
    let mut ids = HashSet::new();
    for surface in &surfaces {
        let name = surface["name"].as_str().expect("a surface has a name");
        assert!(names.insert(name.to_string()), "the name {name} is twice");
        let id = surface["id"].as_str().expect("a surface has an id");
        assert!(ids.insert(id.to_string()), "the id {id} is twice");
    }
}

#[test]
fn every_surface_is_well_formed() {
    for surface in surfaces() {
        let name = surface["name"].as_str().expect("a surface has a name");
        // The identity is a schema id: reverse DNS with a version.
        let id = surface["id"].as_str().expect("a surface has an id");
        let (base, version) = id.rsplit_once('/').expect("a schema id has a version");
        assert!(
            base.starts_with("dev.conduit."),
            "the id {id} of {name} is not under dev.conduit"
        );
        assert!(
            version.starts_with('v'),
            "the id {id} of {name} has no version"
        );
        assert!(
            version[1..].parse::<u32>().is_ok(),
            "the version of {id} is not a number"
        );
        // The channel is there.
        assert!(
            surface["channel"].as_str().is_some_and(|c| !c.is_empty()),
            "the surface {name} has no channel"
        );
        // The schemas are schema ids.
        for schema in surface["schemas"]
            .as_array()
            .expect("the schemas are a list")
        {
            let schema = schema.as_str().expect("a schema is a string");
            assert!(
                schema.starts_with("dev.conduit.") && schema.contains("/v"),
                "the schema {schema} of {name} is not a schema id"
            );
        }
    }
}

/// The namespace of a surface: the first segment its scopes share.
fn surface_namespace(surface: &Value) -> String {
    let scopes = surface["scopes"].as_array().expect("the scopes are a list");
    let first = scopes
        .first()
        .and_then(|scope| scope.as_str())
        .expect("a scope is a string");
    let namespace = first.rsplit_once('.').expect("a scope has a verb").0;
    namespace.split('.').next().unwrap_or(namespace).to_string()
}

#[test]
fn every_scope_is_a_namespace_and_a_known_verb() {
    let verbs = [verb::READ, verb::WRITE, verb::CALL, verb::ADMIN];
    for surface in surfaces() {
        let name = surface["name"].as_str().expect("a surface has a name");
        for scope in surface["scopes"].as_array().expect("the scopes are a list") {
            let scope = scope.as_str().expect("a scope is a string");
            let (namespace, the_verb) = scope
                .rsplit_once('.')
                .unwrap_or_else(|| panic!("the scope {scope} of {name} has no verb"));
            assert!(!namespace.is_empty(), "the scope {scope} has no namespace");
            assert!(
                verbs.contains(&the_verb),
                "the scope {scope} of {name} names the verb {the_verb}, which the protocol does not know"
            );
            // The scopes of a surface share one namespace: the first
            // segment of the scope's namespace is the same for all of
            // them.  The mail surfaces share `mail`, and the shortcut
            // surface shares `shortcuts`; the name of the directory is
            // not the namespace.
            let first = namespace.split('.').next().unwrap_or(namespace);
            let namespace_of_the_surface = surface_namespace(&surface);
            assert_eq!(
                first, namespace_of_the_surface,
                "the scope {scope} of {name} is not in the namespace {namespace_of_the_surface}"
            );
        }
    }
}

#[test]
fn every_surface_in_the_tree_is_in_the_registry() {
    let surfaces = surfaces();
    let mut directories = Vec::new();
    let entries = fs::read_dir(root().join("surfaces")).expect("the surfaces are there");
    for entry in entries {
        let entry = entry.expect("a surface");
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        directories.push(name.clone());
        assert!(
            surfaces
                .iter()
                .any(|surface| surface["name"].as_str() == Some(name.as_str())),
            "the surface {name} is not in the registry"
        );
    }
    assert!(
        directories.len() >= 15,
        "the registry knows fewer surfaces than the tree has"
    );
}

#[test]
fn the_scopes_of_a_surface_are_scopes_the_protocol_knows() {
    for surface in surfaces() {
        for scope in surface["scopes"].as_array().expect("the scopes are a list") {
            let scope = scope.as_str().expect("a scope is a string");
            let (namespace, the_verb) = scope.rsplit_once('.').expect("a scope has a verb");
            let parsed = Scope::new(namespace, the_verb);
            assert_eq!(
                parsed.to_string(),
                scope,
                "the scope {scope} does not round trip"
            );
            assert!(parsed.is(the_verb), "the scope {scope} lost its verb");
        }
    }
}

#[test]
fn the_identifiers_of_the_registry_are_the_ones_the_crate_makes() {
    for surface in surfaces() {
        let id = surface["id"].as_str().expect("a surface has an id");
        let (base, version) = id.rsplit_once('/').expect("a schema id has a version");
        let parsed = SchemaId::new(
            base,
            version.trim_start_matches('v').parse().expect("a number"),
        );
        assert_eq!(parsed.to_string(), id, "the id {id} does not round trip");
    }
}
