//! Content binding: RFC 8785 vectors match, property reordering preserves an
//! envelope digest, semantic mutation changes it, and duplicate keys and
//! nonconforming numbers reject before anything is hashed.

mod common;

use checkspan::contracts::{
    Attempt, Digest, EvidenceRef, GraphSpec, NodeSpec, ReceiptRef, parse_record,
};
use checkspan::digests::{
    CanonicalError, VerificationContext, artifact_digest, check_envelope_numbers, context_digest,
    digest_of, envelope_bytes, envelope_digest, envelope_digest_from_text, graph_spec_digest,
    input_manifest_digest, jcs_bytes, node_spec_digest,
};
use checkspan::validation::ParseErrorKind;
use common::fixture;
use serde_json::{Value, json};

fn hex(digest: &Digest) -> &str {
    digest.as_str().strip_prefix("sha256:").unwrap()
}

#[test]
fn rfc_8785_text_vectors_match_byte_for_byte() {
    for name in ["rfc_example", "rfc_sorting"] {
        let input: Value =
            serde_json::from_str(&fixture(&format!("canonical/{name}.input.json"))).unwrap();
        let expected = fixture(&format!("canonical/{name}.expected.json"));
        let actual = jcs_bytes(&input).unwrap();
        assert_eq!(
            String::from_utf8(actual).unwrap(),
            expected,
            "{name}: canonical bytes"
        );
    }
    // The RFC prints the example's canonical form as hex; check that too.
    let hex_from_rfc = "7b226c69746572616c73223a5b6e756c6c2c747275652c66616c73655d2c226e756d62657273223a5b3333333333333333332e333333333333332c31652b33302c342e352c302e3030322c31652d32375d2c22737472696e67223a22e282ac245c75303030665c6e4127425c225c5c5c5c5c222f227d";
    let input: Value = serde_json::from_str(&fixture("canonical/rfc_example.input.json")).unwrap();
    let actual: String = jcs_bytes(&input)
        .unwrap()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(actual, hex_from_rfc);
}

#[test]
fn independent_python_vectors_match_bytes_and_digests() {
    let file: Value = serde_json::from_str(&fixture("canonical/crosscheck.json")).unwrap();
    let cases = file["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let bytes = envelope_bytes(&case["input"]).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            case["canonical"].as_str().unwrap(),
            "{name}: canonical form"
        );
        let digest = envelope_digest(&case["input"]).unwrap();
        assert_eq!(
            hex(&digest),
            case["sha256"].as_str().unwrap(),
            "{name}: digest"
        );
    }
}

#[test]
fn sha256_known_answers() {
    assert_eq!(
        hex(&artifact_digest(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        hex(&artifact_digest(b"")),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    let digest = artifact_digest(b"abc");
    assert_eq!(
        Digest::new(digest.as_str()).unwrap(),
        digest,
        "text form round-trips"
    );
}

#[test]
fn reordering_and_formatting_preserve_the_digest_and_mutation_changes_it() {
    let compact = r#"{"a":1,"b":[1,2,{"c":"x"}],"d":null}"#;
    let reordered = "{\n  \"d\": null,\n  \"b\": [1, 2, {\"c\": \"x\"}],\n  \"a\": 1\n}";
    let base = envelope_digest_from_text(compact, 64).unwrap();
    assert_eq!(envelope_digest_from_text(reordered, 64).unwrap(), base);
    for mutated in [
        r#"{"a":2,"b":[1,2,{"c":"x"}],"d":null}"#,
        r#"{"a":1,"b":[2,1,{"c":"x"}],"d":null}"#,
        r#"{"a":1,"b":[1,2,{"c":"X"}],"d":null}"#,
        r#"{"a":1,"b":[1,2,{"c":"x"}],"d":false}"#,
        r#"{"a":1,"b":[1,2,{"c":"x"}]}"#,
        r#"{"a":"1","b":[1,2,{"c":"x"}],"d":null}"#,
    ] {
        assert_ne!(
            envelope_digest_from_text(mutated, 64).unwrap(),
            base,
            "{mutated}"
        );
    }
}

#[test]
fn graph_and_node_digests_follow_content_not_identity() {
    let spec: GraphSpec = parse_record(&fixture("contracts/valid/graph-spec-full.json")).unwrap();
    let base = graph_spec_digest(&spec).unwrap();
    let again: GraphSpec = parse_record(&serde_json::to_string(&spec).unwrap()).unwrap();
    assert_eq!(
        graph_spec_digest(&again).unwrap(),
        base,
        "serialization round trip"
    );

    let mut renamed = spec.clone();
    renamed.display_name = Some("another label".into());
    assert_ne!(
        graph_spec_digest(&renamed).unwrap(),
        base,
        "content changed"
    );
    assert_eq!(renamed.graph_ref(), spec.graph_ref(), "identity did not");

    let mut edited = spec.clone();
    edited.nodes[1].prompt.push_str(" (edited)");
    assert_ne!(graph_spec_digest(&edited).unwrap(), base);
    assert_ne!(
        node_spec_digest(&edited.nodes[1]).unwrap(),
        node_spec_digest(&spec.nodes[1]).unwrap()
    );
    assert_eq!(
        node_spec_digest(&edited.nodes[0]).unwrap(),
        node_spec_digest(&spec.nodes[0]).unwrap()
    );
    let node: NodeSpec = serde_json::from_str(&fixture("nodes/valid/check-ci.json")).unwrap();
    assert_ne!(
        node_spec_digest(&node).unwrap(),
        digest_of("checkspan.graph_spec@1", &node).unwrap(),
        "the envelope kind is part of the digest"
    );
}

#[test]
fn duplicate_keys_reject_before_hashing() {
    let err = envelope_digest_from_text(r#"{"a":1,"a":2}"#, 64).unwrap_err();
    match err {
        CanonicalError::Parse(e) => {
            assert_eq!(e.kind, ParseErrorKind::DuplicateKey);
            assert_eq!(e.path, "/a");
        }
        other => panic!("{other:?}"),
    }
    let err = envelope_digest_from_text(r#"{"outer":{"k":1,"k":1}}"#, 64).unwrap_err();
    assert!(matches!(err, CanonicalError::Parse(e) if e.path == "/outer/k"));
    assert!(matches!(
        envelope_digest_from_text("{} {}", 64),
        Err(CanonicalError::Parse(e)) if e.kind == ParseErrorKind::Syntax
    ));
}

#[test]
fn nonconforming_numbers_reject_before_hashing() {
    for (text, path) in [
        (r#"{"x":1.5}"#, "/x"),
        (r#"{"x":[0,1e30]}"#, "/x/1"),
        (r#"{"x":-0.0}"#, "/x"),
        (r#"{"x":{"y":9007199254740993}}"#, "/x/y"),
        (r#"{"x":-9007199254740993}"#, "/x"),
        (r#"{"x":18446744073709551615}"#, "/x"),
        (r#"[2, 3.0]"#, "/1"),
    ] {
        let value: Value = serde_json::from_str(text).unwrap();
        match check_envelope_numbers(&value) {
            Err(CanonicalError::UnsupportedNumber { path: p, .. }) => assert_eq!(p, path, "{text}"),
            other => panic!("{text}: {other:?}"),
        }
        assert!(envelope_digest(&value).is_err(), "{text}");
        assert!(
            jcs_bytes(&value).is_ok(),
            "{text}: raw JCS still canonicalizes floats"
        );
    }
    for text in [
        r#"{"x":9007199254740992}"#,
        r#"{"x":-9007199254740992}"#,
        r#"{"x":0}"#,
        r#"{"x":-1}"#,
    ] {
        let value: Value = serde_json::from_str(text).unwrap();
        assert!(envelope_digest(&value).is_ok(), "{text}");
    }
}

#[test]
fn input_manifest_digest_is_independent_of_assembly_order() {
    let attempt: Attempt =
        parse_record(&fixture("outcomes/valid/attempt-retry-with-hint.json")).unwrap();
    let a: EvidenceRef = attempt.input_manifest[0].clone();
    let mut b: EvidenceRef = attempt.produced_evidence[0].clone();
    b.port_name = checkspan::contracts::Ident::new("zz_other").unwrap();
    let r1 = attempt.dependency_receipts[0].clone();
    let r2 = ReceiptRef {
        id: checkspan::contracts::Ident::new("rcpt-other").unwrap(),
        ..r1.clone()
    };
    let forward =
        input_manifest_digest(&[a.clone(), b.clone()], &[r1.clone(), r2.clone()]).unwrap();
    let reversed =
        input_manifest_digest(&[b.clone(), a.clone()], &[r2.clone(), r1.clone()]).unwrap();
    assert_eq!(forward, reversed);
    let mut changed = a.clone();
    changed.content_digest = artifact_digest(b"different bytes");
    assert_ne!(
        input_manifest_digest(&[changed, b.clone()], &[r1.clone(), r2.clone()]).unwrap(),
        forward
    );
    assert_ne!(input_manifest_digest(&[a, b], &[r1]).unwrap(), forward);
}

#[test]
fn verification_context_digest_binds_every_component() {
    let attempt: Attempt = parse_record(&fixture(
        "outcomes/valid/attempt-completed-with-receipt.json",
    ))
    .unwrap();
    let node: NodeSpec = serde_json::from_str(&fixture("nodes/valid/check-ci.json")).unwrap();
    let attempt_ref = attempt.attempt_ref();
    let manifest =
        input_manifest_digest(&attempt.input_manifest, &attempt.dependency_receipts).unwrap();
    let result = attempt.result.as_ref().unwrap().digest.clone();
    let context = VerificationContext {
        attempt: &attempt_ref,
        node: &node,
        input_manifest_digest: &manifest,
        dependency_receipts: &attempt.dependency_receipts,
        result_digest: &result,
        produced_evidence: &attempt.produced_evidence,
    };
    let base = context_digest(&context).unwrap();
    assert_eq!(context_digest(&context).unwrap(), base, "deterministic");

    let mut other_attempt = attempt_ref.clone();
    other_attempt.number = checkspan::contracts::AttemptNumber::new(2).unwrap();
    assert_ne!(
        context_digest(&VerificationContext {
            attempt: &other_attempt,
            ..context.clone()
        })
        .unwrap(),
        base,
        "attempt"
    );
    let mut other_node = node.clone();
    other_node.acceptance.required_checks.pop();
    assert_ne!(
        context_digest(&VerificationContext {
            node: &other_node,
            ..context.clone()
        })
        .unwrap(),
        base,
        "contract"
    );
    let other_manifest = artifact_digest(b"other manifest");
    assert_ne!(
        context_digest(&VerificationContext {
            input_manifest_digest: &other_manifest,
            ..context.clone()
        })
        .unwrap(),
        base,
        "inputs"
    );
    let other_result = artifact_digest(b"other result");
    assert_ne!(
        context_digest(&VerificationContext {
            result_digest: &other_result,
            ..context.clone()
        })
        .unwrap(),
        base,
        "result"
    );
    let produced = vec![attempt.input_manifest[0].clone()];
    assert_ne!(
        context_digest(&VerificationContext {
            produced_evidence: &produced,
            ..context.clone()
        })
        .unwrap(),
        base,
        "produced evidence"
    );
    assert_ne!(
        context_digest(&VerificationContext {
            dependency_receipts: &[],
            ..context.clone()
        })
        .unwrap(),
        base,
        "dependency receipts"
    );
    let same_body_other_kind = digest_of("checkspan.other@1", &context).unwrap();
    assert_ne!(same_body_other_kind, base, "kind");
    assert!(json!(base.as_str()).is_string());
}
