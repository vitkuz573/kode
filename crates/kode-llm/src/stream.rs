// stream.rs — helpers for consuming StreamChunk channels
use crate::client::StreamChunk;
use tokio::sync::mpsc::Receiver;

/// Collect all stream chunks into a final string + usage
pub async fn collect_stream(mut rx: Receiver<StreamChunk>) -> (String, u64, u64) {
    let mut text = String::new();
    let mut prompt = 0u64;
    let mut completion = 0u64;

    while let Some(chunk) = rx.recv().await {
        match chunk {
            StreamChunk::Delta(s) => text.push_str(&s),
            StreamChunk::Done { prompt_tokens, completion_tokens, .. } => {
                prompt = prompt_tokens;
                completion = completion_tokens;
            }
            _ => {}
        }
    }
    (text, prompt, completion)
}
