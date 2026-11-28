// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The Tailor Kit Authors

//! Embeddings against recorded answers of each provider family.

use super::testing::{Fake, config};
use super::*;
use crate::block_on;

fn with_model(mut c: AiConfig, model: &str) -> AiConfig {
    c.embedding_model = Some(model.to_owned());
    c
}

fn texts() -> Vec<String> {
    vec!["alpha".to_owned(), "beta".to_owned()]
}

const OPENAI_ANSWER: &str = r#"{"object":"list","data":[
  {"object":"embedding","index":1,"embedding":[0.5,-0.25,1.0]},
  {"object":"embedding","index":0,"embedding":[0.125,0.0,2.0]}],
  "model":"text-embedding-3-small","usage":{"prompt_tokens":4,"total_tokens":4}}"#;

#[test]
fn openai_orders_vectors_by_index() {
    let t = Fake::new(200, OPENAI_ANSWER);
    let c = with_model(
        config(AiProviderKind::Openai, "gpt-5", None),
        "text-embedding-3-small",
    );
    let p = provider(&t, &c, Some("sk-1".into()), false).unwrap();
    let v = block_on(p.embed(texts())).unwrap();
    assert_eq!(v, vec![vec![0.125, 0.0, 2.0], vec![0.5, -0.25, 1.0]]);
    assert_eq!(
        t.seen.borrow()[0].url,
        "https://api.openai.com/v1/embeddings"
    );
    assert_eq!(t.header("authorization").as_deref(), Some("Bearer sk-1"));
    let body = t.sent_json();
    assert_eq!(body["model"], "text-embedding-3-small");
    assert_eq!(body["input"], json!(["alpha", "beta"]));
}

#[test]
fn ollama_uses_the_compatible_endpoint_without_a_key() {
    let t = Fake::new(200, OPENAI_ANSWER);
    let c = with_model(
        config(
            AiProviderKind::OpenaiCompatible,
            "llama3.1",
            Some("http://localhost:11434/v1/"),
        ),
        "nomic-embed-text",
    );
    let p = provider(&t, &c, None, false).unwrap();
    assert_eq!(block_on(p.embed(texts())).unwrap().len(), 2);
    assert_eq!(
        t.seen.borrow()[0].url,
        "http://localhost:11434/v1/embeddings"
    );
    assert_eq!(t.header("authorization"), None);
    assert_eq!(t.sent_json()["model"], "nomic-embed-text");
}

#[test]
fn azure_sends_the_deployment_with_the_api_key_header() {
    let t = Fake::new(200, OPENAI_ANSWER);
    let c = with_model(
        config(
            AiProviderKind::AzureOpenai,
            "chat-deployment",
            Some("https://r.openai.azure.com/openai/v1"),
        ),
        "embed-deployment",
    );
    let p = provider(&t, &c, Some("azk".into()), false).unwrap();
    block_on(p.embed(texts())).unwrap();
    assert_eq!(
        t.seen.borrow()[0].url,
        "https://r.openai.azure.com/openai/v1/embeddings"
    );
    assert_eq!(t.header("api-key").as_deref(), Some("azk"));
    assert_eq!(t.header("authorization"), None);
    assert_eq!(t.sent_json()["model"], "embed-deployment");
}

#[test]
fn mistral_embeds_on_its_own_endpoint() {
    let t = Fake::new(
        200,
        r#"{"id":"x","object":"list","data":[{"object":"embedding","embedding":[1.0,2.0],"index":0}],"model":"mistral-embed"}"#,
    );
    let c = with_model(
        config(AiProviderKind::Mistral, "mistral-large-latest", None),
        "mistral-embed",
    );
    let p = provider(&t, &c, Some("mk".into()), false).unwrap();
    let v = block_on(p.embed(vec!["one".into()])).unwrap();
    assert_eq!(v, vec![vec![1.0, 2.0]]);
    assert_eq!(
        t.seen.borrow()[0].url,
        "https://api.mistral.ai/v1/embeddings"
    );
}

#[test]
fn gemini_batches_every_text() {
    let t = Fake::new(
        200,
        r#"{"embeddings":[{"values":[0.1,0.2]},{"values":[0.3,0.4]}]}"#,
    );
    let c = with_model(
        config(AiProviderKind::Gemini, "gemini-3-pro", None),
        "gemini-embedding-001",
    );
    let p = provider(&t, &c, Some("gk".into()), false).unwrap();
    let v = block_on(p.embed(texts())).unwrap();
    assert_eq!(v, vec![vec![0.1, 0.2], vec![0.3, 0.4]]);
    assert_eq!(
        t.seen.borrow()[0].url,
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-embedding-001:batchEmbedContents"
    );
    assert_eq!(t.header("x-goog-api-key").as_deref(), Some("gk"));
    let body = t.sent_json();
    let reqs = body["requests"].as_array().unwrap();
    assert_eq!(reqs.len(), 2);
    assert_eq!(reqs[1]["model"], "models/gemini-embedding-001");
    assert_eq!(reqs[1]["content"]["parts"][0]["text"], "beta");
}

#[test]
fn unsupported_is_the_default() {
    let t = Fake::new(200, "{}");
    let c = config(AiProviderKind::Anthropic, "claude-opus-5-5", None);
    let p = provider(&t, &c, Some("k".into()), false).unwrap();
    assert!(matches!(
        block_on(p.embed(texts())),
        Err(AiError::Unsupported(_))
    ));
    assert!(t.seen.borrow().is_empty(), "nothing is sent");
}

#[test]
fn a_profile_without_an_embedding_model_is_unsupported() {
    for kind in [AiProviderKind::Openai, AiProviderKind::Gemini] {
        let t = Fake::new(200, "{}");
        let c = config(kind, "m", None);
        let p = provider(&t, &c, Some("k".into()), false).unwrap();
        assert!(matches!(
            block_on(p.embed(texts())),
            Err(AiError::Unsupported(_))
        ));
        assert!(t.seen.borrow().is_empty());
    }
}

#[test]
fn no_texts_send_nothing() {
    let t = Fake::new(200, "{}");
    let c = with_model(config(AiProviderKind::Openai, "m", None), "e");
    let p = provider(&t, &c, Some("k".into()), false).unwrap();
    assert_eq!(
        block_on(p.embed(Vec::new())).unwrap(),
        Vec::<Vec<f32>>::new()
    );
    assert!(t.seen.borrow().is_empty());
}

#[test]
fn a_wrong_count_or_shape_is_an_unusable_answer() {
    let c = with_model(config(AiProviderKind::Openai, "m", None), "e");
    for body in [
        r#"{"data":[{"index":0,"embedding":[1.0]}]}"#,
        r#"{"data":[{"index":0,"embedding":[1.0]},{"index":0,"embedding":[2.0]}]}"#,
        r#"{"data":[{"index":0,"embedding":["x"]},{"index":1,"embedding":[2.0]}]}"#,
        r#"{"nothing":true}"#,
    ] {
        let t = Fake::new(200, body);
        let p = provider(&t, &c, Some("k".into()), false).unwrap();
        assert!(
            matches!(block_on(p.embed(texts())), Err(AiError::Answer(_))),
            "{body}"
        );
    }
}

#[test]
fn provider_errors_are_masked_with_the_key() {
    let t = Fake::new(401, r#"{"error":{"message":"bad key sk-secret-1"}}"#);
    let c = with_model(config(AiProviderKind::Openai, "m", None), "e");
    let p = provider(&t, &c, Some("sk-secret-1".into()), false).unwrap();
    assert_eq!(
        block_on(p.embed(texts())).unwrap_err(),
        AiError::Provider("401 bad key ***".into())
    );
}

#[test]
fn execute_embed_on_masks_inputs_and_errors() {
    use crate::{CancelToken, execute_embed_on};
    let secrets = vec!["tok-1234".to_owned()];
    let t = Fake::new(200, OPENAI_ANSWER);
    let c = with_model(config(AiProviderKind::Openai, "m", None), "e");
    let p = provider(&t, &c, Some("sk-1".into()), false).unwrap();
    let v = block_on(execute_embed_on(
        p.as_ref(),
        vec!["use tok-1234 here".into(), "plain".into()],
        &secrets,
        &CancelToken::default(),
    ))
    .unwrap();
    assert_eq!(v.len(), 2);
    let sent = t.sent_json().to_string();
    assert!(!sent.contains("tok-1234"), "{sent}");
    assert!(sent.contains("use *** here"), "{sent}");

    let t = Fake::new(500, r#"{"error":{"message":"echo tok-1234"}}"#);
    let p = provider(&t, &c, Some("sk-1".into()), false).unwrap();
    let e = block_on(execute_embed_on(
        p.as_ref(),
        texts(),
        &secrets,
        &CancelToken::default(),
    ))
    .unwrap_err();
    assert!(!format!("{e:?}").contains("tok-1234"));

    let token = CancelToken::default();
    token.cancel();
    assert_eq!(
        block_on(execute_embed_on(p.as_ref(), texts(), &secrets, &token)).unwrap_err(),
        AiError::Cancelled
    );
}

#[test]
fn the_embedding_model_is_optional_in_stored_settings() {
    let old: AiConfig =
        serde_json::from_str(r#"{"provider":"openai","model":"m","base_url":null}"#).unwrap();
    assert_eq!(old.embedding_model, None);
    assert!(
        !serde_json::to_string(&old)
            .unwrap()
            .contains("embedding_model")
    );
    let new = with_model(old, "e");
    let back: AiConfig = serde_json::from_str(&serde_json::to_string(&new).unwrap()).unwrap();
    assert_eq!(back.embedding_model.as_deref(), Some("e"));
}
