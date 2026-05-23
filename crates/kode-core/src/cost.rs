/// Token cost tracking per model
#[derive(Debug, Clone, Default)]
pub struct CostTracker {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_usd: f64,
}

impl CostTracker {
    pub fn add(&mut self, prompt: u64, completion: u64, model: &str) {
        self.prompt_tokens += prompt;
        self.completion_tokens += completion;
        self.total_usd += estimate_cost(prompt, completion, model);
    }

    pub fn summary(&self) -> String {
        format!(
            "↑{}  ↓{}  ${:.6}",
            self.prompt_tokens, self.completion_tokens, self.total_usd
        )
    }
}

/// Rough cost estimate based on known model pricing (USD per 1M tokens)
fn estimate_cost(prompt: u64, completion: u64, model: &str) -> f64 {
    let (input_rate, output_rate) = model_rates(model);
    (prompt as f64 / 1_000_000.0) * input_rate
        + (completion as f64 / 1_000_000.0) * output_rate
}

fn model_rates(model: &str) -> (f64, f64) {
    let m = model.to_lowercase();
    if m.contains("gpt-4o") { return (5.0, 15.0); }
    if m.contains("gpt-4") { return (30.0, 60.0); }
    if m.contains("gpt-3.5") { return (0.5, 1.5); }
    if m.contains("claude-3-5") { return (3.0, 15.0); }
    if m.contains("claude-3") { return (3.0, 15.0); }
    if m.contains("gemini-1.5-pro") { return (3.5, 10.5); }
    if m.contains("gemini-1.5-flash") { return (0.35, 1.05); }
    if m.contains("deepseek") { return (0.14, 0.28); }
    // default / unknown
    (1.0, 3.0)
}
