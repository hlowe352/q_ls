use q_parser::parse;

#[test]
fn parse_basic_fixture_lossless() {
    let source = include_str!("../../../test_data/basic.q");
    let result = parse(source);
    assert_eq!(result.syntax().text().to_string(), source);
}

#[test]
fn parse_qsql_fixture_lossless() {
    let source = include_str!("../../../test_data/qsql.q");
    let result = parse(source);
    assert_eq!(result.syntax().text().to_string(), source);
}

#[test]
fn parse_errors_no_panic() {
    let source = include_str!("../../../test_data/errors.q");
    let result = parse(source);
    assert!(!result.errors.is_empty());
    assert_eq!(result.syntax().text().to_string(), source);
}
