//! Builtin keyword recognition for q.
//!
//! Lifted from tree-sitter-q `builtin_infix_func` (grammar.js:285-314).
//! Keep alphabetised for greppability and binary-search correctness.

const BUILTIN_INFIX: &[&str] = &[
    "and", "asof", "bin", "binr", "cor", "cov", "cross", "div", "dsave", "each", "ema", "except",
    "fby", "ij", "ijf", "in", "insert", "inter", "like", "lj", "ljf", "lsq", "mavg", "mcount",
    "mdev", "mmax", "mmin", "mmu", "mod", "msum", "or", "peach", "pj", "scov", "setenv", "ss",
    "sublist", "sv", "uj", "ujf", "union", "upsert", "vs", "wavg", "within", "wsum", "xasc",
    "xbar", "xcol", "xcols", "xdesc", "xexp", "xkey", "xlog", "xprev", "xrank",
];

/// True if `text` names a builtin q verb that is conventionally used infix.
#[must_use]
pub fn is_builtin_infix(text: &str) -> bool {
    BUILTIN_INFIX.binary_search(&text).is_ok()
}

#[cfg(test)]
mod tests {
    use super::{BUILTIN_INFIX, is_builtin_infix};

    #[test]
    fn table_sorted_and_lookup_works() {
        assert!(BUILTIN_INFIX.is_sorted(), "BUILTIN_INFIX must stay sorted");
        assert!(BUILTIN_INFIX.iter().all(|kw| is_builtin_infix(kw)));
        for s in ["foo", "bar", "select", "from", "where", "by", "x", ""] {
            assert!(!is_builtin_infix(s), "{s} should not be recognised");
        }
    }
}
