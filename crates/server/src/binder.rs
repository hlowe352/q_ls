//! Name resolution: one pass over the CST records every name in a document
//! — identifiers, parameters, and backtick symbols naming a table (``from `t``,
//! `` `t upsert x ``) — with its role and the symbol it means. q's rules,
//! each checked against a real q in `binding_oracle`:
//!
//! - A name is **local** to a lambda iff it is a parameter, an implicit
//!   `x`/`y`/`z` (no `[...]` list), or the target of a plain `name:` anywhere
//!   in that lambda's body: q fixes locals when a lambda compiles, and a
//!   lambda never sees an enclosing lambda's names.
//! - Every other name is a **global**, qualified by the `\d` namespace in
//!   effect: inside `\d .ns` a bare `a` means `.ns.a` (root `a` is not
//!   visible), a name starting with `.` is absolute, and only a top-level
//!   `name::` assigns the root. A name containing a dot is never local.
//! - `::`, `+:`, `+::` and `a[i]:` amend: they change the local if the name
//!   is one, else the global.
//! - A backtick symbol before `set`, `upsert` or `insert` (or first in
//!   `` set[`a;1] ``) assigns that global; `` `..name `` is a root name.
//!
//! Builtins, qSQL column names and table-constructor column names are not
//! names.

use std::collections::{HashMap, HashSet};

use q_parser::{SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken, TextRange, TextSize};
use smol_str::SmolStr;

use crate::builtins::is_builtin;
use crate::syntax::{first_token, in_row_context, tokens};

/// What a name refers to.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Symbol {
    /// A global, by qualified name (`.ns.f`, or `f` in the root).
    Global(SmolStr),
    /// A local of the lambda starting at byte `lambda`.
    Local { lambda: u32, name: SmolStr },
}

/// How a name occurs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Use,
    Param,
    /// `name:` (or `name::` at top level): declares the name.
    Declare,
    /// `::`, `+:`, `+::`, `a[i]:`: changes an existing local or a global.
    Amend,
}

#[derive(Debug)]
pub struct Name {
    /// The name's text in the source (a symbol's without its backtick).
    pub range: TextRange,
    pub role: Role,
    pub symbol: Symbol,
}

impl Name {
    /// Is this occurrence a definition site?
    pub fn is_def(&self) -> bool {
        self.role != Role::Use
    }
}

#[derive(Default)]
pub struct Bindings {
    /// Every name, sorted by position.
    names: Vec<Name>,
    /// Definition offsets of each symbol, sorted.
    defs: HashMap<Symbol, Vec<u32>>,
    /// Definitions whose value is a dictionary or keyed-table literal.
    dictionaries: HashSet<u32>,
}

impl Bindings {
    pub fn build(root: &SyntaxNode) -> Self {
        Builder::default().run(root)
    }

    pub fn names(&self) -> &[Name] {
        &self.names
    }

    /// The name at byte `offset` (inside it, or just after its last byte).
    #[allow(clippy::cast_possible_truncation)]
    pub fn name_at(&self, offset: usize) -> Option<&Name> {
        let off = TextSize::from(offset as u32);
        let i = self.names.partition_point(|n| n.range.start() <= off);
        self.names[..i]
            .last()
            .filter(|n| n.range.contains_inclusive(off))
    }

    /// Definition offsets of `symbol` in this document.
    pub fn defs(&self, symbol: &Symbol) -> &[u32] {
        self.defs.get(symbol).map_or(&[], Vec::as_slice)
    }

    /// Where `name` was last defined before it, else its next definition
    /// (q reads right to left: `(c:key d) like c`).
    pub fn definition(&self, name: &Name) -> Option<u32> {
        let defs = self.defs(&name.symbol);
        let at: u32 = name.range.start().into();
        defs.iter()
            .rev()
            .find(|&&d| d <= at)
            .or_else(|| defs.iter().find(|&&d| d > at))
            .copied()
    }

    /// Every occurrence of `symbol`.
    pub fn occurrences<'a>(&'a self, symbol: &'a Symbol) -> impl Iterator<Item = &'a Name> {
        self.names.iter().filter(move |n| n.symbol == *symbol)
    }

    /// Does the definition at `offset` assign a dictionary or keyed-table
    /// literal (`d:([a:1])`, `` `d set ([k:1] v:2) ``)?
    pub fn holds_dictionary(&self, offset: u32) -> bool {
        self.dictionaries.contains(&offset)
    }

    /// Globals defined in this document, with their definition offsets.
    pub fn globals(&self) -> impl Iterator<Item = (&str, &[u32])> {
        self.defs.iter().filter_map(|(s, d)| match s {
            Symbol::Global(q) => Some((q.as_str(), d.as_slice())),
            Symbol::Local { .. } => None,
        })
    }
}

/// A lambda's local names, known once its whole body is walked.
#[derive(Default)]
struct Lambda {
    start: u32,
    implicit: bool,
    locals: HashSet<SmolStr>,
}

impl Lambda {
    fn has_local(&self, name: &str) -> bool {
        self.locals.contains(name) || (self.implicit && matches!(name, "x" | "y" | "z"))
    }
}

/// A name awaiting resolution: whether it is local depends on the rest of
/// its lambda.
struct Pending {
    token: SyntaxToken,
    /// A backtick symbol: always a global; its range skips the tick.
    symbol: bool,
    role: Role,
    lambda: Option<usize>,
    ns: SmolStr,
}

#[derive(Default)]
struct Builder {
    lambdas: Vec<Lambda>,
    pending: Vec<Pending>,
    /// Assignment targets, by token offset, with their role.
    targets: HashMap<u32, Role>,
    /// Offsets of top-level `name::` targets, which are root globals.
    root: HashSet<u32>,
    /// Offsets of assignment targets whose value is a dictionary literal.
    dictionaries: HashSet<u32>,
}

impl Builder {
    fn run(mut self, root: &SyntaxNode) -> Bindings {
        // Iterative DFS: real q files nest deeper than the default stack.
        enum Step {
            Visit(SyntaxNode),
            Leave,
        }
        let mut work = vec![Step::Visit(root.clone())];
        let mut stack: Vec<usize> = Vec::new();
        let mut ns = SmolStr::default();
        while let Some(step) = work.pop() {
            let node = match step {
                Step::Leave => {
                    stack.pop();
                    continue;
                }
                Step::Visit(node) => node,
            };
            if let Some(new_ns) = namespace_change(&node) {
                ns = new_ns;
            }
            let lambda = stack.last().copied();
            match node.kind() {
                SyntaxKind::Lambda => {
                    stack.push(self.lambda(&node, &ns));
                    work.push(Step::Leave);
                }
                SyntaxKind::BinExpr => self.assignment(&node, lambda),
                SyntaxKind::IdentExpr | SyntaxKind::Namespace => {
                    if let Some(token) = first_token(&node).filter(|t| is_name(&node, t)) {
                        let off = token.text_range().start().into();
                        let role = self.targets.get(&off).copied().unwrap_or(Role::Use);
                        self.push(token, false, role, lambda, &ns);
                    }
                }
                SyntaxKind::LiteralExpr => {
                    if let Some((token, role)) = named_symbol(&node) {
                        if node.parent().is_some_and(|bin| assigns_dictionary(&bin)) {
                            self.dictionaries.insert(token.text_range().start().into());
                        }
                        self.push(token, true, role, lambda, &ns);
                    }
                }
                _ => {}
            }
            let children: Vec<SyntaxNode> = node.children().collect();
            work.extend(children.into_iter().rev().map(Step::Visit));
        }
        self.resolve()
    }

    /// Open a lambda scope, recording its parameters.
    fn lambda(&mut self, node: &SyntaxNode, ns: &SmolStr) -> usize {
        let idx = self.lambdas.len();
        let params = node.children().find(|c| c.kind() == SyntaxKind::ParamList);
        let mut lambda = Lambda {
            start: node.text_range().start().into(),
            implicit: params.is_none(),
            ..Lambda::default()
        };
        for token in params.iter().flat_map(SyntaxNode::children_with_tokens) {
            if let Some(t) = token.into_token().filter(|t| t.kind() == SyntaxKind::Ident) {
                lambda.locals.insert(SmolStr::new(t.text()));
                self.push(t, false, Role::Param, Some(idx), ns);
            }
        }
        self.lambdas.push(lambda);
        idx
    }

    /// Record the targets of an assignment `BinExpr`; plain `name:` in a
    /// lambda makes the name one of its locals.
    fn assignment(&mut self, bin: &SyntaxNode, lambda: Option<usize>) {
        if is_literal_entry(bin) {
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
        let pattern = matches!(lhs.kind(), SyntaxKind::ListExpr | SyntaxKind::ParenExpr);
        // A top-level `name::` declares a root global whatever `\d` says.
        let root = op.kind() == SyntaxKind::ColonColon && lambda.is_none();
        let declares =
            root || (op.kind() == SyntaxKind::Colon && lhs.kind() != SyntaxKind::IndexExpr);
        let role = if declares { Role::Declare } else { Role::Amend };
        let dictionary = assigns_dictionary(bin);
        let targets: Vec<SyntaxToken> = if pattern {
            lhs.children()
                .filter_map(|entry| tokens(&entry).find(is_ident_token))
                .collect()
        } else {
            first_token(&lhs)
                .filter(is_ident_token)
                .into_iter()
                .collect()
        };
        for t in targets {
            let off = t.text_range().start().into();
            if let Some(idx) = lambda.filter(|_| declares && !t.text().contains('.')) {
                self.lambdas[idx].locals.insert(SmolStr::new(t.text()));
            }
            if root {
                self.root.insert(off);
            }
            if dictionary {
                self.dictionaries.insert(off);
            }
            self.targets.insert(off, role);
        }
    }

    fn push(
        &mut self,
        token: SyntaxToken,
        symbol: bool,
        role: Role,
        lambda: Option<usize>,
        ns: &SmolStr,
    ) {
        self.pending.push(Pending {
            token,
            symbol,
            role,
            lambda,
            ns: ns.clone(),
        });
    }

    /// Resolve every pending name now that all lambdas' locals are known.
    fn resolve(self) -> Bindings {
        let mut out = Bindings::default();
        for p in self.pending {
            let skip = u32::from(p.symbol);
            let text = &p.token.text()[skip as usize..];
            let start: u32 = p.token.text_range().start().into();
            let range = TextRange::new((start + skip).into(), p.token.text_range().end());
            let local = p.lambda.filter(|&l| {
                p.role == Role::Param
                    || (!p.symbol && !text.contains('.') && self.lambdas[l].has_local(text))
            });
            let symbol = match local {
                Some(l) => Symbol::Local {
                    lambda: self.lambdas[l].start,
                    name: SmolStr::new(text),
                },
                None if self.root.contains(&start) => Symbol::Global(SmolStr::new(text)),
                None => Symbol::Global(qualify(text, &p.ns)),
            };
            if p.role != Role::Use && self.dictionaries.contains(&start) {
                out.dictionaries.insert(range.start().into());
            }
            if p.role != Role::Use {
                out.defs
                    .entry(symbol.clone())
                    .or_default()
                    .push(range.start().into());
            } else if let Symbol::Local { lambda, name } = &symbol
                && matches!(name.as_str(), "x" | "y" | "z")
            {
                // An implicit parameter is defined by its lambda.
                out.defs.entry(symbol.clone()).or_default().push(*lambda);
            }
            out.names.push(Name {
                range,
                role: p.role,
                symbol,
            });
        }
        for defs in out.defs.values_mut() {
            defs.sort_unstable();
            defs.dedup();
        }
        out.names.sort_by_key(|n| n.range.start());
        out
    }
}

fn is_ident_token(t: &SyntaxToken) -> bool {
    matches!(t.kind(), SyntaxKind::Ident | SyntaxKind::DottedIdent)
}

/// Is `token`, the first token of `node`, a name (not a builtin, qSQL
/// column, or table-constructor column)?
fn is_name(node: &SyntaxNode, token: &SyntaxToken) -> bool {
    is_ident_token(token)
        && !is_builtin(token.text())
        && !in_row_context(node)
        && !node
            .parent()
            .is_some_and(|bin| is_literal_entry(&bin) && bin.first_child().as_ref() == Some(node))
}

/// Is `bin` a column or key `name:value` of a table or dictionary literal
/// (`([k:1] v:2)`, `([a:1])`)? Those names are not variables.
fn is_literal_entry(bin: &SyntaxNode) -> bool {
    let colon = bin
        .children_with_tokens()
        .any(|t| t.kind() == SyntaxKind::Colon);
    let Some(owner) = bin
        .parent()
        .filter(|_| colon)
        .filter(|s| s.kind() == SyntaxKind::ExprStmt)
        .and_then(|s| s.parent())
    else {
        return false;
    };
    match owner.kind() {
        SyntaxKind::TableExpr => true,
        SyntaxKind::ArgList => owner
            .parent()
            .is_some_and(|p| matches!(p.kind(), SyntaxKind::TableKeys | SyntaxKind::DictExpr)),
        _ => false,
    }
}

/// Is `bin`'s right side a dictionary or keyed-table literal?
fn assigns_dictionary(bin: &SyntaxNode) -> bool {
    bin.kind() == SyntaxKind::BinExpr
        && bin.children().nth(1).is_some_and(|v| {
            v.kind() == SyntaxKind::DictExpr
                || (v.kind() == SyntaxKind::TableExpr
                    && v.children().any(|c| c.kind() == SyntaxKind::TableKeys))
        })
}

/// A backtick symbol naming a global, with its role: the table of a qSQL
/// `from` (a use), or the first argument of `set`, `upsert` or `insert`,
/// which assign it (`` `t upsert x ``, `` set[`a;1] ``).
fn named_symbol(lit: &SyntaxNode) -> Option<(SyntaxToken, Role)> {
    // One name: `` `a`b set 3 4 `` defines neither `a` nor `b` in q.
    let token = first_token(lit).filter(|t| {
        t.kind() == SyntaxKind::Symbol && t.text().len() > 1 && !t.text()[1..].contains('`')
    })?;
    let parent = lit.parent()?;
    let verb = match parent.kind() {
        SyntaxKind::FromClause => return Some((token, Role::Use)),
        // `` `t verb x ``
        SyntaxKind::BinExpr if parent.first_child().as_ref() == Some(lit) => parent
            .children_with_tokens()
            .filter_map(SyntaxElement::into_token)
            .find(|t| t.kind() == SyntaxKind::Ident)?,
        // `verb[`t;x]`
        SyntaxKind::ExprStmt => {
            let args = parent.parent().filter(|a| {
                a.kind() == SyntaxKind::ArgList && a.children().next().as_ref() == Some(&parent)
            })?;
            first_token(
                &args
                    .parent()
                    .filter(|c| c.kind() == SyntaxKind::IndexExpr)?,
            )?
        }
        _ => return None,
    };
    matches!(verb.text(), "set" | "upsert" | "insert").then_some((token, Role::Amend))
}

/// The namespace `node` switches to: `\d .ns` / `\d .`, or
/// `system "d .ns"`.
fn namespace_change(node: &SyntaxNode) -> Option<SmolStr> {
    let directive = match node.kind() {
        SyntaxKind::SystemCmdStmt => {
            let t = tokens(node).find(|t| t.kind() == SyntaxKind::SystemCmd)?;
            t.text().strip_prefix("\\d")?.to_string()
        }
        SyntaxKind::ApplyExpr => {
            let mut kids = node.children();
            let f = first_token(&kids.next()?)?;
            let arg = first_token(&kids.next()?)?;
            if f.text() != "system" || arg.kind() != SyntaxKind::String {
                return None;
            }
            let cmd = arg.text().strip_prefix('"')?.strip_suffix('"')?;
            cmd.strip_prefix('d')?.to_string()
        }
        _ => return None,
    };
    match directive.trim() {
        "" | "." => Some(SmolStr::default()),
        ns if ns.starts_with('.') => Some(SmolStr::new(ns)),
        _ => None,
    }
}

/// `name` qualified by namespace `ns`, unless it starts with a dot; a
/// leading `..` names the root (`` `..qs `` is `qs` inside any `\d`).
fn qualify(name: &str, ns: &str) -> SmolStr {
    if let Some(root) = name.strip_prefix("..") {
        SmolStr::new(root)
    } else if ns.is_empty() || name.starts_with('.') {
        SmolStr::new(name)
    } else {
        SmolStr::new(format!("{ns}.{name}"))
    }
}

#[cfg(test)]
mod tests {
    use crate::workspace_index::tests::doc;

    #[test]
    fn namespace_applies_inside_cond_progn() {
        // torq.q defines `cp` inside `$[c; [cp:…]; …]` under `\d .proc`.
        let d = doc("\\d .proc\n$[1b;[cp:{.z.p};cd:{.z.d}];[cp:{.z.P};cd:{.z.D}]];\n\\d .");
        let globals: Vec<_> = d.bindings().globals().map(|(n, _)| n).collect();
        assert!(
            globals.contains(&".proc.cp") && !globals.contains(&"cp"),
            "{globals:?}"
        );
    }
}
