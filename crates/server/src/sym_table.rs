//! One-pass symbol table built per parse.
//!
//! The table is the source of truth for name resolution. It's built once
//! when a document is parsed, then queried by goto-definition, the
//! unresolved-reference diagnostic, and completion.
//!
//! Scoping rules (q):
//! - Lambda parameters are visible only inside that lambda.
//! - Implicit `x`/`y`/`z` exist when a lambda has no `[...]` param list.
//! - Plain `name:` inside a lambda is a local visible only in that lambda.
//! - `name::` (double-colon) and dotted `name.foo:` are globals visible
//!   anywhere in the file, regardless of which lambda they textually live in.
//! - Plain `name:` at top level (no enclosing lambda) is also a global.

use std::collections::{HashMap, HashSet};

use q_parser::{SyntaxKind, SyntaxNode, TextRange};
use smol_str::SmolStr;

#[derive(Default)]
pub struct SymTable {
    /// Global defs keyed by name. Each entry is a list of definition byte
    /// offsets, sorted ascending.
    globals: HashMap<SmolStr, Vec<u32>>,
    /// Every lambda in the file, in DFS order. `parent` indexes back into
    /// this vec.
    lambdas: Vec<LambdaScope>,
    /// All identifier texts seen (idents, dotted idents, namespaces).
    /// Used to feed completion without re-walking the tree.
    idents: HashSet<SmolStr>,
}

struct LambdaScope {
    range: TextRange,
    parent: Option<usize>,
    has_param_list: bool,
    /// Lambda parameters, in source order.
    params: Vec<(SmolStr, u32)>,
    /// Plain `name:` locals (and list-pattern bindings) in this lambda's
    /// own body, *not* counting nested lambdas. Order = source order.
    locals: Vec<(SmolStr, u32)>,
}

impl SymTable {
    pub fn build(root: &SyntaxNode) -> Self {
        // Iterative DFS — recursion blows the stack on deeply nested files
        // (real q files like dbmaint.q exceed the default 2 MB thread stack).
        enum Step {
            Visit(SyntaxNode),
            PopScope,
        }

        let mut t = SymTable::default();
        let mut scope_stack: Vec<usize> = Vec::new();
        let mut work: Vec<Step> = vec![Step::Visit(root.clone())];

        while let Some(step) = work.pop() {
            let node = match step {
                Step::PopScope => {
                    scope_stack.pop();
                    continue;
                }
                Step::Visit(n) => n,
            };

            let kind = node.kind();

            // Harvest identifiers for completion.
            if matches!(kind, SyntaxKind::IdentExpr | SyntaxKind::Namespace)
                && let Some(tok) = node
                    .descendants_with_tokens()
                    .filter_map(|el| el.into_token())
                    .find(|t| !t.kind().is_trivia())
            {
                t.idents.insert(SmolStr::new(tok.text()));
            }

            if kind == SyntaxKind::Lambda {
                let scope_idx = t.lambdas.len();
                let parent = scope_stack.last().copied();
                let has_param_list =
                    node.children().any(|c| c.kind() == SyntaxKind::ParamList);
                let mut params = Vec::new();
                if let Some(plist) =
                    node.children().find(|c| c.kind() == SyntaxKind::ParamList)
                {
                    for tok in plist
                        .children_with_tokens()
                        .filter_map(|el| el.into_token())
                        .filter(|tok| tok.kind() == SyntaxKind::Ident)
                    {
                        let off: u32 = tok.text_range().start().into();
                        params.push((SmolStr::new(tok.text()), off));
                    }
                }
                t.lambdas.push(LambdaScope {
                    range: node.text_range(),
                    parent,
                    has_param_list,
                    params,
                    locals: Vec::new(),
                });
                scope_stack.push(scope_idx);
                // PopScope runs after all children have been processed.
                work.push(Step::PopScope);
            } else if kind == SyntaxKind::BinExpr {
                t.record_bin_expr(&node, &scope_stack);
            }

            // Push children in reverse so leftmost is visited first.
            let children: Vec<SyntaxNode> = node.children().collect();
            for child in children.into_iter().rev() {
                work.push(Step::Visit(child));
            }
        }

        t
    }

    fn record_bin_expr(&mut self, bin: &SyntaxNode, stack: &[usize]) {
        // Look for an assignment colon directly on this BinExpr.
        let Some(op) = bin
            .children_with_tokens()
            .filter_map(|el| el.into_token())
            .find(|t| t.kind() == SyntaxKind::Colon || t.kind() == SyntaxKind::ColonColon)
        else {
            return;
        };
        let is_double_colon = op.kind() == SyntaxKind::ColonColon;
        let in_lambda = stack.last().copied();

        let Some(lhs) = bin.first_child() else { return };

        // Single-name assign: `name:` or `.ns.name:`.
        let single_name = lhs
            .descendants_with_tokens()
            .filter_map(|el| el.into_token())
            .find(|t| !t.kind().is_trivia())
            .filter(|t| t.kind() == SyntaxKind::Ident || t.kind() == SyntaxKind::DottedIdent);

        if let Some(tok) = single_name
            && !matches!(lhs.kind(), SyntaxKind::ListExpr | SyntaxKind::ParenExpr)
        {
            let is_dotted = tok.kind() == SyntaxKind::DottedIdent;
            let name = SmolStr::new(tok.text());
            let off: u32 = tok.text_range().start().into();
            let is_global = is_double_colon || is_dotted || in_lambda.is_none();
            if is_global {
                self.globals.entry(name).or_default().push(off);
            } else if let Some(idx) = in_lambda {
                self.lambdas[idx].locals.push((name, off));
            }
            return;
        }

        // List-pattern: `(a; b:type; c):rhs`. Each LHS element binds at
        // the same scope as the outer assign. `::` is unusual here but
        // legal — treat it as global.
        if matches!(lhs.kind(), SyntaxKind::ListExpr | SyntaxKind::ParenExpr) {
            let is_global = is_double_colon || in_lambda.is_none();
            for entry in lhs.children() {
                let Some(tok) = entry
                    .descendants_with_tokens()
                    .filter_map(|el| el.into_token())
                    .find(|t| t.kind() == SyntaxKind::Ident || t.kind() == SyntaxKind::DottedIdent)
                else {
                    continue;
                };
                let name = SmolStr::new(tok.text());
                let off: u32 = tok.text_range().start().into();
                if is_global {
                    self.globals.entry(name).or_default().push(off);
                } else if let Some(idx) = in_lambda {
                    self.lambdas[idx].locals.push((name, off));
                }
            }
        }
    }

    /// Find the innermost lambda whose range contains `cursor`. Returns the
    /// scope index, or `None` if `cursor` is at top level.
    fn innermost_lambda(&self, cursor: usize) -> Option<usize> {
        let off = cursor as u32;
        // Iterate in reverse: later lambdas in DFS order are deeper or come
        // after; the innermost containing one is the last hit.
        let mut best: Option<usize> = None;
        for (idx, scope) in self.lambdas.iter().enumerate() {
            let s: u32 = scope.range.start().into();
            let e: u32 = scope.range.end().into();
            if s <= off && off < e {
                // Strictly more nested = larger start (since later-started
                // lambdas inside the parent come after it in DFS).
                let deeper = best.is_none_or(|b| {
                    let bs: u32 = self.lambdas[b].range.start().into();
                    s >= bs
                });
                if deeper {
                    best = Some(idx);
                }
            }
        }
        best
    }

    /// Resolve `name` against the lexical scope at byte `cursor`.
    pub fn resolve(&self, cursor: usize, name: &str) -> Option<usize> {
        // Climb lambda chain.
        let mut current = self.innermost_lambda(cursor);
        while let Some(idx) = current {
            let scope = &self.lambdas[idx];

            // Explicit params.
            if let Some(off) = scope.params.iter().find(|(n, _)| n == name).map(|(_, o)| *o) {
                return Some(off as usize);
            }

            // Local `name:` (last before cursor; else first after — covers
            // q's right-to-left binding inside `(c:key db) like …`).
            let mut before: Option<u32> = None;
            let mut after: Option<u32> = None;
            for (n, o) in &scope.locals {
                if n != name {
                    continue;
                }
                if (*o as usize) < cursor {
                    before = Some(*o);
                } else if after.is_none() {
                    after = Some(*o);
                }
            }
            if let Some(o) = before.or(after) {
                return Some(o as usize);
            }

            // Implicit x/y/z: only when the lambda has no `[...]` list.
            if !scope.has_param_list && matches!(name, "x" | "y" | "z") {
                return Some(scope.range.start().into());
            }

            current = scope.parent;
        }

        // Globals: last def before cursor; else last overall.
        let list = self.globals.get(name)?;
        let mut last_overall: Option<u32> = None;
        let mut before: Option<u32> = None;
        for &o in list {
            last_overall = Some(o);
            if (o as usize) < cursor {
                before = Some(o);
            }
        }
        before.or(last_overall).map(|o| o as usize)
    }

    /// All identifier texts seen in the document (for completion).
    pub fn idents(&self) -> impl Iterator<Item = &str> {
        self.idents.iter().map(|s| s.as_str())
    }
}

