use bridgething_companion::lyrics::lrc;
use libbridgething::LyricLine;

fn line(start_ms: u32, text: &str) -> LyricLine {
  LyricLine {
    start_ms,
    text: text.to_string(),
  }
}

#[test]
fn parses_single_timestamp_lines() {
  let lines = lrc::parse("[00:12.50]Line one\n[00:17.00]Line two\n[01:30.50]Line three");

  assert_eq!(
    lines,
    vec![
      line(12500, "Line one"),
      line(17000, "Line two"),
      line(90500, "Line three")
    ]
  );
}

#[test]
fn expands_multiple_timestamps_on_one_line() {
  let lines = lrc::parse("[00:12.00][01:30.00]Repeated chorus");

  assert_eq!(
    lines,
    vec![line(12000, "Repeated chorus"), line(90000, "Repeated chorus")]
  );
}

#[test]
fn skips_lines_without_timestamps() {
  let lines = lrc::parse("[ti: Title]\n[ar: Artist]\n[00:12.50]Line one");

  assert_eq!(lines, vec![line(12500, "Line one")]);
}

#[test]
fn accepts_three_digit_fractional_seconds() {
  let lines = lrc::parse("[00:12.500]Line");

  assert_eq!(lines, vec![line(12500, "Line")]);
}

#[test]
fn emits_sorted_by_timestamp() {
  let lines = lrc::parse("[01:30.00]B\n[00:12.50]A");

  assert_eq!(lines, vec![line(12500, "A"), line(90000, "B")]);
}

#[test]
fn stops_consuming_brackets_at_the_first_non_timestamp() {
  let lines = lrc::parse("[00:12.00][ti: Title]Line one");

  assert_eq!(lines, vec![line(12000, "[ti: Title]Line one")]);
}

#[test]
fn trims_spaces_around_the_body() {
  let lines = lrc::parse("[00:12.00]   Line one   ");

  assert_eq!(lines, vec![line(12000, "Line one")]);
}

#[test]
fn keeps_a_timestamp_whose_body_is_empty() {
  let lines = lrc::parse("[00:12.00]");

  assert_eq!(lines, vec![line(12000, "")]);
}

#[test]
fn pads_a_one_digit_fraction_to_hundredths() {
  let lines = lrc::parse("[00:12.5]Line");

  assert_eq!(lines, vec![line(12500, "Line")]);
}

#[test]
fn truncates_fractional_digits_past_hundredths() {
  let lines = lrc::parse("[00:12.5009]Line");

  assert_eq!(lines, vec![line(12500, "Line")]);
}

#[test]
fn treats_an_empty_fraction_as_zero() {
  let lines = lrc::parse("[00:12.]Line");

  assert_eq!(lines, vec![line(12000, "Line")]);
}

#[test]
fn drops_a_line_whose_bracket_never_closes() {
  let lines = lrc::parse("[00:12.00]Line one\n[00:17.00");

  assert_eq!(lines, vec![line(12000, "Line one")]);
}

#[test]
fn accepts_minutes_past_an_hour() {
  let lines = lrc::parse("[90:00.00]Line");

  assert_eq!(lines, vec![line(5400000, "Line")]);
}

#[test]
fn strips_the_carriage_return_from_crlf_input() {
  let lines = lrc::parse("[00:12.00]Line one\r\n[00:17.00]Line two\r\n");

  assert_eq!(lines, vec![line(12000, "Line one"), line(17000, "Line two")]);
}

#[test]
fn drops_a_timestamp_that_is_negative_or_out_of_range() {
  assert_eq!(lrc::parse("[-1:30.00]Line"), Vec::<LyricLine>::new());
  assert_eq!(lrc::parse("[00:-5.00]Line"), Vec::<LyricLine>::new());
  assert_eq!(lrc::parse("[4000000:00.00]Line"), Vec::<LyricLine>::new());
}

#[test]
fn treats_a_signed_fraction_as_zero() {
  assert_eq!(lrc::parse("[00:12.-1]Line"), vec![line(12000, "Line")]);
  assert_eq!(lrc::parse("[00:12.+1]Line"), vec![line(12000, "Line")]);
}

#[test]
fn drops_a_timestamp_whose_seconds_field_is_only_a_dot() {
  assert_eq!(lrc::parse("[00:.]Line"), Vec::<LyricLine>::new());
  assert_eq!(lrc::parse("[00:..]Line"), Vec::<LyricLine>::new());
}

#[test]
fn returns_nothing_for_empty_input() {
  assert_eq!(lrc::parse(""), Vec::<LyricLine>::new());
}
