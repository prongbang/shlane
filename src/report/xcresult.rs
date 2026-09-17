//! Turning an `.xcresult` into JUnit.
//!
//! Xcode 16's `xcrun xcresulttool get test-results tests --format json` returns
//! a tree of nodes: a test plan holds bundles, which hold suites, which hold
//! cases, and a failed case holds its failure messages as children.
//!
//! Everything here is deliberately forgiving. The schema is Apple's and can
//! change between Xcode releases, so a missing field or an unfamiliar node type
//! must not lose the rest of the run. The shapes are checked against fixtures;
//! the round trip against a real Xcode runs in the macOS CI job
//! (`docs/plan/13-testing-and-quality.md`).

use serde::Deserialize;
use std::fmt::Write as _;

#[derive(Debug, Deserialize, Default)]
pub struct TestResults {
    #[serde(rename = "testNodes", default)]
    pub nodes: Vec<Node>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Node {
    #[serde(default)]
    pub name: String,
    #[serde(rename = "nodeType", default)]
    pub node_type: String,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub duration: Option<String>,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub children: Vec<Node>,
}

#[derive(Debug, PartialEq)]
pub struct TestCase {
    pub suite: String,
    pub name: String,
    pub seconds: f64,
    pub outcome: Outcome,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    Failed,
    Skipped,
    /// A test Apple reports as expected to fail, which is not a failure.
    ExpectedFailure,
}

/// `0,01s`, `1.5s`, `12s`, or nothing at all.
///
/// The comma is not a typo: `xcresulttool` formats durations for the machine's
/// locale, so an agent in one region writes `0,01s` where another writes
/// `0.01s`.
pub fn parse_duration(value: &str) -> f64 {
    let cleaned: String = value
        .trim()
        .trim_end_matches('s')
        .replace(',', ".")
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    cleaned.parse().unwrap_or(0.0)
}

fn outcome(result: Option<&str>) -> Outcome {
    match result.unwrap_or("").to_ascii_lowercase().as_str() {
        "failed" => Outcome::Failed,
        "skipped" => Outcome::Skipped,
        "expected failure" => Outcome::ExpectedFailure,
        _ => Outcome::Passed,
    }
}

/// Walk the tree and collect every test case.
pub fn test_cases(results: &TestResults) -> Vec<TestCase> {
    let mut cases = Vec::new();
    for node in &results.nodes {
        collect(node, &mut Vec::new(), &mut cases);
    }
    cases
}

fn collect(node: &Node, path: &mut Vec<String>, cases: &mut Vec<TestCase>) {
    if is_case(&node.node_type) {
        cases.push(TestCase {
            suite: path.last().cloned().unwrap_or_else(|| "tests".to_string()),
            name: node.name.clone(),
            seconds: node.duration.as_deref().map(parse_duration).unwrap_or(0.0),
            outcome: outcome(node.result.as_deref()),
            message: failure_message(node),
        });
        return;
    }

    // Anything that is not a case is a container, and its name is the suite.
    if !node.name.is_empty() {
        path.push(node.name.clone());
    }
    for child in &node.children {
        collect(child, path, cases);
    }
    if !node.name.is_empty() {
        path.pop();
    }
}

fn is_case(node_type: &str) -> bool {
    node_type.eq_ignore_ascii_case("Test Case")
}

/// A failed case carries its reasons as children.
fn failure_message(node: &Node) -> Option<String> {
    let mut lines: Vec<String> = node
        .children
        .iter()
        .filter(|child| {
            let kind = child.node_type.to_ascii_lowercase();
            kind.contains("failure") || kind.contains("error")
        })
        .map(|child| match &child.details {
            Some(details) if !details.is_empty() => format!("{}: {details}", child.name),
            _ => child.name.clone(),
        })
        .collect();

    if lines.is_empty() {
        if let Some(details) = &node.details {
            lines.push(details.clone());
        }
    }

    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// Read what `xcresulttool` printed.
pub fn parse(json: &str) -> Result<TestResults, String> {
    // JSON is valid YAML, so this needs no extra parser.
    serde_yaml::from_str(json).map_err(|err| format!("could not read the test results: {err}"))
}

pub fn to_junit(cases: &[TestCase], suite_name: &str) -> String {
    let failures = cases
        .iter()
        .filter(|case| case.outcome == Outcome::Failed)
        .count();
    let skipped = cases
        .iter()
        .filter(|case| case.outcome == Outcome::Skipped)
        .count();
    let total: f64 = cases.iter().map(|case| case.seconds).sum();

    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        out,
        "<testsuites name=\"{}\" tests=\"{}\" failures=\"{failures}\" skipped=\"{skipped}\" time=\"{total:.3}\">",
        super::escape_xml(suite_name),
        cases.len()
    );

    // One testsuite per suite, in the order they first appear.
    let mut suites: Vec<&str> = Vec::new();
    for case in cases {
        if !suites.contains(&case.suite.as_str()) {
            suites.push(&case.suite);
        }
    }

    for suite in suites {
        let in_suite: Vec<&TestCase> = cases.iter().filter(|case| case.suite == suite).collect();
        let suite_failures = in_suite
            .iter()
            .filter(|case| case.outcome == Outcome::Failed)
            .count();
        let suite_time: f64 = in_suite.iter().map(|case| case.seconds).sum();

        let _ = writeln!(
            out,
            "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{suite_failures}\" time=\"{suite_time:.3}\">",
            super::escape_xml(suite),
            in_suite.len()
        );

        for case in in_suite {
            let _ = write!(
                out,
                "    <testcase classname=\"{}\" name=\"{}\" time=\"{:.3}\"",
                super::escape_xml(&case.suite),
                super::escape_xml(&case.name),
                case.seconds
            );
            match case.outcome {
                Outcome::Passed | Outcome::ExpectedFailure => {
                    let _ = writeln!(out, " />");
                }
                Outcome::Skipped => {
                    let _ = writeln!(out, ">\n      <skipped />\n    </testcase>");
                }
                Outcome::Failed => {
                    let message = case.message.as_deref().unwrap_or("test failed");
                    let _ = writeln!(
                        out,
                        ">\n      <failure message=\"{}\">{}</failure>\n    </testcase>",
                        super::escape_xml(&first_line(message)),
                        super::escape_xml(message)
                    );
                }
            }
        }

        out.push_str("  </testsuite>\n");
    }

    out.push_str("</testsuites>\n");
    out
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or(text).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape Xcode 16's `xcresulttool get test-results tests` produces.
    const SAMPLE: &str = r#"{
      "devices": [{"deviceName": "iPhone 16"}],
      "testNodes": [
        {
          "name": "Counter",
          "nodeType": "Test Plan",
          "result": "Failed",
          "children": [
            {
              "name": "CounterTests",
              "nodeType": "Unit test bundle",
              "result": "Failed",
              "children": [
                {
                  "name": "CounterTests",
                  "nodeType": "Test Suite",
                  "result": "Failed",
                  "children": [
                    {"name": "testStartsAtZero()", "nodeType": "Test Case", "result": "Passed", "duration": "0,01s"},
                    {"name": "testIncrements()", "nodeType": "Test Case", "result": "Passed", "duration": "0.002s"},
                    {"name": "testSkipped()", "nodeType": "Test Case", "result": "Skipped"},
                    {
                      "name": "testDecrementStopsAtZero()",
                      "nodeType": "Test Case",
                      "result": "Failed",
                      "duration": "0.5s",
                      "children": [
                        {
                          "name": "CounterTests.swift:24",
                          "nodeType": "Failure Message",
                          "details": "XCTAssertEqual failed: (\"-1\") is not equal to (\"0\") - a counter should not go negative"
                        }
                      ]
                    }
                  ]
                }
              ]
            }
          ]
        }
      ]
    }"#;

    fn cases() -> Vec<TestCase> {
        test_cases(&parse(SAMPLE).expect("the sample parses"))
    }

    #[test]
    fn reads_every_case_out_of_the_tree() {
        let cases = cases();
        assert_eq!(cases.len(), 4, "{cases:?}");
        assert_eq!(cases[0].name, "testStartsAtZero()");
        assert_eq!(cases[0].suite, "CounterTests");
    }

    #[test]
    fn reads_the_outcomes() {
        let cases = cases();
        assert_eq!(cases[0].outcome, Outcome::Passed);
        assert_eq!(cases[2].outcome, Outcome::Skipped);
        assert_eq!(cases[3].outcome, Outcome::Failed);
    }

    #[test]
    fn reads_durations_in_either_locale() {
        let cases = cases();
        assert!((cases[0].seconds - 0.01).abs() < 1e-9, "{:?}", cases[0]);
        assert!((cases[1].seconds - 0.002).abs() < 1e-9, "{:?}", cases[1]);
        assert_eq!(cases[2].seconds, 0.0, "a missing duration is zero");
    }

    #[test]
    fn parses_durations_defensively() {
        assert_eq!(parse_duration("0,01s"), 0.01);
        assert_eq!(parse_duration("1.5s"), 1.5);
        assert_eq!(parse_duration("12"), 12.0);
        assert_eq!(parse_duration(""), 0.0);
        assert_eq!(parse_duration("unknown"), 0.0);
    }

    #[test]
    fn keeps_the_failure_message() {
        let failed = &cases()[3];
        let message = failed.message.as_deref().expect("a message");
        assert!(message.contains("is not equal to"), "{message}");
        assert!(message.contains("CounterTests.swift:24"), "{message}");
    }

    #[test]
    fn writes_junit_a_ci_can_read() {
        let xml = to_junit(&cases(), "Counter");
        assert!(xml.contains("tests=\"4\""), "{xml}");
        assert!(xml.contains("failures=\"1\""), "{xml}");
        assert!(xml.contains("skipped=\"1\""), "{xml}");
        assert!(xml.contains("classname=\"CounterTests\""), "{xml}");
        assert!(xml.contains("name=\"testStartsAtZero()\""), "{xml}");
        assert!(xml.contains("<skipped />"), "{xml}");
        assert!(xml.contains("<failure message="), "{xml}");
        // The quotes inside the assertion message must not break the XML.
        assert!(
            !xml.contains("(\"-1\")"),
            "unescaped quotes in the XML:\n{xml}"
        );
    }

    #[test]
    fn an_expected_failure_is_not_a_failure() {
        let results = parse(
            r#"{"testNodes":[{"name":"S","nodeType":"Test Suite","children":[
                {"name":"t()","nodeType":"Test Case","result":"Expected Failure"}]}]}"#,
        )
        .expect("parses");
        let cases = test_cases(&results);
        assert_eq!(cases[0].outcome, Outcome::ExpectedFailure);
        assert!(to_junit(&cases, "x").contains("failures=\"0\""));
    }

    #[test]
    fn an_empty_or_unfamiliar_result_does_not_lose_the_rest() {
        assert!(test_cases(&parse("{}").expect("parses")).is_empty());

        // An unfamiliar node type is treated as a container, so its children
        // still arrive: a new Xcode must not empty the report.
        let results = parse(
            r#"{"testNodes":[{"name":"Plan","nodeType":"Something New","children":[
                {"name":"t()","nodeType":"Test Case","result":"Passed"}]}]}"#,
        )
        .expect("parses");
        assert_eq!(test_cases(&results).len(), 1);
    }

    #[test]
    fn something_that_is_not_the_expected_json_is_reported() {
        assert!(parse("not json at all: [").is_err());
    }
}
