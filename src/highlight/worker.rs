//! One worker per application; newest revision replaces queued work for each document.
use super::{
    Span, Syntax,
    engine::{Document, Engine},
};
use eframe::egui::Context;
use std::{
    collections::HashMap,
    sync::{
        Arc, Condvar, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
};

struct Job {
    id: u64,
    revision: u64,
    syntax: Option<Syntax>,
    text: Arc<str>,
    latest: Arc<AtomicU64>,
}

#[derive(Default)]
struct Queue {
    jobs: Mutex<HashMap<u64, Job>>,
    wake: Condvar,
    stop: AtomicBool,
}

pub(crate) struct Output {
    pub id: u64,
    revision: u64,
    spans: Arc<[Span]>,
}

pub struct Worker {
    queue: Arc<Queue>,
    results: mpsc::Receiver<Output>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Worker {
    pub fn new(ctx: Context) -> Self {
        let queue = Arc::new(Queue::default());
        let (tx, results) = mpsc::channel();
        let shared = queue.clone();
        let thread = thread::Builder::new()
            .name("syntax-analysis".into())
            .spawn(move || {
                let engine = Engine::new();
                let mut documents: HashMap<u64, (Weak<AtomicU64>, Document)> = HashMap::new();
                loop {
                    let job = {
                        let mut pending = shared.jobs.lock().expect("syntax queue");
                        while pending.is_empty() && !shared.stop.load(Ordering::Relaxed) {
                            pending = shared.wake.wait(pending).expect("syntax queue");
                        }
                        if shared.stop.load(Ordering::Relaxed) {
                            break;
                        }
                        let id = *pending.keys().next().expect("queued document");
                        pending.remove(&id).expect("queued document")
                    };
                    documents.retain(|_, (owner, _)| owner.strong_count() > 0);
                    let document = &mut documents
                        .entry(job.id)
                        .or_insert_with(|| (Arc::downgrade(&job.latest), Document::default()))
                        .1;
                    let cancelled =
                        || shared.stop.load(Ordering::Relaxed) || job.latest.load(Ordering::Relaxed) != job.revision;
                    if cancelled() {
                        continue;
                    }
                    let spans = document.analyze(&engine, job.syntax, job.text.clone(), cancelled).unwrap_or_default();
                    if !cancelled() {
                        if tx.send(Output { id: job.id, revision: job.revision, spans: spans.into() }).is_err() {
                            break;
                        }
                        ctx.request_repaint();
                    }
                }
            })
            .expect("start syntax analysis worker");
        Self { queue, results, thread: Some(thread) }
    }

    pub(crate) fn drain(&self) -> impl Iterator<Item = Output> + '_ {
        self.results.try_iter()
    }

    fn submit(&self, job: Job) {
        self.queue.jobs.lock().expect("syntax queue").insert(job.id, job);
        self.queue.wake.notify_one();
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        {
            let _pending = self.queue.jobs.lock().expect("syntax queue");
            self.queue.stop.store(true, Ordering::Relaxed);
            self.queue.wake.notify_one();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn remap_unchanged(old: &str, new: &str, spans: &[Span]) -> Vec<Span> {
    let mut prefix = old.bytes().zip(new.bytes()).take_while(|(a, b)| a == b).count();
    while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let mut suffix = old.as_bytes()[prefix..]
        .iter()
        .rev()
        .zip(new.as_bytes()[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) {
        suffix -= 1;
    }
    let (old_end, new_end) = (old.len() - suffix, new.len() - suffix);
    let mut output = Vec::new();
    for span in spans {
        if span.range.start < prefix {
            super::push_span(&mut output, span.range.start..span.range.end.min(prefix), span.color);
        }
        if span.range.end > old_end {
            super::push_span(
                &mut output,
                new_end + span.range.start.max(old_end) - old_end..new_end + span.range.end - old_end,
                span.color,
            );
        }
    }
    output
}

#[derive(Default)]
pub struct Analysis {
    source: Arc<str>,
    syntax: Option<Syntax>,
    initialized: bool,
    latest: Arc<AtomicU64>,
    colors_revision: u64,
    spans: Arc<[Span]>,
}

impl Analysis {
    pub fn requires_worker(text: &str, syntax: Option<Syntax>) -> bool {
        match syntax {
            Some(syntax) => syntax.language().engine.is_some(),
            None => text.contains("```") || text.contains("~~~"),
        }
    }

    pub fn update(&mut self, id: u64, text: &str, syntax: Option<Syntax>, worker: Option<&Worker>) {
        if self.initialized && self.source.as_ref() == text && self.syntax == syntax {
            return;
        }
        let provisional = if self.initialized && self.syntax == syntax && Self::requires_worker(text, syntax) {
            remap_unchanged(&self.source, text, &self.spans)
        } else {
            Vec::new()
        };
        self.initialized = true;
        self.source = Arc::from(text);
        self.syntax = syntax;
        let revision = self.latest.fetch_add(1, Ordering::Relaxed) + 1;
        // Only unchanged UTF-8 ranges retain provisional colors, translated to the new buffer.
        // Changed lexical context is corrected by the worker's exact revision result.
        self.spans = provisional.into();
        self.colors_revision += 1;
        match syntax {
            Some(syntax) if syntax.language().engine.is_none() => {
                self.spans = super::native_spans(text, syntax).into();
            }
            None if !text.contains("```") && !text.contains("~~~") => {}
            _ => {
                if let Some(worker) = worker {
                    worker.submit(Job { id, revision, syntax, text: self.source.clone(), latest: self.latest.clone() });
                }
            }
        }
    }

    pub(crate) fn accept(&mut self, output: Output) {
        if output.revision == self.latest.load(Ordering::Relaxed) {
            self.spans = output.spans;
            self.colors_revision += 1;
        }
    }

    pub fn spans(&self) -> &[Span] {
        &self.spans
    }
    pub fn revision(&self) -> u64 {
        self.colors_revision
    }
}

impl Drop for Analysis {
    fn drop(&mut self) {
        self.latest.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn provisional_colors_are_remapped_around_utf8_edits() {
        let old = "abc🙂def";
        let new = "abcżółw🙂def";
        let spans = vec![Span { range: 0..old.len(), color: eframe::egui::Color32::RED }];
        let mapped = remap_unchanged(old, new, &spans);
        assert_eq!(mapped.len(), 2);
        assert_eq!(&new[mapped[0].range.clone()], "abc");
        assert_eq!(&new[mapped[1].range.clone()], "🙂def");
        let deleted = remap_unchanged(new, old, &mapped);
        assert!(deleted.iter().all(|s| old.is_char_boundary(s.range.start) && old.is_char_boundary(s.range.end)));
    }

    #[test]
    fn worker_delivers_latest_revision_without_applying_stale_offsets() {
        let worker = Worker::new(Context::default());
        let mut analysis = Analysis::default();
        analysis.update(1, "fn old() {}", Some(Syntax::Rust), Some(&worker));
        let stale = analysis.latest.load(Ordering::Relaxed);
        let current = "// żółw🙂\nfn current() {}";
        analysis.update(1, current, Some(Syntax::Rust), Some(&worker));
        analysis.accept(Output {
            id: 1,
            revision: stale,
            spans: vec![Span { range: 100..200, color: eframe::egui::Color32::RED }].into(),
        });
        assert!(analysis.spans().is_empty());
        let deadline = Instant::now() + Duration::from_secs(10);
        while analysis.spans().is_empty() {
            let output = worker.results.recv_timeout(deadline.saturating_duration_since(Instant::now())).unwrap();
            analysis.accept(output);
        }
        assert!(analysis.spans().iter().all(|s| current.is_char_boundary(s.range.start)
            && current.is_char_boundary(s.range.end)
            && s.range.end <= current.len()));
        assert_eq!(analysis.spans().first().unwrap().color, crate::md::DIM);
        let revision = analysis.revision();
        analysis.update(1, current, Some(Syntax::Rust), Some(&worker));
        assert_eq!(analysis.revision(), revision);
    }
}
