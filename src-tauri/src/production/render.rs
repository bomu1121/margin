// Headless rendering for booklet export.
//
// No CDP and no extra runtime: the built HTML carries a tiny measurement
// script that writes a JSON blob into the DOM; `msedge --dump-dom` returns the
// DOM with that blob, and a second pass prints the PDF. Edge/Chrome discovery
// mirrors the ffmpeg approach (PATH -> well-known paths -> env override).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderResult {
    pub metrics: Value,
    pub html_path: String,
    pub pdf_path: String,
    pub png_path: String,
    pub pages: i64,
    pub pdf_bytes: i64,
    pub elapsed_ms: i64,
    pub browser: String,
}

const MEASURE_SCRIPT: &str = r#"
<script>
(function () {
  function measure() {
    try {
      var doc = document.querySelector('.doc') || document.body;
      var d = doc.getBoundingClientRect();
      var all = [].slice.call(doc.querySelectorAll('*')).filter(function (el) {
        return el.namespaceURI === 'http://www.w3.org/1999/xhtml';
      });
      function label(el) { return (el.className && String(el.className).slice(0, 24)) || el.tagName; }
      var overflow = all.filter(function (el) {
        return el.scrollWidth > el.clientWidth + 2
          && ['auto', 'scroll'].indexOf(getComputedStyle(el).overflowX) === -1;
      }).map(label);
      var oob = all.filter(function (el) {
        var r = el.getBoundingClientRect();
        return r.width > 0 && (r.right > d.right + 1 || r.left < d.left - 1);
      }).map(label);
      var under12 = all.filter(function (el) {
        var hasText = [].slice.call(el.childNodes).some(function (n) {
          return n.nodeType === 3 && n.textContent.trim();
        });
        return hasText && el.getBoundingClientRect().width > 0
          && parseFloat(getComputedStyle(el).fontSize) < 12;
      }).map(function (el) { return label(el) + ':' + getComputedStyle(el).fontSize; });
      var clipped = 0;
      var emptyBoxes = [];
      var labelOverlaps = [];
      var textOverflow = [];
      var clippedFigures = [];
      [].slice.call(doc.querySelectorAll('.fig')).forEach(function (fig, fi) {
        var svg = fig.querySelector('svg');
        if (!svg) return;
        var title = ((fig.querySelector('.fig-title') || {}).textContent || '').slice(0, 30);
        var texts = [].slice.call(svg.querySelectorAll('text'));
        var boxes = [].slice.call(svg.querySelectorAll('rect.box, rect.chip'));
        var vb = svg.viewBox.baseVal;
        var clippedTexts = [];
        texts.forEach(function (t) {
          var b = t.getBBox();
          if (b.x < -2 || b.y < -2 || b.x + b.width > vb.width + 1 || b.y + b.height > vb.height + 1) {
            clipped += 1;
            clippedTexts.push(t.textContent.slice(0, 14));
          }
        });
        if (clippedTexts.length) clippedFigures.push({ fig: fi, title: title, texts: clippedTexts });
        boxes.forEach(function (rect) {
          var rb = rect.getBBox();
          var inner = texts.some(function (t) {
            var b = t.getBBox();
            return b.x >= rb.x - 1 && b.y >= rb.y - 1
              && b.x + b.width <= rb.x + rb.width + 1
              && b.y + b.height <= rb.y + rb.height + 1;
          });
          if (!inner) emptyBoxes.push({ fig: fi, title: title, x: Math.round(rb.x), y: Math.round(rb.y) });
        });
        for (var i = 0; i < texts.length; i += 1) {
          for (var j = i + 1; j < texts.length; j += 1) {
            var a = texts[i].getBBox();
            var b = texts[j].getBBox();
            var ox = Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x);
            var oy = Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y);
            if (ox > 2 && oy > 3) {
              labelOverlaps.push({ fig: fi, title: title, a: texts[i].textContent.slice(0, 12), b: texts[j].textContent.slice(0, 12) });
            }
          }
        }
        texts.forEach(function (t) {
          var tb = t.getBBox();
          var cx = tb.x + tb.width / 2;
          var cy = tb.y + tb.height / 2;
          var host = boxes.filter(function (rect) {
            var rb = rect.getBBox();
            return cx >= rb.x && cx <= rb.x + rb.width && cy >= rb.y && cy <= rb.y + rb.height;
          })[0];
          if (!host) return;
          var hb = host.getBBox();
          if (tb.x < hb.x - 2 || tb.x + tb.width > hb.x + hb.width + 2) {
            textOverflow.push({ fig: fi, title: title, text: t.textContent.slice(0, 14) });
          }
        });
      });
      var metrics = {
        height: doc.scrollHeight,
        width: Math.round(d.width),
        sections: doc.querySelectorAll('.sec').length,
        figures: doc.querySelectorAll('.fig').length,
        pages: doc.querySelectorAll('.page').length,
        pageOffsets: [].slice.call(doc.querySelectorAll('.page')).map(function (el) {
          return Math.round(el.getBoundingClientRect().top + window.scrollY);
        }),
        overflow: overflow,
        outOfBounds: oob,
        under12: under12,
        svgClipped: clipped,
        clippedFigures: clippedFigures,
        emptyBoxes: emptyBoxes,
        labelOverlaps: labelOverlaps,
        textOverflow: textOverflow,
        pageOverflow: [].slice.call(doc.querySelectorAll('.page')).map(function (el, pi) {
          return el.scrollHeight > el.clientHeight + 2 ? pi : -1;
        }).filter(function (value) { return value >= 0; }),
        title: (document.querySelector('.book-cover h1') || {}).textContent || ''
      };
      var tag = document.createElement('script');
      tag.type = 'application/json';
      tag.id = 'yb-metrics';
      tag.textContent = JSON.stringify(metrics);
      (document.body || document.documentElement).appendChild(tag);
    } catch (error) {
      var tag = document.createElement('script');
      tag.type = 'application/json';
      tag.id = 'yb-metrics';
      tag.textContent = JSON.stringify({ error: String(error) });
      (document.body || document.documentElement).appendChild(tag);
    }
  }
  if (document.readyState === 'complete') setTimeout(measure, 600);
  else window.addEventListener('load', function () { setTimeout(measure, 600); });
})();
</script>
"#;

fn candidate_browsers() -> Vec<PathBuf> {
    let mut items = Vec::new();
    if let Ok(value) = std::env::var("BOOK_BROWSER") {
        if !value.trim().is_empty() {
            items.push(PathBuf::from(value));
        }
    }
    for base in [
        "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
        "C:/Program Files/Microsoft/Edge/Application/msedge.exe",
        "C:/Program Files/Google/Chrome/Application/chrome.exe",
        "C:/Program Files (x86)/Google/Chrome/Application/chrome.exe",
    ] {
        items.push(PathBuf::from(base));
    }
    items
}

pub fn find_browser() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            for name in ["msedge.exe", "chrome.exe"] {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    candidate_browsers().into_iter().find(|path| path.is_file())
}

async fn run_browser(browser: &Path, args: &[String], timeout: Duration) -> Result<String, String> {
    let mut command = Command::new(browser);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let child = command.spawn().map_err(|error| format!("无法启动浏览器：{error}"))?;
    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| format!("浏览器超时（{}s）", timeout.as_secs()))?
        .map_err(|error| format!("浏览器执行失败：{error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("浏览器退出码 {:?}：{}", output.status.code(), stderr.chars().take(300).collect::<String>()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn extract_metrics(dom: &str) -> Option<Value> {
    // Do not depend on attribute order: browsers serialize <script> attributes
    // differently. Find the marker id, walk back to its opening tag, read the
    // body up to </script>.
    let marker = dom.find("id=\"yb-metrics\"")?;
    let open_start = dom[..marker].rfind("<script")?;
    let open_end = dom[open_start..].find('>')? + open_start;
    let close = dom[open_end..].find("</script>")? + open_end;
    let text = dom[open_end + 1..close].trim();
    serde_json::from_str(text).ok()
}

fn count_pdf_pages(path: &Path) -> i64 {
    let Ok(bytes) = std::fs::read(path) else { return 0 };
    let text = String::from_utf8_lossy(&bytes);
    let mut count = 0;
    for (index, _) in text.match_indices("/Type") {
        let tail: String = text[index..].chars().take(24).collect();
        if tail.starts_with("/Type/Page") || tail.starts_with("/Type /Page") {
            if !tail.starts_with("/Type/Pages") && !tail.starts_with("/Type /Pages") {
                count += 1;
            }
        }
    }
    count
}

/// Render one standalone HTML document into metrics + PDF + screenshot.
pub async fn render_book(html: &str, out_dir: &Path, stem: &str) -> Result<RenderResult, String> {
    let browser = find_browser().ok_or_else(|| "没有找到 Edge 或 Chrome，无法渲染".to_string())?;
    std::fs::create_dir_all(out_dir).map_err(|error| error.to_string())?;
    let started = std::time::Instant::now();

    let injected = if html.contains("</body>") {
        html.replacen("</body>", &format!("{MEASURE_SCRIPT}</body>"), 1)
    } else {
        format!("{html}{MEASURE_SCRIPT}")
    };
    let html_path = out_dir.join(format!("{stem}.html"));
    std::fs::write(&html_path, injected.as_bytes()).map_err(|error| error.to_string())?;
    let url = format!("file:///{}", html_path.to_string_lossy().replace('\\', "/"));

    let profile = out_dir.join("browser-profile");
    std::fs::create_dir_all(&profile).map_err(|error| error.to_string())?;
    let profile_arg = format!("--user-data-dir={}", profile.to_string_lossy());

    let dom = run_browser(
        &browser,
        &[
            "--headless=new".into(),
            "--disable-gpu".into(),
            "--allow-file-access-from-files".into(),
            "--virtual-time-budget=4000".into(),
            profile_arg.clone(),
            "--window-size=1440,1100".into(),
            "--dump-dom".into(),
            url.clone(),
        ],
        Duration::from_secs(60),
    )
    .await?;
    let metrics = extract_metrics(&dom).ok_or_else(|| "渲染完成但没有拿到测量结果".to_string())?;
    if let Some(error) = metrics.get("error").and_then(Value::as_str) {
        return Err(format!("页内测量失败：{error}"));
    }

    let pdf_path = out_dir.join(format!("{stem}.pdf"));
    run_browser(
        &browser,
        &[
            "--headless=new".into(),
            "--disable-gpu".into(),
            "--allow-file-access-from-files".into(),
            "--no-pdf-header-footer".into(),
            "--print-to-pdf-no-header".into(),
            "--virtual-time-budget=5000".into(),
            profile_arg.clone(),
            format!("--print-to-pdf={}", pdf_path.to_string_lossy()),
            url.clone(),
        ],
        Duration::from_secs(90),
    )
    .await?;
    let pages = count_pdf_pages(&pdf_path);
    let pdf_bytes = std::fs::metadata(&pdf_path).map(|meta| meta.len() as i64).unwrap_or(0);

    let height = metrics
        .get("height")
        .and_then(Value::as_i64)
        .unwrap_or(1600)
        .clamp(800, 20000);
    let png_path = out_dir.join(format!("{stem}.png"));
    let _ = run_browser(
        &browser,
        &[
            "--headless=new".into(),
            "--disable-gpu".into(),
            "--hide-scrollbars".into(),
            "--allow-file-access-from-files".into(),
            "--virtual-time-budget=3000".into(),
            profile_arg,
            format!("--window-size=1440,{height}"),
            format!("--screenshot={}", png_path.to_string_lossy()),
            url,
        ],
        Duration::from_secs(60),
    )
    .await;

    Ok(RenderResult {
        metrics,
        html_path: html_path.to_string_lossy().into_owned(),
        pdf_path: pdf_path.to_string_lossy().into_owned(),
        png_path: png_path.to_string_lossy().into_owned(),
        pages,
        pdf_bytes,
        elapsed_ms: started.elapsed().as_millis() as i64,
        browser: browser.to_string_lossy().into_owned(),
    })
}

/// Gate used by the booklet workflow later: any layout violation fails.
pub fn gate_failed(metrics: &Value) -> bool {
    let count = |key: &str| metrics.get(key).and_then(Value::as_array).map(|items| items.len()).unwrap_or(0);
    count("overflow") > 0 || count("outOfBounds") > 0 || count("under12") > 0
        || metrics.get("svgClipped").and_then(Value::as_i64).unwrap_or(0) > 0
        || count("emptyBoxes") > 0 || count("labelOverlaps") > 0 || count("textOverflow") > 0
}

#[allow(dead_code)]
pub fn summary(metrics: &Value) -> Value {
    json!({
        "figures": metrics.get("figures"),
        "sections": metrics.get("sections"),
        "height": metrics.get("height"),
        "failed": gate_failed(metrics),
    })
}

/// Render viewport slices for the vision reviewer. Each slice is a separate
/// 1440x1200 screenshot taken after scrolling the built HTML to an offset.
pub async fn render_slices(
    html: &str,
    out_dir: &Path,
    stem: &str,
    offsets: &[i64],
) -> Result<Vec<String>, String> {
    let browser = find_browser().ok_or_else(|| "没有找到 Edge 或 Chrome，无法渲染".to_string())?;
    std::fs::create_dir_all(out_dir).map_err(|error| error.to_string())?;
    let profile = out_dir.join("browser-profile");
    std::fs::create_dir_all(&profile).map_err(|error| error.to_string())?;
    let profile_arg = format!("--user-data-dir={}", profile.to_string_lossy());
    let mut paths = Vec::new();
    for (index, offset) in offsets.iter().enumerate() {
        // Shift the body up instead of scrolling: a headless screenshot is
        // taken before/independently of scroll timing, so window.scrollTo
        // produced blank slices. A negative top offset is applied at paint.
        let shift = format!(
            "<style>html{{overflow:hidden}}body{{position:relative;top:-{offset}px}}</style>"
        );
        let slice = if html.contains("</head>") {
            html.replacen("</head>", &format!("{shift}</head>"), 1)
        } else if html.contains("</body>") {
            html.replacen("</body>", &format!("{shift}</body>"), 1)
        } else {
            format!("{html}{shift}")
        };
        let html_path = out_dir.join(format!("{stem}-slice-{index}.html"));
        std::fs::write(&html_path, slice.as_bytes()).map_err(|error| error.to_string())?;
        let png_path = out_dir.join(format!("{stem}-slice-{index}.png"));
        let url = format!("file:///{}", html_path.to_string_lossy().replace('\\', "/"));
        run_browser(
            &browser,
            &[
                "--headless=new".into(),
                "--disable-gpu".into(),
                "--hide-scrollbars".into(),
                "--allow-file-access-from-files".into(),
                "--virtual-time-budget=3000".into(),
                profile_arg.clone(),
                "--window-size=1440,1200".into(),
                format!("--screenshot={}", png_path.to_string_lossy()),
                url,
            ],
            Duration::from_secs(60),
        )
        .await?;
        if png_path.is_file() {
            paths.push(png_path.to_string_lossy().into_owned());
        }
    }
    if paths.is_empty() {
        return Err("切片渲染没有产出图片".into());
    }
    Ok(paths)
}
