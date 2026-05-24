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
//! - `\d .ns` / `\d .` changes the active namespace; bare globals defined or
//!   referenced in that region are implicitly qualified as `.ns.name`.

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
    /// Namespace change points from `\d` directives, sorted by byte offset.
    /// Each entry is `(offset_of_directive, new_namespace)`.
    /// `""` means root namespace; `".foo"` means namespace `.foo`.
    ns_changes: Vec<(u32, SmolStr)>,
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
        let mut active_ns = SmolStr::default(); // "" = root; ".foo" = namespace .foo
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
                    .filter_map(q_parser::SyntaxElement::into_token)
                    .find(|t| !t.kind().is_trivia())
            {
                t.idents.insert(SmolStr::new(tok.text()));
            }

            // Track \d .namespace and system "d .namespace" directives.
            if kind == SyntaxKind::SystemCmdStmt {
                if let Some(cmd_tok) = node
                    .descendants_with_tokens()
                    .filter_map(q_parser::SyntaxElement::into_token)
                    .find(|t| t.kind() == SyntaxKind::SystemCmd)
                    && let Some(ns) = parse_d_directive(cmd_tok.text()) {
                        let off: u32 = cmd_tok.text_range().start().into();
                        active_ns = ns.clone();
                        t.ns_changes.push((off, ns));
                    }
            } else if kind == SyntaxKind::ApplyExpr
                && let Some(ns) = parse_system_d_call(&node) {
                    let off: u32 = node.text_range().start().into();
                    active_ns = ns.clone();
                    t.ns_changes.push((off, ns));
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
                        .filter_map(q_parser::SyntaxElement::into_token)
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
                t.record_bin_expr(&node, &scope_stack, &active_ns);
            }

            // Push children in reverse so leftmost is visited first.
            let children: Vec<SyntaxNode> = node.children().collect();
            for child in children.into_iter().rev() {
                work.push(Step::Visit(child));
            }
        }

        t
    }

    fn record_bin_expr(&mut self, bin: &SyntaxNode, stack: &[usize], active_ns: &str) {
        // Column definitions inside a TableExpr (keyed or plain table
        // constructor) are not variable assignments — skip them.
        if bin.ancestors().any(|n| n.kind() == SyntaxKind::TableExpr) {
            return;
        }

        // Look for an assignment colon directly on this BinExpr.
        let Some(op) = bin
            .children_with_tokens()
            .filter_map(q_parser::SyntaxElement::into_token)
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
            .filter_map(q_parser::SyntaxElement::into_token)
            .find(|t| !t.kind().is_trivia())
            .filter(|t| t.kind() == SyntaxKind::Ident || t.kind() == SyntaxKind::DottedIdent);

        if let Some(tok) = single_name
            && !matches!(lhs.kind(), SyntaxKind::ListExpr | SyntaxKind::ParenExpr)
        {
            let is_dotted = tok.kind() == SyntaxKind::DottedIdent;
            let off: u32 = tok.text_range().start().into();
            let is_global = is_double_colon || is_dotted || in_lambda.is_none();
            // Qualify bare idents with the active \d namespace when at global scope.
            let name = qualify(tok.text(), is_global && !is_dotted, active_ns);
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
                    .filter_map(q_parser::SyntaxElement::into_token)
                    .find(|t| t.kind() == SyntaxKind::Ident || t.kind() == SyntaxKind::DottedIdent)
                else {
                    continue;
                };
                let is_dotted = tok.kind() == SyntaxKind::DottedIdent;
                let name = qualify(tok.text(), is_global && !is_dotted, active_ns);
                let off: u32 = tok.text_range().start().into();
                if is_global {
                    self.globals.entry(name).or_default().push(off);
                } else if let Some(idx) = in_lambda {
                    self.lambdas[idx].locals.push((name, off));
                }
            }
        }
    }

    /// Active namespace at `offset` based on `\d` directives seen so far.
    /// Returns `""` (root) or `".foo"`.
    #[allow(clippy::cast_possible_truncation)]
    fn active_ns_at(&self, offset: usize) -> &str {
        let off = offset as u32;
        let i = self.ns_changes.partition_point(|(o, _)| *o <= off);
        if i == 0 { "" } else { self.ns_changes[i - 1].1.as_str() }
    }

    /// Find the innermost lambda whose range contains `cursor`, or `None`
    /// if `cursor` is at top level.
    ///
    /// `self.lambdas` is built in DFS preorder, which (because the CST's
    /// `range.start()` is monotonic in preorder) leaves it sorted by start
    /// offset. We binary-search for the rightmost lambda starting at or
    /// before `cursor` and walk back through siblings whose range ended
    /// before `cursor` until we find one whose range still contains it.
    /// The walk-back is bounded by sibling count, not total lambda count.
    #[allow(clippy::cast_possible_truncation)]
    fn innermost_lambda(&self, cursor: usize) -> Option<usize> {
        let off = cursor as u32;
        let mut hi = self.lambdas.partition_point(|l| {
            let s: u32 = l.range.start().into();
            s <= off
        });
        while hi > 0 {
            hi -= 1;
            let e: u32 = self.lambdas[hi].range.end().into();
            if off < e {
                return Some(hi);
            }
        }
        None
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

        // Globals: when inside a namespace, prefer the qualified form over a
        // root-level name with the same spelling.  Fall back to the bare name
        // for names that are not in the active namespace (e.g. dotted idents
        // from other namespaces, or names that genuinely live at root).
        let ns = self.active_ns_at(cursor);
        if !ns.is_empty() && !name.starts_with('.') {
            let qualified = format!("{ns}.{name}");
            if let Some(off) = self.resolve_global(cursor, &qualified) {
                return Some(off);
            }
        }
        self.resolve_global(cursor, name)
    }

    fn resolve_global(&self, cursor: usize, name: &str) -> Option<usize> {
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

    /// All offsets that bind `name` in the same scope as the def the cursor
    /// resolves to. Used by find-references / rename so that multiple
    /// rebindings of the same name (`a:1; a:2; a`) are treated as one
    /// symbol, not several. Returns an empty vec if `name` has no visible
    /// def at `cursor`.
    pub fn def_offsets_for(&self, cursor: usize, name: &str) -> Vec<usize> {
        // Walk the lambda chain looking for the scope that owns `name`.
        let mut current = self.innermost_lambda(cursor);
        while let Some(idx) = current {
            let scope = &self.lambdas[idx];

            let params: Vec<usize> = scope
                .params
                .iter()
                .filter(|(n, _)| n == name)
                .map(|(_, o)| *o as usize)
                .collect();
            if !params.is_empty() {
                return params;
            }

            let locals: Vec<usize> = scope
                .locals
                .iter()
                .filter(|(n, _)| n == name)
                .map(|(_, o)| *o as usize)
                .collect();
            if !locals.is_empty() {
                return locals;
            }

            if !scope.has_param_list && matches!(name, "x" | "y" | "z") {
                return vec![scope.range.start().into()];
            }

            current = scope.parent;
        }

        // Prefer namespace-qualified form when inside a namespace (same
        // priority logic as resolve()).  Fall back to bare name.
        let ns = self.active_ns_at(cursor);
        if !ns.is_empty() && !name.starts_with('.') {
            let qualified = format!("{ns}.{name}");
            let namespaced: Vec<usize> = self.globals
                .get(qualified.as_str())
                .map_or_else(Vec::new, |v| v.iter().map(|&o| o as usize).collect());
            if !namespaced.is_empty() {
                return namespaced;
            }
        }
        self.globals
            .get(name)
            .map(|v| v.iter().map(|&o| o as usize).collect())
            .unwrap_or_default()
    }

    /// If `name` at `cursor` resolves via namespace fallback AND that resolution
    /// is not shadowed by a lambda param or local, returns the qualified form.
    /// Used by hover to surface the full name.
    pub fn qualified_for(&self, cursor: usize, name: &str) -> Option<SmolStr> {
        if name.starts_with('.') {
            return None; // already qualified
        }
        let ns = self.active_ns_at(cursor);
        if ns.is_empty() {
            return None;
        }
        let q = format!("{ns}.{name}");
        let global_off = self.resolve_global(cursor, &q)?;
        // A param or local shadows the global if resolve() returns a different
        // offset (the local def site rather than the global one).
        let resolved_off = self.resolve(cursor, name)?;
        if resolved_off == global_off {
            Some(SmolStr::new(q))
        } else {
            None
        }
    }

    /// All identifier texts seen in the document (for completion).
    pub fn idents(&self) -> impl Iterator<Item = &str> {
        self.idents.iter().map(smol_str::SmolStr::as_str)
    }
}

/// Qualify `name` with `ns` if `should_qualify` and `ns` is non-empty.
fn qualify(name: &str, should_qualify: bool, ns: &str) -> SmolStr {
    if should_qualify && !ns.is_empty() {
        SmolStr::new(format!("{ns}.{name}"))
    } else {
        SmolStr::new(name)
    }
}

/// Detect `system "d .ns"` / `system "d ."` calls and return the new namespace.
fn parse_system_d_call(apply: &SyntaxNode) -> Option<SmolStr> {
    let mut children = apply.children();
    let func = children.next()?;
    if func.kind() != SyntaxKind::IdentExpr {
        return None;
    }
    let func_ident = func
        .descendants_with_tokens()
        .filter_map(q_parser::SyntaxElement::into_token)
        .find(|t| t.kind() == SyntaxKind::Ident)?;
    if func_ident.text() != "system" {
        return None;
    }
    let arg = children.next()?;
    if arg.kind() != SyntaxKind::LiteralExpr {
        return None;
    }
    let str_tok = arg
        .descendants_with_tokens()
        .filter_map(q_parser::SyntaxElement::into_token)
        .find(|t| t.kind() == SyntaxKind::String)?;
    // Strip surrounding quotes then delegate to the same logic as \d.
    let raw = str_tok.text();
    let content = raw.strip_prefix('"')?.strip_suffix('"')?;
    // Must start with 'd'; remainder is the namespace argument.
    let after_d = content.strip_prefix('d')?;
    parse_d_directive(&format!("\\d{after_d}"))
}

/// Parse a `\d` directive token text and return the new active namespace.
/// Returns `Some("")` for `\d .` (root), `Some(".foo")` for `\d .foo`,
/// `None` if the token is not a `\d` directive.
fn parse_d_directive(text: &str) -> Option<SmolStr> {
    let rest = text.strip_prefix("\\d")?.trim();
    if rest.is_empty() || rest == "." {
        Some(SmolStr::default())
    } else if rest.starts_with('.') {
        Some(SmolStr::new(rest))
    } else {
        None
    }
}
