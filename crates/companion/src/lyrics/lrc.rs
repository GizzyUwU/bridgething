use libbridgething::LyricLine;

pub fn parse(text: &str) -> Vec<LyricLine> {
  let mut out = Vec::new();

  for raw in text.split('\n') {
    let (stamps, body) = split_timestamps(raw);
    for start_ms in stamps {
      out.push(LyricLine {
        start_ms,
        text: body.to_string(),
      });
    }
  }

  out.sort_by_key(|line| line.start_ms);
  out
}

fn split_timestamps(line: &str) -> (Vec<u32>, &str) {
  let mut stamps = Vec::new();
  let mut rest = line;

  while let Some(after_open) = rest.strip_prefix('[') {
    let Some(close) = after_open.find(']') else { break };
    let Some(ms) = parse_timestamp(&after_open[..close]) else {
      break;
    };
    stamps.push(ms);
    rest = &after_open[close + 1..];
  }

  (stamps, rest.trim())
}

fn parse_timestamp(s: &str) -> Option<u32> {
  let mut halves = s.split(':');
  let minutes = halves.next()?.parse::<u32>().ok()?;
  let rest = halves.next()?;
  if halves.next().is_some() {
    return None;
  }

  let mut parts = rest.split('.');
  let seconds = parts.next()?.parse::<u32>().ok()?;
  let hundredths = match (parts.next(), parts.next()) {
    (Some(frac), None) => hundredths(frac),
    _ => 0,
  };

  let ms = (u64::from(minutes) * 60 + u64::from(seconds)) * 1000 + u64::from(hundredths) * 10;
  u32::try_from(ms).ok()
}

fn hundredths(frac: &str) -> u32 {
  let mut chars = frac.chars();
  let tens = chars.next().unwrap_or('0');
  let ones = chars.next().unwrap_or('0');

  match (tens.to_digit(10), ones.to_digit(10)) {
    (Some(tens), Some(ones)) => tens * 10 + ones,
    _ => 0,
  }
}
