#[macro_use] extern crate lazy_static;
extern crate regex;

use regex::Regex;

lazy_static! {
    static ref NUMBER_REGEX: Regex = Regex::new(r"\d+").unwrap();
}

pub fn parse_number_in_middle(string: &str) -> Option<u32> {
    NUMBER_REGEX.find(string)
        .and_then(|m| m.as_str().parse().ok())
}

#[test]
fn test_parse_number_in_middle() {
    assert_eq!(parse_number_in_middle("abc123def"), Some(123));
    assert_eq!(parse_number_in_middle("1234"), Some(1234));
    assert_eq!(parse_number_in_middle("12e56"), Some(12));
    assert_eq!(parse_number_in_middle("???"), None);
    assert_eq!(parse_number_in_middle("99999999999999999999"), None);
}
