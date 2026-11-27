//! One-pass symbol table built per parse.
//!
//! The table is the source of truth for name resolution. It's built once
//! when a document is parsed, then queried by goto-definition, the
//! unresolved-reference diagnostic, and completion.
//!
//! Scoping rules (q):
//! - q has no closures: a lambda sees only its own params/locals and
//!   globals, never the params or locals of an enclosing lambda.
//! - Lambda parameters are visible only inside that lambda.
//! - Implicit `x`/`y`/`z` exist when a lambda has no `[...]` param list.
//! - Locals are fixed when a lambda compiles: a name is local iff it is a
//!   parameter or the target of a plain `name:` anywhere in the body. Every
//!   other assignment (`::`, `+:`, `+::`, `a[i]:`) changes that local if
//!   there is one, else the global. A name containing a dot is always global.
//! - Every assignment at top level (no enclosing lambda) is global.
//! - `\d .ns` / `\d .` changes the active namespace; bare globals defined or
//!   referenced in that region are implicitly qualified as `.ns.name`, except
//!   that a top-level `name::` assigns the root.
//!
//! Each rule is checked against a real q in `binding_oracle`.

use std::collections::{HashMap, HashSet};

use q_parser::{SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken, TextRange};
use smol_str::SmolStr;

use crate::syntax::{first_token, has_ancestor, tokens};

#[derive(Default)]
pub struct SymTable {
    /// Global defs keyed by name. Each entry is a list of definition byte
    /// offsets, sorted ascending.
    globals: HashMap<SmolStr, Vec<u32>>,
    /// Every lambda in the file, in DFS order.
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
        // Lambda assignments other than `name:`, as (scope, name, offset,
        // namespace): local or global is known once the lambda's locals are.
        let mut amends: Vec<(usize, SmolStr, u32, SmolStr)> = Vec::new();
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
                && let Some(tok) = first_token(&node)
            {
                t.idents.insert(SmolStr::new(tok.text()));
            }

            // Track \d .namespace and system "d .namespace" directives.
            if kind == SyntaxKind::SystemCmdStmt {
                if let Some(cmd_tok) = tokens(&node).find(|t| t.kind() == SyntaxKind::SystemCmd)
                    && let Some(ns) = parse_d_directive(cmd_tok.text())
                {
                    let off: u32 = cmd_tok.text_range().start().into();
                    active_ns = ns.clone();
                    t.ns_changes.push((off, ns));
                }
            } else if kind == SyntaxKind::ApplyExpr
                && let Some(ns) = parse_system_d_call(&node)
            {
                let off: u32 = node.text_range().start().into();
                active_ns = ns.clone();
                t.ns_changes.push((off, ns));
            }

            if kind == SyntaxKind::Lambda {
                let scope_idx = t.lambdas.len();
                let plist = node.children().find(|c| c.kind() == SyntaxKind::ParamList);
                let params = plist.iter().flat_map(|pl| {
                    pl.children_with_tokens()
                        .filter_map(SyntaxElement::into_token)
                        .filter(|tok| tok.kind() == SyntaxKind::Ident)
                        .map(|tok| (SmolStr::new(tok.text()), tok.text_range().start().into()))
                });
                t.lambdas.push(LambdaScope {
                    range: node.text_range(),
                    has_param_list: plist.is_some(),
                    params: params.collect(),
                    locals: Vec::new(),
                });
                scope_stack.push(scope_idx);
                // PopScope runs after all children have been processed.
                work.push(Step::PopScope);
            } else if kind == SyntaxKind::BinExpr {
                t.record_bin_expr(&node, &scope_stack, &active_ns, &mut amends);
            }

            // Push children in reverse so leftmost is visited first.
            let children: Vec<SyntaxNode> = node.children().collect();
            for child in children.into_iter().rev() {
                work.push(Step::Visit(child));
            }
        }

        for (idx, name, off, ns) in amends {
            let scope = &mut t.lambdas[idx];
            let local = scope
                .params
                .iter()
                .chain(&scope.locals)
                .any(|(n, _)| *n == name)
                || (!scope.has_param_list && matches!(name.as_str(), "x" | "y" | "z"));
            if local {
                scope.locals.push((name, off));
            } else {
                t.globals.entry(qualify(&name, &ns)).or_default().push(off);
            }
        }
        for scope in &mut t.lambdas {
            scope.locals.sort_by_key(|(_, o)| *o);
        }
        for offsets in t.globals.values_mut() {
            offsets.sort_unstable();
        }

        t
    }

    fn record_bin_expr(
        &mut self,
        bin: &SyntaxNode,
        stack: &[usize],
        active_ns: &str,
        amends: &mut Vec<(usize, SmolStr, u32, SmolStr)>,
    ) {
        // Column definitions inside a table constructor are not variables.
        if has_ancestor(bin, SyntaxKind::TableExpr) {
            return;
        }
        let Some(op) = bin
            .children_with_tokens()
            .filter_map(SyntaxElement::into_token)
            .find(|t| {
                matches!(
                    t.kind(),
                    SyntaxKind::Colon | SyntaxKind::ColonColon | SyntaxKind::CompoundAssign
                )
            })
        else {
            return;
        };
        let Some(lhs) = bin.first_child() else { return };
        let is_name =
            |t: &SyntaxToken| matches!(t.kind(), SyntaxKind::Ident | SyntaxKind::DottedIdent);
        // `name:` and list patterns `(a;b):` declare; the rest amend.
        let pattern = matches!(lhs.kind(), SyntaxKind::ListExpr | SyntaxKind::ParenExpr);
        let declares = op.kind() == SyntaxKind::Colon && lhs.kind() != SyntaxKind::IndexExpr;
        let targets: Vec<SyntaxToken> = if pattern {
            lhs.children()
                .filter_map(|entry| tokens(&entry).find(is_name))
                .collect()
        } else {
            first_token(&lhs).filter(is_name).into_iter().collect()
        };
        for tok in targets {
            let name = SmolStr::new(tok.text());
            let off = tok.text_range().start().into();
            match stack.last().copied() {
                // A top-level `name::` assigns the root, ignoring `\d`.
                None if op.kind() == SyntaxKind::ColonColon => {
                    self.globals.entry(name).or_default().push(off);
                }
                None => self
                    .globals
                    .entry(qualify(&name, active_ns))
                    .or_default()
                    .push(off),
                Some(_) if name.contains('.') => {
                    self.globals
                        .entry(qualify(&name, active_ns))
                        .or_default()
                        .push(off);
                }
                Some(idx) if declares => self.lambdas[idx].locals.push((name, off)),
                Some(idx) => amends.push((idx, name, off, SmolStr::new(active_ns))),
            }
        }
    }

    /// Active namespace at `offset` based on `\d` directives seen so far.
    /// Returns `""` (root) or `".foo"`.
    #[allow(clippy::cast_possible_truncation)]
    pub fn active_ns_at(&self, offset: usize) -> &str {
        let off = offset as u32;
        let i = self.ns_changes.partition_point(|(o, _)| *o <= off);
        if i == 0 {
            ""
        } else {
            self.ns_changes[i - 1].1.as_str()
        }
    }

    /// Find the innermost lambda whose range contains `cursor`, or `None`
    /// if `cursor` is at top level.
    ///
    /// `self.lambdas` is built in DFS preorder, so it is sorted by start
    /// offset. Binary-search for the rightmost lambda starting at or before
    /// `cursor`, then walk back past siblings that ended before `cursor`.
    #[allow(clippy::cast_possible_truncation)]
    fn innermost_lambda(&self, cursor: usize) -> Option<usize> {
        let off = cursor as u32;
        let hi = self
            .lambdas
            .partition_point(|l| u32::from(l.range.start()) <= off);
        (0..hi)
            .rev()
            .find(|&i| off < u32::from(self.lambdas[i].range.end()))
    }

    /// The definition sites of `name` visible at `cursor`: the innermost
    /// lambda's params, then its locals, then implicit `x`/`y`/`z` (only
    /// without a `[...]` list); otherwise globals, preferring the
    /// `\d`-namespace-qualified form. Enclosing lambdas are not consulted.
    fn binding(&self, cursor: usize, name: &str) -> Option<Binding<'_>> {
        let named = |list: &[(SmolStr, u32)]| -> Vec<u32> {
            list.iter()
                .filter(|(n, _)| n == name)
                .map(|(_, o)| *o)
                .collect()
        };
        if let Some(idx) = self.innermost_lambda(cursor) {
            let scope = &self.lambdas[idx];
            let params = named(&scope.params);
            if !params.is_empty() {
                return Some(Binding::Param(params[0], params));
            }
            let locals = named(&scope.locals);
            if !locals.is_empty() {
                return Some(Binding::Local(locals));
            }
            if !scope.has_param_list && matches!(name, "x" | "y" | "z") {
                let start = scope.range.start().into();
                return Some(Binding::Param(start, vec![start]));
            }
        }

        // Inside a namespace prefer the qualified global over a root-level
        // name with the same spelling.
        let ns = self.active_ns_at(cursor);
        if !ns.is_empty()
            && !name.starts_with('.')
            && let Some(list) = self.globals.get(format!("{ns}.{name}").as_str())
        {
            return Some(Binding::Global(list));
        }
        self.globals.get(name).map(|l| Binding::Global(l))
    }

    /// Resolve `name` against the lexical scope at byte `cursor`.
    pub fn resolve(&self, cursor: usize, name: &str) -> Option<u32> {
        match self.binding(cursor, name)? {
            Binding::Param(first, _) => Some(first),
            // Last local before cursor; else first after — covers q's
            // right-to-left binding inside `(c:key db) like …`.
            Binding::Local(list) => last_before(&list, cursor)
                .or_else(|| list.iter().copied().find(|&o| o as usize >= cursor)),
            Binding::Global(list) => last_before(list, cursor).or(list.last().copied()),
        }
    }

    /// Last global def of `name` before `cursor`, else the last overall.
    fn resolve_global(&self, cursor: usize, name: &str) -> Option<u32> {
        let list = self.globals.get(name)?;
        last_before(list, cursor).or(list.last().copied())
    }

    /// All offsets that bind `name` in the same scope as the def the cursor
    /// resolves to. Used by find-references / rename so that rebindings
    /// (`a:1; a:2; a`) are one symbol. Empty if `name` is not visible.
    pub fn def_offsets_for(&self, cursor: usize, name: &str) -> Vec<u32> {
        match self.binding(cursor, name) {
            Some(Binding::Param(_, all) | Binding::Local(all)) => all,
            Some(Binding::Global(list)) => list.to_vec(),
            None => Vec::new(),
        }
    }

    /// If `name` at `cursor` resolves via namespace fallback AND that resolution
    /// is not shadowed by a lambda param or local, returns the qualified form.
    /// Used by hover to surface the full name.
    pub fn qualified_for(&self, cursor: usize, name: &str) -> Option<SmolStr> {
        let ns = self.active_ns_at(cursor);
        if name.starts_with('.') || ns.is_empty() {
            return None;
        }
        let q = format!("{ns}.{name}");
        let global_off = self.resolve_global(cursor, &q)?;
        // A shadowing param/local resolves to a different offset.
        (self.resolve(cursor, name)? == global_off).then(|| SmolStr::new(q))
    }

    /// All identifier texts seen in the document (for completion).
    pub fn idents(&self) -> impl Iterator<Item = &str> {
        self.idents.iter().map(SmolStr::as_str)
    }

    /// All global definitions, yielding `(name, offsets)` pairs.
    pub fn global_entries(&self) -> impl Iterator<Item = (&str, &[u32])> {
        self.globals.iter().map(|(k, v)| (k.as_str(), v.as_slice()))
    }

    /// All byte offsets where `name` is globally defined (empty if none).
    pub fn global_def_offsets(&self, name: &str) -> &[u32] {
        self.globals.get(name).map_or(&[], Vec::as_slice)
    }
}

/// Last offset in `list` (in list order) before `cursor`.
fn last_before(list: &[u32], cursor: usize) -> Option<u32> {
    list.iter().copied().rfind(|&o| (o as usize) < cursor)
}

/// Where a name is bound; see [`SymTable::binding`].
enum Binding<'a> {
    /// Explicit or implicit lambda parameter: `(resolved, all sites)`.
    Param(u32, Vec<u32>),
    Local(Vec<u32>),
    Global(&'a [u32]),
}

/// Qualify a global `name` with `ns` unless it starts with a dot.
fn qualify(name: &str, ns: &str) -> SmolStr {
    if !ns.is_empty() && !name.starts_with('.') {
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
    let func_ident = tokens(&func).find(|t| t.kind() == SyntaxKind::Ident)?;
    if func_ident.text() != "system" {
        return None;
    }
    let arg = children.next()?;
    if arg.kind() != SyntaxKind::LiteralExpr {
        return None;
    }
    let str_tok = tokens(&arg).find(|t| t.kind() == SyntaxKind::String)?;
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

#[cfg(test)]
mod tests {
    use crate::document::Document;

    #[test]
    fn global_def_offsets_lists_only_globals() {
        let st = |src: &str| Document::new(src.to_string(), 0);
        assert_eq!(
            st("foo:1; foo:2").sym_table().global_def_offsets("foo"),
            [0, 7]
        );
        assert_eq!(st("foo:1").sym_table().global_def_offsets("bar"), [0u32; 0]);
        // lambda locals are not globals
        assert_eq!(
            st("f:{x:42; x}").sym_table().global_def_offsets("x"),
            [0u32; 0]
        );
    }

    #[test]
    fn torq_style_cp_in_cond_progn_indexed_as_proc_cp() {
        // torq.q assigns `cp` inside `$[cond; [cp:{.z.p}]; ...]` under `\d .proc`.
        // Must be indexed as `.proc.cp`, not bare `cp`.
        let src = "\\d .proc\n$[1b;[cp:{.z.p};cd:{.z.d}];[cp:{.z.P};cd:{.z.D}]];\n\\d .";
        let doc = Document::new(src.to_string(), 0);
        let st = doc.sym_table();
        let entries: Vec<_> = st.global_entries().collect();
        assert!(
            !st.global_def_offsets(".proc.cp").is_empty(),
            "expected .proc.cp in globals, got: {entries:?}"
        );
        assert!(
            st.global_def_offsets("cp").is_empty(),
            "bare `cp` must not appear — must be qualified, got: {entries:?}"
        );
    }

    #[test]
    fn torq_style_cross_file_goto_def() {
        use crate::goto_def::{goto_definition, tests::locations};
        use crate::workspace_index::tests::{doc, index, uri};

        let idx = index(&[("torq.q", "\\d .proc\n$[1b;[cp:{.z.p}];[cp:{.z.P}]];\n\\d .")]);
        // cache.q references .proc.cp inside \d .cache
        let cache_src = "\\d .cache\nadd:{[f] now:.proc.cp[];now}\n\\d .";
        let cache = doc(cache_src);
        let pos = cache.position_of(cache_src.find(".proc.cp").unwrap());
        let locs = locations(goto_definition(&cache, pos, &uri("cache.q"), &idx));
        assert_eq!(locs[0].uri, uri("torq.q"));
    }
}
