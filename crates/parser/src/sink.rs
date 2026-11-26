use rowan::{GreenNode, GreenNodeBuilder, Language};

use crate::parser::Event;
use crate::syntax_kind::QLang;

/// Build the green tree from the parser's event stream.
///
/// A `Start` with a `forward_parent` chain opens its ancestors first
/// (outermost-first); each visited ancestor is replaced with a Placeholder so
/// it isn't opened again when the loop reaches it.
pub fn build(mut events: Vec<Event>) -> GreenNode {
    let mut builder = GreenNodeBuilder::new();
    let mut chain = Vec::new();
    for i in 0..events.len() {
        match std::mem::replace(&mut events[i], Event::Placeholder) {
            Event::Start {
                kind,
                mut forward_parent,
            } => {
                chain.push(kind);
                let mut idx = i;
                while let Some(offset) = forward_parent {
                    idx += offset;
                    let Event::Start {
                        kind,
                        forward_parent: next,
                    } = std::mem::replace(&mut events[idx], Event::Placeholder)
                    else {
                        unreachable!("forward_parent must point to a Start event")
                    };
                    chain.push(kind);
                    forward_parent = next;
                }
                for kind in chain.drain(..).rev() {
                    builder.start_node(QLang::kind_to_raw(kind));
                }
            }
            Event::Token { kind, text } => builder.token(QLang::kind_to_raw(kind), &text),
            Event::Finish => builder.finish_node(),
            Event::Placeholder => {}
        }
    }
    builder.finish()
}
