//! q built-in name recognition.
//!
//! Used by the unresolved-reference diagnostic to suppress warnings on
//! names that q provides intrinsically (the `.Q.res` reserved set, the
//! verb keywords from `q_parser::grammar::keywords`, qSQL keywords, and
//! members of the well-known `.q` / `.Q` / `.h` / `.j` / `.z` / `.m`
//! namespaces).

/// Reserved names from `.Q.res` plus the qSQL keywords. Sorted alphabetically
/// for binary-search lookup.
const RESERVED: &[&str] = &[
    "abs", "acos", "all", "and", "any", "asc", "asin", "asof", "atan", "attr",
    "avg", "avgs",
    "bin", "binr", "by",
    "ceiling", "cols", "cor", "cos", "count", "cov", "cross", "csv", "cut",
    "delete", "deltas", "desc", "dev", "differ", "distinct", "div", "do",
    "dsave",
    "each", "ej", "ema", "enlist", "eval", "except", "exec", "exit", "exp",
    "fby", "fills", "first", "fkeys", "flip", "floor", "from",
    "get", "getenv", "group", "gtime",
    "hclose", "hcount", "hdel", "hopen", "hsym",
    "iasc", "ic", "idesc", "if", "ij", "ij3", "ijf", "in", "insert", "inter",
    "inv",
    "key", "keys",
    "last", "like", "lj", "ljf", "load", "log", "lower", "lsq", "ltime",
    "ltrim",
    "mavg", "max", "maxs", "mcount", "md5", "mdev", "med", "meta", "min",
    "mins", "mmax", "mmin", "mmu", "mod", "msum",
    "neg", "next", "not", "null",
    "or", "over",
    "parse", "peach", "pj", "prd", "prds", "prev", "prior",
    "rand", "rank", "ratios", "raze", "read0", "read1", "reciprocal", "reval",
    "reverse", "rload", "rotate", "rsave", "rtrim",
    "save", "scan", "scov", "sdev", "select", "set", "setenv", "show",
    "signum", "sin", "sqrt", "ss", "ssr", "string", "sublist", "sum", "sums",
    "sv", "svar", "system",
    "tables", "tan", "til", "trim", "type",
    "uj", "ujf", "ungroup", "union", "update", "upsert",
    "value", "var", "view", "views", "vs",
    "wavg", "where", "while", "within", "wj", "wj1", "wsum",
    "xasc", "xbar", "xcol", "xcols", "xdesc", "xexp", "xgroup", "xkey",
    "xlog", "xprev", "xrank",
    "year",
];

/// Top-level namespace prefixes that are q built-ins. A reference like
/// `.q.foo`, `.Q.dd`, `.z.s` etc is always treated as resolved.
const BUILTIN_NS: &[&str] = &[".q.", ".Q.", ".h.", ".j.", ".z.", ".m."];

/// True if `name` is a q built-in (reserved word or built-in namespace
/// member).
pub fn is_builtin(name: &str) -> bool {
    if RESERVED.binary_search(&name).is_ok() {
        return true;
    }
    BUILTIN_NS.iter().any(|ns| name.starts_with(ns))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_alphabetised() {
        let mut sorted: Vec<&&str> = RESERVED.iter().collect();
        sorted.sort();
        let actual: Vec<&&str> = RESERVED.iter().collect();
        assert_eq!(actual, sorted, "RESERVED list must stay alphabetised");
    }

    #[test]
    fn recognises_reserved() {
        for n in ["each", "count", "select", "from", "where", "by", "string",
                  "key", "keys", "cols", "type", "raze", "enlist", "if"] {
            assert!(is_builtin(n), "{n} should be builtin");
        }
    }

    #[test]
    fn recognises_namespaces() {
        assert!(is_builtin(".q.id"));
        assert!(is_builtin(".Q.dd"));
        assert!(is_builtin(".z.s"));
        assert!(is_builtin(".h.iso8601"));
    }

    #[test]
    fn rejects_user_names() {
        for n in ["foo", "bar", "myFunc", ".app.cfg", ".myns.x", "x", ""] {
            assert!(!is_builtin(n), "{n} should not be builtin");
        }
    }
}
