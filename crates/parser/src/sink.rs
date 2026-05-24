use rowan::{GreenNode, GreenNodeBuilder, Language};

use crate::event::Event;
use crate::parser::ParseError;
use crate::syntax_kind::QLang;

pub struct Sink {
    builder: GreenNodeBuilder<'static>,
    events: Vec<Event>,
    errors: Vec<ParseError>,
}

impl Sink {
    #[must_use] 
    pub fn new(events: Vec<Event>, errors: Vec<ParseError>) -> Self {
        Self {
            builder: GreenNodeBuilder::new(),
            events,
            errors,
        }
    }

    #[must_use] 
    pub fn finish(mut self) -> (GreenNode, Vec<ParseError>) {
        // Indices of Start events that have already been emitted as part of a
        // forward_parent chain (so we skip them when we reach them normally).
        let mut already_started: std::collections::HashSet<usize> = std::collections::HashSet::new();

        // We need to iterate by index because we need to walk forward_parent
        // chains which are stored by absolute index.
        let len = self.events.len();
        let mut i = 0;
        while i < len {
            match &self.events[i] {
                Event::Start { kind, forward_parent } => {
                    if already_started.contains(&i) {
                        // Already emitted as part of a chain started from a
                        // child that had a forward_parent pointing here.
                        i += 1;
                        continue;
                    }

                    if forward_parent.is_some() {
                        // This node has a parent waiting further in the event
                        // list.  Collect the full ancestor chain so we can
                        // emit outermost-first.
                        let mut chain: Vec<usize> = vec![i];
                        let mut cur = i;
                        loop {
                            let fp = match &self.events[cur] {
                                Event::Start { forward_parent, .. } => *forward_parent,
                                _ => None,
                            };
                            match fp {
                                Some(offset) => {
                                    let parent_idx = cur + offset;
                                    chain.push(parent_idx);
                                    already_started.insert(parent_idx);
                                    cur = parent_idx;
                                }
                                None => break,
                            }
                        }

                        // Emit nodes outermost-first (reverse of the chain
                        // which was built innermost-first).
                        for &idx in chain.iter().rev() {
                            let k = match &self.events[idx] {
                                Event::Start { kind, .. } => *kind,
                                _ => unreachable!(),
                            };
                            self.builder.start_node(QLang::kind_to_raw(k));
                        }
                    } else {
                        // Simple case: no forward_parent.
                        let k = *kind;
                        self.builder.start_node(QLang::kind_to_raw(k));
                    }
                }
                Event::Token { kind, text } => {
                    let k = *kind;
                    let t = text.clone();
                    self.builder.token(QLang::kind_to_raw(k), t.as_str());
                }
                Event::Finish => {
                    self.builder.finish_node();
                }
                Event::Placeholder => {
                    // Abandoned marker — nothing to emit.
                }
            }
            i += 1;
        }

        let green = self.builder.finish();
        (green, self.errors)
    }
}
