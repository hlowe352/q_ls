//! q built-in name registry — single source of truth.
//!
//! Two collections:
//! - [`BUILTINS`]: documented verbs/keywords surfaced in completion and
//!   hover. Tuple entries: `(name, doc)`.
//! - [`EXTRA_RESERVED`]: reserved names from `.Q.res` that aren't in
//!   `BUILTINS`. They are still recognised by [`is_builtin`] (so the
//!   unresolved-reference diagnostic stays quiet) but lack docs.
//!
//! [`is_builtin`] also matches anything starting with a built-in
//! namespace prefix (`.q.`, `.Q.`, `.h.`, `.j.`, `.z.`, `.m.`).
//!
//! Both lists must stay alphabetically sorted; tests enforce it.

/// Documented q verbs and keywords. Sorted by name (binary search).
pub const BUILTINS: &[(&str, &str)] = &[
    ("abs", "Absolute value"),
    ("acos", "Arc cosine"),
    ("aj", "As-of join"),
    ("all", "All true"),
    ("and", "Logical AND / minimum"),
    ("any", "Any true"),
    ("asc", "Ascending sort"),
    ("atan", "Arc tangent"),
    ("attr", "Attributes"),
    ("avg", "Average"),
    ("avgs", "Running averages"),
    ("by", "qSQL group-by clause"),
    ("ceiling", "Round up"),
    ("cols", "Column names"),
    ("cor", "Correlation"),
    ("cos", "Cosine"),
    ("count", "Count elements"),
    ("cov", "Covariance"),
    ("cross", "Cross product"),
    ("csv", "CSV separator"),
    ("cut", "Cut list"),
    ("delete", "qSQL delete statement"),
    ("deltas", "Differences"),
    ("desc", "Descending sort"),
    ("dev", "Standard deviation"),
    ("differ", "Differ from previous"),
    ("distinct", "Unique elements"),
    ("div", "Integer division"),
    ("do", "Do-loop control word"),
    ("each", "Apply to each"),
    ("ej", "Equi-join"),
    ("enlist", "Enlist"),
    ("eval", "Evaluate parse tree"),
    ("except", "Set difference"),
    ("exec", "qSQL exec statement"),
    ("exit", "Exit process"),
    ("exp", "Exponential"),
    ("fby", "Filter by"),
    ("fills", "Forward fill nulls"),
    ("first", "First element"),
    ("fkeys", "Foreign keys"),
    ("flip", "Transpose"),
    ("floor", "Round down"),
    ("from", "qSQL from clause"),
    ("get", "Get variable"),
    ("getenv", "Get env variable"),
    ("group", "Group indices"),
    ("gtime", "Greenwich time"),
    ("hclose", "Close handle"),
    ("hcount", "File size"),
    ("hdel", "Delete file"),
    ("hopen", "Open handle"),
    ("hsym", "File symbol"),
    ("iasc", "Indices ascending"),
    ("idesc", "Indices descending"),
    ("if", "If control word"),
    ("ij", "Inner join"),
    ("in", "Membership"),
    ("insert", "Insert into table"),
    ("inter", "Set intersection"),
    ("inv", "Matrix inverse"),
    ("key", "Keys"),
    ("keys", "Key columns"),
    ("last", "Last element"),
    ("like", "Pattern match"),
    ("lj", "Left join"),
    ("load", "Load script"),
    ("log", "Natural log"),
    ("lower", "Lowercase"),
    ("lsq", "Least squares"),
    ("ltime", "Local time"),
    ("ltrim", "Left trim"),
    ("mavg", "Moving average"),
    ("max", "Maximum"),
    ("maxs", "Running maximums"),
    ("mcount", "Moving count"),
    ("md5", "MD5 hash"),
    ("mdev", "Moving deviation"),
    ("med", "Median"),
    ("meta", "Table metadata"),
    ("min", "Minimum"),
    ("mins", "Running minimums"),
    ("mmax", "Moving maximum"),
    ("mmin", "Moving minimum"),
    ("mmu", "Matrix multiply"),
    ("mod", "Modulo"),
    ("msum", "Moving sum"),
    ("neg", "Negate"),
    ("next", "Next element"),
    ("not", "Logical NOT"),
    ("null", "Is null"),
    ("or", "Logical OR / maximum"),
    ("over", "Reduce"),
    ("parse", "Parse string"),
    ("peach", "Parallel each"),
    ("pj", "Plus join"),
    ("prd", "Product"),
    ("prds", "Running products"),
    ("prev", "Previous element"),
    ("prior", "Apply with prior"),
    ("rand", "Random"),
    ("rank", "Rank"),
    ("ratios", "Ratios"),
    ("raze", "Flatten"),
    ("read0", "Read lines"),
    ("read1", "Read bytes"),
    ("reciprocal", "Reciprocal"),
    ("reval", "Restricted eval"),
    ("reverse", "Reverse"),
    ("rotate", "Rotate"),
    ("rtrim", "Right trim"),
    ("save", "Save to file"),
    ("scan", "Accumulate"),
    ("scov", "Sample covariance"),
    ("sdev", "Sample std dev"),
    ("select", "qSQL select statement"),
    ("set", "Set variable"),
    ("show", "Display"),
    ("signum", "Sign"),
    ("sin", "Sine"),
    ("sqrt", "Square root"),
    ("ss", "String search"),
    ("ssr", "String search replace"),
    ("string", "To string"),
    ("sublist", "Sublist"),
    ("sum", "Sum"),
    ("sums", "Running sums"),
    ("sv", "Scalar from vector"),
    ("svar", "Sample variance"),
    ("system", "System command"),
    ("tables", "List tables"),
    ("tan", "Tangent"),
    ("til", "Range 0..n-1"),
    ("trim", "Trim whitespace"),
    ("type", "Type of value"),
    ("uj", "Union join"),
    ("ungroup", "Ungroup"),
    ("union", "Set union"),
    ("update", "qSQL update statement"),
    ("upper", "Uppercase"),
    ("upsert", "Upsert"),
    ("value", "Value"),
    ("var", "Variance"),
    ("view", "View definition"),
    ("views", "List views"),
    ("vs", "Vector from scalar"),
    ("wavg", "Weighted average"),
    ("where", "Where / indices"),
    ("while", "While-loop control word"),
    ("within", "Within range"),
    ("wj", "Window join"),
    ("wsum", "Weighted sum"),
    ("xasc", "Sort asc by col"),
    ("xbar", "Round to multiple"),
    ("xcol", "Rename columns"),
    ("xcols", "Reorder columns"),
    ("xdesc", "Sort desc by col"),
    ("xexp", "Power"),
    ("xgroup", "Group by"),
    ("xkey", "Set key columns"),
    ("xlog", "Log base x"),
    ("xprev", "Previous by n"),
    ("xrank", "Bucket rank"),
];

/// Reserved names from `.Q.res` that aren't surfaced in completion/hover.
/// Sorted; binary-searched alongside [`BUILTINS`] by [`is_builtin`].
const EXTRA_RESERVED: &[&str] = &[
    "asin", "asof",
    "bin", "binr",
    "dsave",
    "ema",
    "ic", "ij3", "ijf",
    "ljf",
    "rload", "rsave",
    "setenv",
    "ujf",
    "wj1",
    "year",
];

/// Top-level namespace prefixes that are q built-ins. A reference like
/// `.q.foo`, `.Q.dd`, `.z.s` etc is always treated as resolved.
const BUILTIN_NS: &[&str] = &[".q.", ".Q.", ".h.", ".j.", ".z.", ".m."];

/// True if `name` is a q built-in (verb, keyword, reserved word, or
/// member of a built-in namespace).
pub fn is_builtin(name: &str) -> bool {
    if BUILTINS.binary_search_by_key(&name, |(n, _)| n).is_ok() {
        return true;
    }
    if EXTRA_RESERVED.binary_search(&name).is_ok() {
        return true;
    }
    BUILTIN_NS.iter().any(|ns| name.starts_with(ns))
}

/// Doc string for a built-in, if one is registered.
pub fn lookup_doc(name: &str) -> Option<&'static str> {
    let i = BUILTINS.binary_search_by_key(&name, |(n, _)| n).ok()?;
    Some(BUILTINS[i].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_alphabetised() {
        let names: Vec<&&str> = BUILTINS.iter().map(|(n, _)| n).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "BUILTINS must stay alphabetised");
    }

    #[test]
    fn extra_reserved_alphabetised() {
        let mut sorted: Vec<&&str> = EXTRA_RESERVED.iter().collect();
        sorted.sort();
        let actual: Vec<&&str> = EXTRA_RESERVED.iter().collect();
        assert_eq!(actual, sorted, "EXTRA_RESERVED must stay alphabetised");
    }

    #[test]
    fn builtins_and_extra_disjoint() {
        for name in EXTRA_RESERVED {
            assert!(
                BUILTINS.binary_search_by_key(name, |(n, _)| n).is_err(),
                "{name} appears in both BUILTINS and EXTRA_RESERVED"
            );
        }
    }

    #[test]
    fn recognises_documented() {
        for n in ["each", "count", "select", "from", "where", "by", "string",
                  "key", "keys", "cols", "type", "raze", "enlist", "if"] {
            assert!(is_builtin(n), "{n} should be builtin");
        }
    }

    #[test]
    fn recognises_extra_reserved() {
        for n in ["asin", "asof", "bin", "ema", "year"] {
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

    #[test]
    fn lookup_doc_returns_doc() {
        assert_eq!(lookup_doc("count"), Some("Count elements"));
        assert_eq!(lookup_doc("nope_not_a_builtin"), None);
    }
}
