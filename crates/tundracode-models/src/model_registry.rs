use std::collections::HashMap;
use std::sync::LazyLock;

static MODEL_REGISTRY: LazyLock<HashMap<&str, u32>> = LazyLock::new(|| {
    let mut m = HashMap::new();

    // OpenAI
    m.insert("gpt-4o", 128_000);
    m.insert("gpt-4o-mini", 128_000);
    m.insert("gpt-4o-2024-05-13", 128_000);
    m.insert("gpt-4.1", 1_047_576);
    m.insert("gpt-4.1-mini", 200_000);
    m.insert("gpt-4.1-nano", 1_047_576);
    m.insert("gpt-4.5-preview", 128_000);
    m.insert("o1", 200_000);
    m.insert("o1-mini", 128_000);
    m.insert("o1-pro", 200_000);
    m.insert("o3", 200_000);
    m.insert("o3-mini", 200_000);
    m.insert("o4-mini", 128_000);

    // OpenCode Free
    m.insert("big-pickle", 128_000);
    m.insert("deepseek-v4-flash-free", 128_000);
    m.insert("mimo-v2.5-free", 128_000);
    m.insert("nemotron-3-ultra-free", 128_000);

    m
});

pub fn lookup_model_context(provider_id: &str, model_id: &str) -> Option<u32> {
    // Try exact match first
    if let Some(&ctx) = MODEL_REGISTRY.get(model_id) {
        return Some(ctx);
    }

    // Try matching by prefix (e.g., "gpt-4o-2024-08-06" -> "gpt-4o")
    for (&known_id, &ctx) in MODEL_REGISTRY.iter() {
        if model_id.starts_with(known_id) {
            return Some(ctx);
        }
    }

    // For opencode-free providers, use a default
    if provider_id.starts_with("opencode") {
        return Some(128_000);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        assert_eq!(lookup_model_context("openai", "gpt-4o"), Some(128_000));
        assert_eq!(lookup_model_context("openai", "o3-mini"), Some(200_000));
    }

    #[test]
    fn test_prefix_match() {
        assert_eq!(lookup_model_context("openai", "gpt-4o-2024-08-06"), Some(128_000));
        assert_eq!(lookup_model_context("opencode-free", "big-pickle"), Some(128_000));
    }

    #[test]
    fn test_opencode_default() {
        assert_eq!(lookup_model_context("opencode-zen", "unknown-model"), Some(128_000));
    }

    #[test]
    fn test_unknown() {
        assert_eq!(lookup_model_context("openai", "completely-unknown"), None);
    }
}
