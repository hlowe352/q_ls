use tower_lsp::lsp_types::*;
use q_parser::SyntaxKind;
use crate::document::Document;

pub const Q_BUILTINS: &[(&str, &str)] = &[
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
    ("ceiling", "Round up"),
    ("cols", "Column names"),
    ("cor", "Correlation"),
    ("cos", "Cosine"),
    ("count", "Count elements"),
    ("cov", "Covariance"),
    ("cross", "Cross product"),
    ("csv", "CSV separator"),
    ("cut", "Cut list"),
    ("deltas", "Differences"),
    ("desc", "Descending sort"),
    ("dev", "Standard deviation"),
    ("differ", "Differ from previous"),
    ("distinct", "Unique elements"),
    ("div", "Integer division"),
    ("each", "Apply to each"),
    ("ej", "Equi-join"),
    ("enlist", "Enlist"),
    ("eval", "Evaluate parse tree"),
    ("except", "Set difference"),
    ("exit", "Exit process"),
    ("exp", "Exponential"),
    ("fby", "Filter by"),
    ("fills", "Forward fill nulls"),
    ("first", "First element"),
    ("fkeys", "Foreign keys"),
    ("flip", "Transpose"),
    ("floor", "Round down"),
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
    ("set", "Set variable"),
    ("show", "Display"),
    ("signum", "Sign"),
    ("sin", "Sine"),
    ("sqrt", "Square root"),
    ("ssr", "String search replace"),
    ("ss", "String search"),
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
    ("upper", "Uppercase"),
    ("upsert", "Upsert"),
    ("value", "Value"),
    ("var", "Variance"),
    ("view", "View definition"),
    ("views", "List views"),
    ("vs", "Vector from scalar"),
    ("wavg", "Weighted average"),
    ("where", "Where / indices"),
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

const Q_KEYWORDS: &[&str] = &[
    "select", "exec", "update", "delete", "from", "where", "by", "if", "do", "while",
];

fn get_prefix(text: &str, offset: usize) -> String {
    let before = &text[..offset.min(text.len())];
    let start = before
        .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .map(|i| i + 1)
        .unwrap_or(0);
    before[start..].to_string()
}

pub fn complete(doc: &Document, pos: Position) -> Vec<CompletionItem> {
    let offset = doc.offset_of(pos);
    let prefix = get_prefix(doc.text(), offset);

    let mut items: Vec<CompletionItem> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Built-in functions
    for &(name, detail) in Q_BUILTINS {
        if name.starts_with(prefix.as_str()) {
            items.push(CompletionItem {
                label: name.to_string(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(detail.to_string()),
                ..Default::default()
            });
            seen.insert(name.to_string());
        }
    }

    // Keywords
    for &kw in Q_KEYWORDS {
        if kw.starts_with(prefix.as_str()) && !seen.contains(kw) {
            items.push(CompletionItem {
                label: kw.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                ..Default::default()
            });
            seen.insert(kw.to_string());
        }
    }

    // Document identifiers from the syntax tree
    for element in doc.parse().syntax().descendants_with_tokens() {
        if let Some(token) = element.as_token() {
            let kind = token.kind();
            if kind == SyntaxKind::Ident || kind == SyntaxKind::DottedIdent || kind == SyntaxKind::Namespace {
                let text = token.text().to_string();
                if text.starts_with(prefix.as_str()) && !seen.contains(&text) {
                    seen.insert(text.clone());
                    items.push(CompletionItem {
                        label: text,
                        kind: Some(CompletionItemKind::VARIABLE),
                        ..Default::default()
                    });
                }
            }
        }
    }

    items
}
