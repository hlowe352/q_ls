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
//! - Plain `name:` inside a lambda is a local visible only in that lambda.
//! - `name::` (double-colon) and dotted `name.foo:` are globals visible
//!   anywhere in the file, regardless of which lambda they textually live in.
//! - Plain `name:` at top level (no enclosing lambda) is also a global.
//! - `\d .ns` / `\d .` changes the active namespace; bare globals defined or
//!   referenced in that region are implicitly qualified as `.ns.name`.

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
        // Deferred compound-assigns inside lambdas: (scope_idx, name, offset, active_ns).
        // Resolved in a second pass: only recorded as locals if the name has no
        // global def (meaning the compound-assign introduces the binding).
        let mut deferred_compound: Vec<(usize, SmolStr, u32, SmolStr)> = Vec::new();
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
                t.record_bin_expr(&node, &scope_stack, &active_ns, &mut deferred_compound);
            }

            // Push children in reverse so leftmost is visited first.
            let children: Vec<SyntaxNode> = node.children().collect();
            for child in children.into_iter().rev() {
                work.push(Step::Visit(child));
            }
        }

        // Second pass: resolve deferred compound-assigns inside lambdas.
        // Rules:
        //   (a) If the name resolves to a global (bare or namespace-qualified),
        //       the compound-assign amends that global — don't create a local.
        //   (b) Only the FIRST compound-assign per (scope, name) introduces the
        //       local binding; subsequent ones amend the already-created local.
        let mut seen_local = HashSet::new();
        for (scope_idx, name, off, ns) in deferred_compound {
            let qualified = qualify(&name, true, &ns);
            if t.globals.contains_key(&name) || t.globals.contains_key(&qualified) {
                continue; // amends a global
            }
            // Only the first compound-assign in this scope creates the binding.
            if seen_local.insert((scope_idx, name.clone())) {
                t.lambdas[scope_idx].locals.push((name, off));
            }
        }

        t
    }

    fn record_bin_expr(
        &mut self,
        bin: &SyntaxNode,
        stack: &[usize],
        active_ns: &str,
        deferred_compound: &mut Vec<(usize, SmolStr, u32, SmolStr)>,
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
        let is_double_colon = op.kind() == SyntaxKind::ColonColon;
        let in_lambda = stack.last().copied();
        let Some(lhs) = bin.first_child() else { return };
        let is_name =
            |t: &SyntaxToken| matches!(t.kind(), SyntaxKind::Ident | SyntaxKind::DottedIdent);

        // List-pattern: `(a; b:type; c):rhs`. Each element binds at the
        // outer assign's scope (`::` is unusual here but legal: global).
        if matches!(lhs.kind(), SyntaxKind::ListExpr | SyntaxKind::ParenExpr) {
            for entry in lhs.children() {
                if let Some(tok) = tokens(&entry).find(is_name) {
                    let is_global = is_double_colon || in_lambda.is_none();
                    self.bind(&tok, is_global, in_lambda, active_ns);
                }
            }
            return;
        }

        // Single-name assign: `name:` or `.ns.name:`.
        let Some(tok) = first_token(&lhs).filter(is_name) else {
            return;
        };
        let is_global =
            is_double_colon || tok.kind() == SyntaxKind::DottedIdent || in_lambda.is_none();
        if let (SyntaxKind::CompoundAssign, Some(idx)) = (op.kind(), in_lambda) {
            // Local only if no global def exists — resolved after pass 1.
            let name = qualify(tok.text(), is_global, active_ns);
            let off = tok.text_range().start().into();
            deferred_compound.push((idx, name, off, SmolStr::new(active_ns)));
            return;
        }
        self.bind(&tok, is_global, in_lambda, active_ns);
    }

    /// Record `tok` as a global (namespace-qualified unless already dotted)
    /// or as a local of the enclosing lambda.
    fn bind(&mut self, tok: &SyntaxToken, is_global: bool, in_lambda: Option<usize>, ns: &str) {
        let name = qualify(tok.text(), is_global, ns);
        let off = tok.text_range().start().into();
        if is_global {
            self.globals.entry(name).or_default().push(off);
        } else if let Some(idx) = in_lambda {
            self.lambdas[idx].locals.push((name, off));
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

/// Qualify a global `name` with `ns` unless it is already dotted.
fn qualify(name: &str, is_global: bool, ns: &str) -> SmolStr {
    if is_global && !ns.is_empty() && !name.starts_with('.') {
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
    fn global_def_offsets_returns_global_sites() {
        let doc = Document::new("foo:1; foo:2".to_string(), 0);
        let offs = doc.sym_table().global_def_offsets("foo");
        assert_eq!(offs.len(), 2, "got: {offs:?}");
    }

    #[test]
    fn global_def_offsets_empty_for_unknown() {
        let doc = Document::new("foo:1".to_string(), 0);
        assert_eq!(doc.sym_table().global_def_offsets("bar"), [0u32; 0]);
    }

    #[test]
    fn global_def_offsets_empty_for_lambda_local() {
        // `x` here is a lambda local, NOT a global.
        let doc = Document::new("f:{x:42; x}".to_string(), 0);
        assert!(
            doc.sym_table().global_def_offsets("x").is_empty(),
            "lambda locals must not appear as globals"
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
    fn real_torq_q_indexes_proc_cp() {
        let src = include_str!("../../parser/tests/data/real_q/torq.q");
        std::thread::Builder::new()
            .stack_size(8 * 1024 * 1024)
            .spawn(move || {
                let doc = Document::new(src.to_string(), 0);
                assert_ne!(doc.sym_table().global_def_offsets(".proc.cp"), [0u32; 0]);
            })
            .unwrap()
            .join()
            .unwrap();
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

    #[test]
    fn compound_assign_in_lambda_creates_local_when_no_global() {
        // `a,:value` inside a lambda when `a` has no global def → creates a local.
        // Subsequent uses of `a` in the lambda must resolve (no unresolved warning).
        let src = "f:{[path] a,:raze 1 2 3; a}";
        let doc = Document::new(src.to_string(), 0);
        let table = doc.sym_table();
        let a_use_off = src.rfind("; a}").unwrap() + 2; // the bare `a` at end
        let resolved = table.resolve(a_use_off, "a");
        assert!(
            resolved.is_some(),
            "a introduced by `,: ` must resolve inside lambda"
        );
    }

    #[test]
    fn compound_assign_second_occurrence_not_a_new_def() {
        // Only the FIRST `a,:` creates the local; subsequent ones amend it.
        // goto-def from a use after the second `,: ` must point to the FIRST.
        let src = "f:{a,:1 2; a,:3 4; a}";
        let doc = Document::new(src.to_string(), 0);
        let table = doc.sym_table();
        let first_def_off = src.find("a,:1").unwrap();
        let a_use_off = src.rfind("; a}").unwrap() + 2;
        let resolved = table.resolve(a_use_off, "a").expect("a must resolve");
        assert_eq!(
            resolved as usize, first_def_off,
            "must point to first `,: `, not the second one"
        );
    }

    #[test]
    fn compound_assign_in_lambda_does_not_shadow_global() {
        // `loadedf:enlist enlist""` is a global def.
        // Inside a lambda, `loadedf,:enlist x` amends the global — it must NOT
        // create a new local that shadows it.  goto-def on a use inside the
        // lambda must resolve to the global def, not the amend site.
        use crate::goto_def::{goto_definition, tests::locations};
        use crate::workspace_index::{WorkspaceIndex, tests::uri};

        let src = "loadedf:enlist enlist\"\"\nf:{[x] if[x in loadedf;:()]; loadedf,:enlist x}";
        let doc = Document::new(src.to_string(), 0);
        // cursor on `loadedf` in `x in loadedf` (use inside lambda)
        let pos = doc.position_of(src.find("x in loadedf").unwrap() + "x in ".len());
        let locs = locations(goto_definition(
            &doc,
            pos,
            &uri("a.q"),
            &WorkspaceIndex::default(),
        ));
        let def_off = src.find("loadedf:enlist").unwrap(); // line 1 global def
        assert_eq!(
            doc.offset_of(locs[0].range.start),
            def_off,
            "global def, not amend site"
        );
    }
}
