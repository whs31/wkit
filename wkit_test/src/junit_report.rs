#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JunitReport {
  suites: Vec<String>,
}

impl JunitReport {
  pub fn add(&mut self, xml: &str) {
    let body = Self::strip_prolog(xml);
    if let Some(rest) = body.strip_prefix("<testsuites") {
      if let Some(open_end) = rest.find('>') {
        if rest[..open_end].ends_with('/') {
          return;
        }
        let inner = &rest[open_end + 1..];
        let inner = inner
          .rfind("</testsuites>")
          .map_or(inner, |end| &inner[..end]);
        self.suites.push(inner.trim().to_owned());
      }
    } else if !body.is_empty() {
      self.suites.push(body.to_owned());
    }
  }

  pub fn failure(name: &str, message: &str) -> String {
    let name = Self::escape(name);
    let message = Self::escape(message);
    format!(
      "<testsuite name=\"{name}\" tests=\"1\" failures=\"1\" errors=\"0\"><testcase name=\"{name}\" classname=\"{name}\"><failure message=\"{message}\"/></testcase></testsuite>"
    )
  }

  pub fn render(&self) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuites>\n");
    for suite in &self.suites {
      out.push_str(suite);
      out.push('\n');
    }
    out.push_str("</testsuites>\n");
    out
  }

  fn strip_prolog(xml: &str) -> &str {
    let mut body = xml.trim_start_matches('\u{feff}').trim();
    loop {
      let skip = if body.starts_with("<?") {
        body.find("?>").map(|end| end + 2)
      } else if body.starts_with("<!--") {
        body.find("-->").map(|end| end + 3)
      } else {
        None
      };
      match skip {
        Some(end) => body = body[end..].trim_start(),
        None => return body.trim(),
      }
    }
  }

  fn escape(text: &str) -> String {
    text
      .replace('&', "&amp;")
      .replace('<', "&lt;")
      .replace('>', "&gt;")
      .replace('"', "&quot;")
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn merges_suites_from_every_runner() {
    let mut report = JunitReport::default();
    report.add("<?xml version=\"1.0\"?>\n<testsuites name=\"nextest\" tests=\"2\">\n  <testsuite name=\"a\"/>\n  <testsuite name=\"b\"/>\n</testsuites>\n");
    report.add("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- ctest -->\n<testsuite name=\"ctest\" tests=\"1\"><testcase name=\"t\"/></testsuite>");
    report.add("<testsuites/>");
    report.add(&JunitReport::failure("unit", "exited with \"code\" 2"));
    let xml = report.render();
    assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuites>\n"));
    assert_eq!(xml.matches("<testsuites").count(), 1);
    assert_eq!(xml.matches("<testsuite ").count(), 4);
    assert!(xml.contains("message=\"exited with &quot;code&quot; 2\""));
    assert!(xml.ends_with("</testsuites>\n"));
  }
}
