use std::collections::HashMap;

use anyhow::{bail, ensure};
use chrono::{DateTime, NaiveDate, Utc};
use chumsky::prelude::*;

use crate::feed::FeedItem;

#[derive(Debug)]
pub(crate) struct DateFilter {
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum GroupKey {
    Date,
    Week,
    Feed,
}

impl GroupKey {
    pub(crate) fn extract(&self, item: &FeedItem, feed_labels: &HashMap<String, String>) -> String {
        match self {
            GroupKey::Date => item
                .date
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            GroupKey::Week => item
                .date
                .map(|d| d.format("%G-W%V").to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            GroupKey::Feed => feed_labels
                .get(&item.feed)
                .cloned()
                .unwrap_or_else(|| item.feed.clone()),
        }
    }

    pub(crate) fn compare(
        &self,
        a: &FeedItem,
        b: &FeedItem,
        feed_labels: &HashMap<String, String>,
    ) -> std::cmp::Ordering {
        match self {
            GroupKey::Date | GroupKey::Week => self
                .extract(b, feed_labels)
                .cmp(&self.extract(a, feed_labels)),
            GroupKey::Feed => self
                .extract(a, feed_labels)
                .cmp(&self.extract(b, feed_labels)),
        }
    }
}

#[derive(Debug)]
pub(crate) struct ShowQuery {
    pub keys: Vec<GroupKey>,
    pub filter: Option<String>,
    pub date_filter: DateFilter,
}

enum Token {
    Group(GroupKey),
    FeedFilter(String),
    Since(DateTime<Utc>),
    Until(DateTime<Utc>),
    Range(Option<DateTime<Utc>>, Option<DateTime<Utc>>),
}

fn date_value_core<'a>() -> impl Parser<'a, &'a str, DateTime<Utc>, extra::Err<Rich<'a, char>>> {
    let named = choice((just("today").to("today"), just("yesterday").to("yesterday")));

    let digits = text::digits(10).to_slice();

    let relative = digits.then(choice((
        just("months").to("months"),
        just("m").to("months"),
        just("w").to("w"),
        just("d").to("d"),
    )));

    let absolute = text::digits(10)
        .to_slice()
        .then_ignore(just('-'))
        .then(text::digits(10).to_slice())
        .then_ignore(just('-'))
        .then(text::digits(10).to_slice());

    choice((
        named.map(|name| match name {
            "today" => Utc::now()
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc(),
            "yesterday" => (Utc::now() - chrono::Duration::days(1))
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc(),
            _ => unreachable!(),
        }),
        absolute.try_map(|((year, month), day), span| {
            let s = format!("{year}-{month}-{day}");
            NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc())
                .map_err(|_| Rich::custom(span, format!("Invalid date: {s}")))
        }),
        relative.try_map(|(num_str, unit): (&str, &str), span| {
            let n: i64 = num_str.parse().map_err(|_| {
                Rich::custom(span, format!("Invalid number in duration: {num_str}"))
            })?;
            let duration = match unit {
                "d" => chrono::Duration::days(n),
                "w" => chrono::Duration::weeks(n),
                "months" => chrono::Duration::days(n * 30),
                _ => unreachable!(),
            };
            Ok(Utc::now() - duration)
        }),
    ))
    .labelled("date value (e.g. 2024-01-15, 3d, 2w, 1m, today, yesterday)")
}

fn date_value_parser<'a>() -> impl Parser<'a, &'a str, DateTime<Utc>, extra::Err<Rich<'a, char>>> {
    date_value_core().then_ignore(end().labelled("end of date value"))
}

fn arg_parser<'a>() -> impl Parser<'a, &'a str, Token, extra::Err<Rich<'a, char>>> {
    let group = just('/')
        .ignore_then(one_of("dwf").labelled("grouping: /d (date), /w (week), or /f (feed)"))
        .then_ignore(end().labelled("end of grouping argument"))
        .map(|c| {
            Token::Group(match c {
                'd' => GroupKey::Date,
                'w' => GroupKey::Week,
                'f' => GroupKey::Feed,
                _ => unreachable!(),
            })
        });

    let feed_filter = just('@')
        .ignore_then(any().repeated().at_least(1).collect::<String>())
        .then_ignore(end().labelled("end of feed filter"))
        .map(|s| Token::FeedFilter(format!("@{s}")));

    let since = just("since:")
        .ignore_then(date_value_parser())
        .map(Token::Since);

    let until = just("until:")
        .ignore_then(date_value_parser())
        .map(Token::Until);

    let range = choice((
        date_value_core()
            .then_ignore(just(".."))
            .then(date_value_core().or_not())
            .then_ignore(end().labelled("end of date range"))
            .map(|(from, to)| Token::Range(Some(from), to)),
        just("..")
            .ignore_then(date_value_core())
            .then_ignore(end().labelled("end of date range"))
            .map(|to| Token::Range(None, Some(to))),
    ));

    choice((since, until, range, group, feed_filter))
        .labelled("argument (since:, until:, 3d..1d, /d, /w, /f, or @feed)")
}

pub(crate) fn parse_show_args(args: &[String]) -> anyhow::Result<ShowQuery> {
    let mut keys = Vec::new();
    let mut filter = None;
    let mut since = None;
    let mut until = None;

    let parser = arg_parser();

    for arg in args {
        match parser.parse(arg).into_result() {
            Ok(token) => match token {
                Token::Group(key) => {
                    ensure!(keys.len() < 2, "Too many grouping arguments (max 2).");
                    keys.push(key);
                }
                Token::FeedFilter(s) => {
                    filter = Some(s);
                }
                Token::Since(dt) => {
                    since = Some(dt);
                }
                Token::Until(dt) => {
                    until = Some(dt);
                }
                Token::Range(s, u) => {
                    if let Some(s) = s {
                        since = Some(s);
                    }
                    if let Some(u) = u {
                        until = Some(u);
                    }
                }
            },
            Err(errs) => {
                let messages: Vec<String> = errs.into_iter().map(|e| e.to_string()).collect();
                bail!(
                    "Failed to parse argument '{arg}': {}\nRun 'blog --help' for query syntax.",
                    messages.join(", ")
                );
            }
        }
    }

    Ok(ShowQuery {
        keys,
        filter,
        date_filter: DateFilter { since, until },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }

    #[rstest]
    #[case::date("/d", GroupKey::Date)]
    #[case::week("/w", GroupKey::Week)]
    #[case::feed("/f", GroupKey::Feed)]
    fn test_parse_group_arg(#[case] input: &str, #[case] expected: GroupKey) {
        let q = parse_show_args(&args(&[input])).unwrap();
        assert_eq!(q.keys, vec![expected]);
    }

    #[test]
    fn test_parse_group_arg_invalid() {
        let result = parse_show_args(&args(&["/x"]));
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Failed to parse argument"), "got: {msg}");
    }

    #[test]
    fn test_too_many_groups() {
        let result = parse_show_args(&args(&["/d", "/f", "/w"]));
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Too many grouping arguments"), "got: {msg}");
    }

    #[test]
    fn test_unknown_argument() {
        let result = parse_show_args(&args(&["d"]));
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Failed to parse argument"), "got: {msg}");
    }

    #[test]
    fn test_feed_filter() {
        let q = parse_show_args(&args(&["@myblog"])).unwrap();
        assert_eq!(q.filter, Some("@myblog".to_string()));
    }

    fn parse_date(value: &str) -> anyhow::Result<DateTime<Utc>> {
        let parser = date_value_parser();
        parser
            .parse(value)
            .into_result()
            .map_err(|e| anyhow::anyhow!("Parse error: {:?}", e))
    }

    #[test]
    fn test_parse_date_value_absolute_date() {
        let dt = parse_date("2024-01-15").unwrap();
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "2024-01-15");
    }

    #[test]
    fn test_parse_date_value_another_absolute_date() {
        let dt = parse_date("2024-04-01").unwrap();
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "2024-04-01");
    }

    #[rstest]
    #[case::one_week("1w", 6, 8)]
    #[case::two_days("2d", 1, 3)]
    #[case::three_months("3months", 85, 95)]
    #[case::three_months_short("3m", 85, 95)]
    fn test_parse_date_relative(#[case] input: &str, #[case] min_days: i64, #[case] max_days: i64) {
        let dt = parse_date(input).unwrap();
        let diff_days = (Utc::now() - dt).num_days();
        assert!(
            (min_days..=max_days).contains(&diff_days),
            "{input} should be ~{min_days}-{max_days} days ago, got {diff_days} days"
        );
    }

    #[test]
    fn test_parse_date_value_yesterday() {
        let dt = parse_date("yesterday").unwrap();
        let expected = (Utc::now() - chrono::Duration::days(1))
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        assert_eq!(
            dt.format("%Y-%m-%d").to_string(),
            expected.format("%Y-%m-%d").to_string()
        );
    }

    #[test]
    fn test_parse_date_value_today() {
        let dt = parse_date("today").unwrap();
        let expected = Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        assert_eq!(
            dt.format("%Y-%m-%d").to_string(),
            expected.format("%Y-%m-%d").to_string()
        );
    }

    #[test]
    fn test_parse_date_value_nonsense_returns_error() {
        assert!(parse_date("nonsense").is_err());
    }

    #[test]
    fn test_since_arg() {
        let q = parse_show_args(&args(&["since:2024-01-15"])).unwrap();
        let dt = q.date_filter.since.unwrap();
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "2024-01-15");
    }

    #[test]
    fn test_until_arg() {
        let q = parse_show_args(&args(&["until:2024-06-01"])).unwrap();
        let dt = q.date_filter.until.unwrap();
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "2024-06-01");
    }

    #[test]
    fn test_combined_args() {
        let q = parse_show_args(&args(&["/d", "@blog", "since:2024-01-15"])).unwrap();
        assert_eq!(q.keys, vec![GroupKey::Date]);
        assert_eq!(q.filter, Some("@blog".to_string()));
        assert!(q.date_filter.since.is_some());
    }

    #[test]
    fn test_range_both() {
        let q = parse_show_args(&args(&["2024-01-15..2024-02-01"])).unwrap();
        assert_eq!(
            q.date_filter.since.unwrap().format("%Y-%m-%d").to_string(),
            "2024-01-15"
        );
        assert_eq!(
            q.date_filter.until.unwrap().format("%Y-%m-%d").to_string(),
            "2024-02-01"
        );
    }

    #[test]
    fn test_range_open_end() {
        let q = parse_show_args(&args(&["2024-01-15.."])).unwrap();
        assert_eq!(
            q.date_filter.since.unwrap().format("%Y-%m-%d").to_string(),
            "2024-01-15"
        );
        assert!(q.date_filter.until.is_none());
    }

    #[test]
    fn test_range_open_start() {
        let q = parse_show_args(&args(&["..2024-02-01"])).unwrap();
        assert!(q.date_filter.since.is_none());
        assert_eq!(
            q.date_filter.until.unwrap().format("%Y-%m-%d").to_string(),
            "2024-02-01"
        );
    }

    #[test]
    fn test_range_relative() {
        let q = parse_show_args(&args(&["3w..1w"])).unwrap();
        assert!(q.date_filter.since.is_some());
        assert!(q.date_filter.until.is_some());
        assert!(q.date_filter.since.unwrap() < q.date_filter.until.unwrap());
    }
}
